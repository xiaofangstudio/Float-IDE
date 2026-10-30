/* Float Script 运行时：树遍历解释器。
 *
 * 结构：
 *   Env            作用域链（函数级作用域，方法内可裸名访问 self 字段）
 *   Runner         控制器：运行 / 断点 / 单步 / ask / wait，与界面通过 Hook 通信
 *
 * 全部求值函数都是 async generator，所以 `return f()`、`print(g(x))` 里的函数体
 * 即使包含 ask / wait 也能正确挂起再继续。
 */

import type { Expr, Program, Span, Stmt, Target, TypeName } from './ast';
import { parseProgram } from './parse';
import { getBuiltin, emptyDict } from './builtins';
import { compileProgram, CompiledRuntime, RuntimeError as CodeRuntimeError, StopSignal as CodeStopSignal, isCInst } from './codegen';
import { callBuiltinMethod, MethodError } from './methods';
import {
  cloneVal, collectFields, deepEq, dict, findMethod, fmtVal, instance, isDict, isInstance,
  isList, toNum, toStr, truthy, typeName,
  type ClassDef, type FuncDef, type Instance, type Val
} from './values';

/* 语言核心不该绑死 DOM：浏览器里是 window，Node（测试 / 以后可能的 CLI）里退到 globalThis。
 * 这样同一份运行时在任何宿主都能跑，不需要在测试里硬塞 window 桩。 */
type Host = { __floatSources?: Map<string, string> } & Record<string, unknown>;
function hostGlobal(): Host {
  return (typeof window !== 'undefined' ? window : globalThis) as unknown as Host;
}
function hostSetTimeout(fn: () => void, ms: number): void {
  if (typeof window !== 'undefined') { window.setTimeout(fn, ms); return; }
  setTimeout(fn, ms);
}

/** 函数调用最深允许叠多少层。超了就报「递归太深」，而不是让 JS 爆栈。 */
const MAX_CALL_DEPTH = 400;

export type OutLine = { kind: 'out' | 'err' | 'info'; text: string };
export type CtlState = 'idle' | 'running' | 'paused' | 'done';
export type VarsSnapshot = { name: string; type: string; value: string }[];

export type Yield =
  | { kind: 'stmt'; node: Stmt; depth: number }
  | { kind: 'ask'; question: string }
  | { kind: 'wait'; seconds: number };

export type Hook = {
  output: (l: OutLine) => void;
  onPaused: (node: Stmt | null, vars: VarsSnapshot) => void;
  onDone: () => void;
  onState: (s: CtlState) => void;
  /** 运行期错误（问题面板用） */
  onError?: (info: { message: string; file: string; line: number }) => void;
};

type Signal = 'normal' | 'break' | 'continue' | 'stop' | 'return';
type Next = { answer?: string };
type ValGen = AsyncGenerator<Yield, Val, Next>;
type SigGen = AsyncGenerator<Yield, Signal, Next>;

export class FloatError extends Error {
  constructor(message: string, public span?: Span, public frames: string[] = []) {
    super(message);
    this.name = 'FloatError';
  }
}

/** stop 指令穿过函数调用时的控制流信号（不是错误） */
class StopSignal extends Error {
  constructor() { super('脚本已停止'); this.name = 'StopSignal'; }
}

/* ───────────────────────────── 作用域 ───────────────────────────── */

class Env {
  vars = new Map<string, Val>();
  selfObj: Instance | null = null;

  constructor(public parent: Env | null, public label = 'local') {
    if (parent) this.selfObj = parent.selfObj;
  }

  lookup(name: string): Val | undefined {
    let e: Env | null = this;
    while (e) {
      if (e.vars.has(name)) return e.vars.get(name);
      e = e.parent;
    }
    if (this.selfObj && this.selfObj.fields.has(name)) return this.selfObj.fields.get(name);
    return undefined;
  }

  has(name: string): boolean {
    let e: Env | null = this;
    while (e) {
      if (e.vars.has(name)) return true;
      e = e.parent;
    }
    return !!(this.selfObj && this.selfObj.fields.has(name));
  }

  define(name: string, v: Val): void {
    this.vars.set(name, v);
  }

  assign(name: string, v: Val): void {
    let e: Env | null = this;
    while (e) {
      if (e.vars.has(name)) { e.vars.set(name, v); return; }
      e = e.parent;
    }
    // 方法内裸名写字段
    if (this.selfObj && this.selfObj.fields.has(name)) {
      this.selfObj.fields.set(name, v);
      return;
    }
    this.vars.set(name, v);
  }

  /** 快照：当前帧 → 全局 */
  chain(): { env: Env; label: string }[] {
    const out: { env: Env; label: string }[] = [];
    let e: Env | null = this;
    while (e) { out.push({ env: e, label: e.parent ? '局部' : '全局' }); e = e.parent; }
    return out;
  }
}

const SPECIAL_DEFAULTS: Record<string, Val> = {
  ans: '',
  __last_calc: 0,
  __last_cmp: false,
  __last_read: null,
  __last_len: 0,
  __last_str: '',
  __last_time: 0,
  __last_ans: ''
};

function defaultValue(type: TypeName | null): Val {
  switch (type) {
    case 'int':
    case 'float': return 0;
    case 'str': return '';
    case 'bool': return false;
    case 'list': return [];
    case 'dict': return emptyDict();
    default: return null;
  }
}

/* ───────────────────────────── 控制器 ───────────────────────────── */

export class Runner {
  state: CtlState = 'idle';
  pauseReason: 'debug' | 'wait' | 'ask' = 'debug';

  private gen: SigGen | null = null;
  private mode: 'run' | 'debug' = 'run';
  private stepMode: 'none' | 'into' | 'over' | 'out' = 'none';
  private stepFromDepth = 0;
  private pendingAsk: string | null = null;
  private waitSeconds = 0;

  private globals = new Env(null, 'global');
  private env: Env;
  private depth = 0;
  /** 函数调用叠了多少层（和上面的缩进 depth 是两回事，专管递归） */
  private callDepth = 0;
  private steps = 0;
  private time0 = 0;
  private retValue: Val = null;
  private frames: string[] = [];

  private funcs = new Map<string, FuncDef>();
  private classes = new Map<string, ClassDef>();
  /** 编译通道的运行时（运行模式用；调试模式为 null） */
  private crt: CompiledRuntime | null = null;
  /** 当前正在执行的方法所属的类，供 super(…) 找父类实现 */
  private ownerStack: (ClassDef | null)[] = [];
  private bps = new Set<string>();
  private programs = new Map<string, Program>();
  private imported = new Set<string>();
  private lastNode: Stmt | null = null;

  constructor(
    private entryFile: string,
    private hook: Hook,
    private importRoot: string,
    /** 显式指定的源码表（插件/一次性脚本用）。不传则用宿主的 __floatSources。
     *  有了它，插件跑一段代码就不会把主工程的源码表顶掉。 */
    private explicitSources: Map<string, string> | null = null
  ) {
    this.env = this.globals;
  }

  /* ── 对外接口 ── */

  setBreakpoints(file: string, lines: number[]): void {
    const norm = file.replace(/\\/g, '/');
    for (const k of [...this.bps]) if (k.startsWith(norm + ':')) this.bps.delete(k);
    for (const l of lines) this.bps.add(`${norm}:${l}`);
  }

  hasBreakpoints(): boolean { return this.bps.size > 0; }

  get askQuestion(): string | null { return this.pendingAsk; }

  varsSnapshot(): VarsSnapshot {
    if (this.crt) {
      const rt = this.crt;
      const out: VarsSnapshot = [];
      for (const [name, v] of Object.entries(rt.v)) {
        out.push({ name, type: typeName(v as Val), value: fmtVal(v as Val) });
      }
      if (rt.self) {
        for (const [name, v] of Object.entries(rt.self.f)) {
          if (name in rt.v) continue;
          out.push({ name: `self.${name}`, type: typeName(v), value: fmtVal(v) });
        }
      }
      out.sort((a, b) => Number(a.name.startsWith('__last')) - Number(b.name.startsWith('__last')));
      return out;
    }
    const seen = new Set<string>();
    const out: VarsSnapshot = [];
    for (const { env } of this.env.chain()) {
      for (const [name, v] of env.vars) {
        if (seen.has(name)) continue;
        seen.add(name);
        out.push({ name, type: typeName(v), value: fmtVal(v) });
      }
    }
    if (this.env.selfObj) {
      const inst = this.env.selfObj;
      for (const [name, v] of inst.fields) {
        const key = `self.${name}`;
        if (seen.has(name)) continue;
        seen.add(name);
        out.push({ name: key, type: typeName(v), value: fmtVal(v) });
      }
    }
    // 特殊变量放最后，避免刷屏
    out.sort((a, b) => Number(a.name.startsWith('__last')) - Number(b.name.startsWith('__last')));
    return out;
  }

  stop(): void {
    if (this.state === 'idle' || this.state === 'done') return;
    this.hook.output({ kind: 'info', text: '已手动停止' });
    this.gen = null;
    this.finish(true);
  }

  resume(answer?: string): void {
    if (!this.gen || this.state !== 'paused') return;
    this.setState('running');
    if (this.pendingAsk !== null) {
      this.pendingAsk = null;
      void this.pump(answer ?? '');
      return;
    }
    if (this.waitSeconds > 0) {
      const s = this.waitSeconds;
      this.waitSeconds = 0;
      hostSetTimeout(() => void this.pump(), Math.round(s * 1000));
      return;
    }
    void this.pump();
  }

  continueRun(): void {
    if (this.state !== 'paused') return;
    if (this.pendingAsk !== null) return;
    this.stepMode = 'none';
    this.resume();
  }

  setStep(mode: 'into' | 'over' | 'out'): void {
    if (this.state !== 'paused') return;
    if (this.pendingAsk !== null || this.waitSeconds > 0) return;
    this.stepMode = mode;
    this.stepFromDepth = this.depth;
    this.resume();
  }

  /* ── 启动 ── */

  start(mode: 'run' | 'debug'): void {
    this.mode = mode;
    // 调试模式先停在第一条语句，运行模式一路跑
    this.stepMode = mode === 'debug' ? 'into' : 'none';
    this.globals = new Env(null, 'global');
    this.env = this.globals;
    this.funcs.clear();
    this.classes.clear();
    this.programs.clear();
    this.imported.clear();
    this.steps = 0;
    this.depth = 0;
    this.callDepth = 0;
    this.retValue = null;
    this.frames = [];
    for (const [k, v] of Object.entries(SPECIAL_DEFAULTS)) this.globals.define(k, v);

    const sources = this.sources();
    const entry = this.entryFile.replace(/\\/g, '/');
    const src = sources.get(entry) ?? '';
    if (!src.trim()) {
      this.fail(new FloatError('入口文件没有内容，无法运行'));
      return;
    }

    this.time0 = performance.now();

    let program: Program;
    try {
      program = parseProgram(entry, src);
      this.programs.set(entry, program);
    } catch (e) {
      this.fail(e);
      return;
    }

    const errors = program.diagnostics.filter((d) => d.severity === 'error');
    if (errors.length) {
      this.setState('running');
      this.hook.output({ kind: 'err', text: `发现 ${errors.length} 处语法错误，先改好再运行：` });
      for (const d of errors.slice(0, 12)) {
        this.hook.output({
          kind: 'err',
          text: `  ${this.shortFile(d.span.file)} 第 ${d.span.line} 行第 ${d.span.col} 列：${d.message}`
        });
      }
      if (errors.length > 12) this.hook.output({ kind: 'err', text: `  ……还有 ${errors.length - 12} 处` });
      this.finish(true);
      return;
    }
    for (const d of program.diagnostics) {
      if (d.severity === 'warning') {
        this.hook.output({ kind: 'info', text: `提示：${this.shortFile(d.span.file)} 第 ${d.span.line} 行 ${d.message}` });
      }
    }

    try {
      this.preregister(program.body);
    } catch (e) {
      this.fail(e);
      return;
    }

    this.preregister(program.body);

    this.setState('running');

    // 运行模式：编译成 JS 交给 V8；编译失败或调试模式：走解释器
    this.crt = null;
    this.gen = null;
    if (mode === 'run') {
      const unit = compileProgram(program);
      if (unit) {
        const rt = new CompiledRuntime();
        for (const [k, v] of Object.entries(SPECIAL_DEFAULTS)) rt.v[k] = v;
        rt.file = entry;
        rt.out = (line) => this.hook.output({ kind: 'out', text: line });
        this.crt = rt;
        this.gen = unit.main(rt) as unknown as SigGen;
        this.hook.output({ kind: 'info', text: '运行开始（编译执行）' });
      }
    }
    if (!this.gen) {
      this.hook.output({
        kind: 'info',
        text: mode === 'debug'
          ? '调试开始（解释执行 · F8 继续 · F10 跳过 · F11 进入 · Shift+F11 跳出）'
          : '运行开始（解释执行）'
      });
      this.gen = this.execBlock(program.body, this.globals, 0);
    }
    this.lastNode = null;
    void this.pump();
  }

  /* ── 推进生成器 ── */

  private async pump(answer?: string): Promise<void> {
    if (!this.gen) return;
    for (;;) {
      let r: IteratorResult<Yield, Signal>;
      try {
        r = await this.gen.next(answer === undefined ? {} : { answer });
      } catch (e) {
        if (e instanceof StopSignal || e instanceof CodeStopSignal) {
          this.hook.output({ kind: 'info', text: '脚本已按 stop 指令停止' });
          this.gen = null;
          this.finish(true);
          return;
        }
        this.fail(e);
        return;
      }
      answer = undefined;
      if (r.done) { this.finish(); return; }

      const y = r.value as Yield & { q?: string };
      if (y.kind === 'ask') {
        this.pauseReason = 'ask';
        this.pendingAsk = y.q ?? (y as { question?: string }).question ?? '';
        this.setState('paused');
        this.hook.onPaused(this.lastNode, this.varsSnapshot());
        return;
      }
      if (y.kind === 'wait') {
        this.pauseReason = 'wait';
        this.waitSeconds = Math.max(0, y.seconds);
        this.setState('paused');
        this.hook.onPaused(null, this.varsSnapshot());
        return;
      }

      this.steps++;
      if (this.steps > 2_000_000) {
        this.fail(new FloatError('执行步数超过 200 万，脚本可能陷入死循环（用 out / break / stop 结束）'));
        return;
      }
      this.lastNode = y.node;
      this.depth = y.depth;

      if (this.mode === 'debug' && this.shouldPause(y)) {
        this.pauseReason = 'debug';
        this.setState('paused');
        this.hook.onPaused(y.node, this.varsSnapshot());
        return;
      }
    }
  }

  private shouldPause(y: Yield & { kind: 'stmt' }): boolean {
    const key = `${y.node.file.replace(/\\/g, '/')}:${y.node.line}`;
    if (this.bps.has(key)) return true;
    if (this.stepMode === 'into') return true;
    if (this.stepMode === 'none') return false;
    if (this.stepMode === 'over') return y.depth <= this.stepFromDepth;
    if (this.stepMode === 'out') return y.depth < this.stepFromDepth;
    return false;
  }

  private setState(s: CtlState): void {
    this.state = s;
    this.hook.onState(s);
  }

  private fail(e: unknown): void {
    // 编译通道的错误带自己的行号，转成统一的 FloatError
    if (e instanceof CodeRuntimeError) {
      e = new FloatError(e.message, {
        file: this.crt?.file ?? this.entryFile,
        line: e.line || 1, col: 1, endLine: e.line || 1, endCol: 1
      }, e.frames);
    }
    const msg = e instanceof FloatError
      ? this.formatError(e)
      : e instanceof Error ? e.message : String(e);
    this.hook.output({ kind: 'err', text: msg });
    this.hook.onError?.({
      message: e instanceof Error ? e.message : String(e),
      file: e instanceof FloatError && e.span ? e.span.file : (this.crt?.file ?? this.entryFile),
      line: e instanceof FloatError && e.span ? e.span.line : (this.crt?.ln ?? 1)
    });
    this.gen = null;
    this.finish(true);
  }

  private formatError(e: FloatError): string {
    const where = e.span ? `（${this.shortFile(e.span.file)} 第 ${e.span.line} 行）` : '';
    const lines = [`出错${where}：${e.message}`];
    for (const f of e.frames) lines.push(`    ${f}`);
    return lines.join('\n');
  }

  private finish(aborted = false): void {
    if (this.crt) this.steps = this.crt.steps;
    if (!aborted) {
      const secs = (performance.now() - this.time0) / 1000;
      this.hook.output({ kind: 'info', text: `结束：${this.steps} 步 · ${secs.toFixed(2)} 秒` });
    }
    this.setState('done');
    this.hook.onPaused(null, this.varsSnapshot());
    this.hook.onDone();
  }

  private shortFile(file: string): string {
    const norm = file.replace(/\\/g, '/');
    return norm.split('/').pop() ?? norm;
  }

  private sources(): Map<string, string> {
    if (this.explicitSources) return this.explicitSources;
    const host = hostGlobal();
    const m = host.__floatSources;
    if (!m) {
      const empty = new Map<string, string>();
      host.__floatSources = empty;
      return empty;
    }
    return m;
  }

  /* ── 预注册：顶层函数与类先就位，允许"先用后定义" ── */

  private preregister(body: Stmt[]): void {
    for (const s of body) {
      if (s.k === 'funcDef') this.funcs.set(s.name, s);
      else if (s.k === 'classDef') this.registerClass(s);
    }
  }

  private registerClass(s: Extract<Stmt, { k: 'classDef' }>): ClassDef {
    const existing = this.classes.get(s.name);
    if (existing) return existing;
    const base = s.base ? this.classes.get(s.base) ?? null : null;
    if (s.base && !base) {
      throw new FloatError(`父类「${s.base}」还没有定义（先写父类，再写子类）`, s);
    }
    const fields = new Map<string, { init: Expr | null; type: TypeName | null }>();
    for (const f of s.fields) fields.set(f.name, { init: f.init, type: f.type });
    const methods = new Map<string, FuncDef>();
    for (const m of s.methods) {
      if (m.k === 'funcDef') methods.set(m.name, m);
    }
    const def: ClassDef = { name: s.name, base, fields, methods, file: s.file, line: s.line };
    this.classes.set(s.name, def);
    return def;
  }

  /* ── 语句执行 ── */

  private async *execBlock(body: Stmt[], env: Env, depth: number): SigGen {
    for (const s of body) {
      const sig = yield* this.execStmt(s, env, depth);
      if (sig !== 'normal') return sig;
    }
    return 'normal';
  }

  private async *execStmt(s: Stmt, env: Env, depth: number): SigGen {
    this.env = env;

    switch (s.k) {
      case 'comment':
        return 'normal';

      case 'unknown': {
        const hint = s.hint ? `（是不是想写 ${s.hint}？）` : '';
        throw new FloatError(`无法识别的语句「${s.raw}」${hint}`, s);
      }

      case 'funcDef':
        this.funcs.set(s.name, s);
        return 'normal';

      case 'classDef':
        this.registerClass(s);
        return 'normal';

      case 'import': {
        const program = await this.loadImport(s.target, s.file, s);
        yield { kind: 'stmt', node: s, depth };
        if (!program) return 'normal';
        this.preregister(program.body);
        const sig = yield* this.execBlock(program.body, this.globals, depth);
        if (sig === 'stop') return sig;
        if (sig === 'return') return 'normal';
        return 'normal';
      }

      case 'break':
        yield { kind: 'stmt', node: s, depth };
        return 'break';

      case 'continue':
        yield { kind: 'stmt', node: s, depth };
        return 'continue';

      case 'stop':
        yield { kind: 'stmt', node: s, depth };
        return 'stop';

      case 'return': {
        yield { kind: 'stmt', node: s, depth };
        this.retValue = s.value ? yield* this.evalExpr(s.value, env) : null;
        return 'return';
      }

      case 'decl': {
        yield { kind: 'stmt', node: s, depth };
        const v = s.init ? yield* this.evalExpr(s.init, env) : defaultValue(s.type);
        env.define(s.name, v);
        return 'normal';
      }

      case 'assign': {
        yield { kind: 'stmt', node: s, depth };
        const value = yield* this.evalExpr(s.value, env);
        yield* this.assignTo(s.target, s.op, value, env, s);
        return 'normal';
      }

      case 'incdec': {
        yield { kind: 'stmt', node: s, depth };
        const cur = yield* this.readTarget(s.target, env, s);
        const n = toNum(cur);
        if (n === null) throw new FloatError(`「${this.targetName(s.target)}」不是数字，不能 ${s.delta > 0 ? '++' : '--'}`, s);
        yield* this.assignTo(s.target, '=', n + s.delta, env, s);
        return 'normal';
      }

      case 'expr': {
        yield { kind: 'stmt', node: s, depth };
        yield* this.execExprStmt(s.expr, env, depth, s);
        return 'normal';
      }

      case 'print': {
        yield { kind: 'stmt', node: s, depth };
        const parts: string[] = [];
        for (const a of s.args) parts.push(toStr(yield* this.evalExpr(a, env)));
        this.hook.output({ kind: 'out', text: parts.join(' ') });
        return 'normal';
      }

      case 'ask': {
        yield { kind: 'stmt', node: s, depth };
        const q = toStr(yield* this.evalExpr(s.question, env));
        const answer = yield { kind: 'ask', question: q };
        const ans = answer?.answer ?? '';
        env.assign('ans', ans);
        this.globals.assign('ans', ans);
        this.globals.assign('__last_ans', ans);
        return 'normal';
      }

      case 'if': {
        yield { kind: 'stmt', node: s, depth };
        for (const br of s.branches) {
          const ok = br.cond === null ? true : truthy(yield* this.evalExpr(br.cond, env));
          if (ok) return yield* this.execBlock(br.body, env, depth);
        }
        if (s.elseBody) return yield* this.execBlock(s.elseBody, env, depth);
        return 'normal';
      }

      case 'while': {
        yield { kind: 'stmt', node: s, depth };
        return yield* this.loopWhile(s, env, depth);
      }

      case 'waitUntil': {
        yield { kind: 'stmt', node: s, depth };
        let guard = 0;
        for (;;) {
          if (truthy(yield* this.evalExpr(s.cond, env))) return 'normal';
          if (++guard > 1_000_000) throw new FloatError('wait until 循环超过 100 万次，可能死循环', s);
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig === 'break') return 'normal';
          if (sig === 'continue') continue;
          if (sig !== 'normal') return sig;
        }
      }

      case 'repeat': {
        yield { kind: 'stmt', node: s, depth };
        const n = toNum(yield* this.evalExpr(s.count, env));
        if (n === null) throw new FloatError('repeat 的次数必须是数字', s);
        const times = Math.trunc(n);
        if (times > 1_000_000) throw new FloatError('repeat 次数超过 100 万，可能死循环', s);
        for (let k = 0; k < times; k++) {
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig === 'break') break;
          if (sig === 'continue') continue;
          if (sig !== 'normal') return sig;
        }
        return 'normal';
      }

      case 'forever': {
        yield { kind: 'stmt', node: s, depth };
        let guard = 0;
        for (;;) {
          if (++guard > 1_000_000) throw new FloatError('forever 循环超过 100 万次，可能死循环', s);
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig === 'break') break;
          if (sig === 'continue') continue;
          if (sig !== 'normal') return sig;
        }
        return 'normal';
      }

      case 'forRange': {
        yield { kind: 'stmt', node: s, depth };
        const from = s.from ? toNum(yield* this.evalExpr(s.from, env)) : 0;
        if (from === null) throw new FloatError('for 的起点必须是数字', s);
        const to = toNum(yield* this.evalExpr(s.to, env));
        if (to === null) throw new FloatError('for 的终点必须是数字', s);
        const step = s.step ? toNum(yield* this.evalExpr(s.step, env)) : 1;
        if (step === null || step === 0) throw new FloatError('for 的步长必须是非 0 数字', s);

        const loopEnv = new Env(env);
        const end = s.inclusive ? to + (step > 0 ? 1e-9 : -1e-9) : to;
        let guard = 0;
        for (let v = from; step > 0 ? v < end : v > end; v += step) {
          if (++guard > 1_000_000) throw new FloatError('for 循环超过 100 万次，可能死循环', s);
          loopEnv.define(s.varName, v);
          this.env = loopEnv;
          const sig = yield* this.execBlock(s.body, loopEnv, depth);
          if (sig === 'break') break;
          if (sig === 'continue') continue;
          if (sig !== 'normal') return sig;
        }
        return 'normal';
      }

      case 'forIn': {
        yield { kind: 'stmt', node: s, depth };
        const iter = yield* this.evalExpr(s.iter, env);
        const pairs: [Val, Val][] = [];
        if (isList(iter)) {
          if (s.valueName) throw new FloatError('for a, b in … 只能用在字典上', s);
          for (let k = 0; k < iter.length; k++) pairs.push([k, iter[k]]);
        } else if (typeof iter === 'string') {
          if (s.valueName) throw new FloatError('for a, b in … 只能用在字典上', s);
          for (let k = 0; k < iter.length; k++) pairs.push([k, iter[k]]);
        } else if (isDict(iter)) {
          for (const [k, v] of iter.map) pairs.push([k, v]);
        } else {
          throw new FloatError(`for … in 只能遍历列表、字符串或字典，得到 ${fmtVal(iter)}`, s);
        }
        if (pairs.length > 1_000_000) throw new FloatError('要遍历的集合超过 100 万个元素', s);

        const loopEnv = new Env(env);
        for (const [k, v] of pairs) {
          if (s.valueName) {
            loopEnv.define(s.varName, k);
            loopEnv.define(s.valueName, v);
          } else {
            loopEnv.define(s.varName, isDict(iter) ? k : v);
          }
          this.env = loopEnv;
          const sig = yield* this.execBlock(s.body, loopEnv, depth);
          if (sig === 'break') break;
          if (sig === 'continue') continue;
          if (sig !== 'normal') return sig;
        }
        return 'normal';
      }

      case 'throw': {
        yield { kind: 'stmt', node: s, depth };
        const v = yield* this.evalExpr(s.value, env);
        throw new FloatError(this.errMessageOf(v), s);
      }

      case 'try': {
        yield { kind: 'stmt', node: s, depth };
        try {
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig !== 'normal') {
            if (s.finallyBody) yield* this.execBlock(s.finallyBody, env, depth);
            return sig;
          }
        } catch (e) {
          if (!s.catchBody) {
            if (s.finallyBody) yield* this.execBlock(s.finallyBody, env, depth);
            throw e;
          }
          if (s.catchName) env.define(s.catchName, this.errorObjectOf(e));
          const sig = yield* this.execBlock(s.catchBody, env, depth);
          if (s.finallyBody) yield* this.execBlock(s.finallyBody, env, depth);
          return sig === 'normal' ? 'normal' : sig;
        }
        if (s.finallyBody) yield* this.execBlock(s.finallyBody, env, depth);
        return 'normal';
      }

      case 'destructure': {
        yield { kind: 'stmt', node: s, depth };
        const v = yield* this.evalExpr(s.value, env);
        const arr: Val[] | null = isList(v) ? v : (typeof v === 'string' ? v.split('') : null);
        if (!arr) throw new FloatError('解构赋值右边要是列表', s);
        if (arr.length < s.names.length) {
          throw new FloatError(`右边只有 ${arr.length} 个值，左边要 ${s.names.length} 个`, s);
        }
        s.names.forEach((n, idx) => env.assign(n.name, arr[idx]));
        return 'normal';
      }

      case 'wait': {
        const secs = toNum(yield* this.evalExpr(s.seconds, env));
        if (secs === null) throw new FloatError('wait 需要秒数', s);
        yield { kind: 'stmt', node: s, depth };
        yield { kind: 'wait', seconds: secs };
        return 'normal';
      }

      case 'ifOut': {
        yield { kind: 'stmt', node: s, depth };
        return truthy(yield* this.evalExpr(s.cond, env)) ? 'break' : 'normal';
      }

      case 'listAdd': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`「${this.targetName(s.target)}」不是列表`, s);
        target.push(cloneVal(yield* this.evalExpr(s.value, env)));
        return 'normal';
      }

      case 'listInsert': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`「${this.targetName(s.target)}」不是列表`, s);
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError('插入位置必须是数字', s);
        const i = Math.trunc(at);
        if (i < 0 || i > target.length) throw new FloatError(`插入位置 ${i} 超出范围（长度 ${target.length}）`, s);
        target.splice(i, 0, cloneVal(yield* this.evalExpr(s.value, env)));
        return 'normal';
      }

      case 'listReplace': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`「${this.targetName(s.target)}」不是列表`, s);
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError('下标必须是数字', s);
        const i = Math.trunc(at);
        if (i < 0 || i >= target.length) throw new FloatError(`下标 ${i} 超出范围（长度 ${target.length}）`, s);
        target[i] = cloneVal(yield* this.evalExpr(s.value, env));
        return 'normal';
      }

      case 'listDelete': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`「${this.targetName(s.target)}」不是列表`, s);
        if (!s.index) { target.length = 0; return 'normal'; }
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError('下标必须是数字', s);
        const i = Math.trunc(at);
        if (i < 0 || i >= target.length) throw new FloatError(`下标 ${i} 超出范围（长度 ${target.length}）`, s);
        target.splice(i, 1);
        return 'normal';
      }

      case 'listRead': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError('下标必须是数字', s);
        const i = Math.trunc(at);
        if (isList(target)) {
          const idx = i < 0 ? target.length + i : i;
          if (idx < 0 || idx >= target.length) throw new FloatError(`下标 ${i} 超出范围（长度 ${target.length}）`, s);
          this.globals.assign('__last_read', target[idx]);
          return 'normal';
        }
        if (typeof target === 'string') {
          const idx = i < 0 ? target.length + i : i;
          if (idx < 0 || idx >= target.length) throw new FloatError(`字符下标 ${i} 超出范围（长度 ${target.length}）`, s);
          this.globals.assign('__last_read', target[idx]);
          return 'normal';
        }
        throw new FloatError(`「${this.targetName(s.target)}」不是列表`, s);
      }

      case 'lenOf': {
        yield { kind: 'stmt', node: s, depth };
        const v = yield* this.evalExpr(s.value, env);
        const len = isList(v) ? v.length
          : typeof v === 'string' ? v.length
          : isDict(v) ? v.map.size
          : isInstance(v) ? v.fields.size
          : -1;
        if (len < 0) throw new FloatError(`len 只接受列表 / 字符串 / 字典，得到 ${fmtVal(v)}`, s);
        this.globals.assign('__last_len', len);
        return 'normal';
      }

      case 'dictSet': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isDict(target)) throw new FloatError(`「${this.targetName(s.target)}」不是字典`, s);
        const key = toStr(yield* this.evalExpr(s.key, env));
        target.map.set(key, cloneVal(yield* this.evalExpr(s.value, env)));
        return 'normal';
      }

      case 'dictRead': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isDict(target)) throw new FloatError(`「${this.targetName(s.target)}」不是字典`, s);
        const key = toStr(yield* this.evalExpr(s.key, env));
        if (!target.map.has(key)) throw new FloatError(`字典「${this.targetName(s.target)}」里没有键「${key}」`, s);
        this.globals.assign('__last_read', target.map.get(key) as Val);
        return 'normal';
      }

      case 'dictDelete': {
        yield { kind: 'stmt', node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isDict(target)) throw new FloatError(`「${this.targetName(s.target)}」不是字典`, s);
        target.map.delete(toStr(yield* this.evalExpr(s.key, env)));
        return 'normal';
      }

      case 'timeRead': {
        yield { kind: 'stmt', node: s, depth };
        this.globals.assign('__last_time', (performance.now() - this.time0) / 1000);
        return 'normal';
      }

      case 'timeReset': {
        yield { kind: 'stmt', node: s, depth };
        this.time0 = performance.now();
        return 'normal';
      }
    }
  }

  /** 把任意错误转成脚本里可读的字典：e.message / e.line / e.file */
  private errorObjectOf(e: unknown): Val {
    const msg = e instanceof FloatError ? e.message : e instanceof Error ? e.message : String(e);
    const span = e instanceof FloatError ? e.span : undefined;
    const d = dict();
    d.map.set('message', msg);
    d.map.set('line', span?.line ?? 0);
    d.map.set('file', span?.file ?? '');
    return d;
  }

  private errMessageOf(v: Val): string {
    if (isDict(v) && v.map.has('message')) return toStr(v.map.get('message') as Val);
    return toStr(v);
  }

  private async *loopWhile(s: Extract<Stmt, { k: 'while' }>, env: Env, depth: number): SigGen {
    let guard = 0;
    for (;;) {
      if (!truthy(yield* this.evalExpr(s.cond, env))) break;
      if (++guard > 1_000_000) throw new FloatError('while 循环超过 100 万次，可能死循环', s);
      const sig = yield* this.execBlock(s.body, env, depth);
      if (sig === 'break') break;
      if (sig === 'continue') continue;
      if (sig !== 'normal') return sig;
    }
    return 'normal';
  }

  /** 表达式语句：独立成行的内建调用仍然写 __last_*，兼容 CW Script 老写法 */
  private async *execExprStmt(e: Expr, env: Env, depth: number, s: Stmt): ValGen {
    if (e.k === 'var') {
      if (e.name === 'ans') {
        this.globals.assign('__last_ans', env.lookup('ans') ?? '');
        return null;
      }
      const v = env.lookup(e.name);
      if (v === undefined) throw new FloatError(`变量「${e.name}」没有定义`, s);
      return v;
    }
    if (e.k === 'call' && e.callee.k === 'var') {
      const name = e.callee.name;
      const b = getBuiltin(name);
      if (b) {
        const args = yield* this.evalArgs(e.args, env);
        const v = this.callBuiltin(name, args, s);
        if (b.writes) this.globals.assign(b.writes, v);
        return v;
      }
    }
    return yield* this.evalExpr(e, env);
  }

  private async *evalArgs(args: Expr[], env: Env): AsyncGenerator<Yield, Val[], Next> {
    const out: Val[] = [];
    for (const a of args) out.push(yield* this.evalExpr(a, env));
    return out;
  }

  /* ── 表达式 ── */

  private async *evalExpr(e: Expr, env: Env): ValGen {
    switch (e.k) {
      case 'num': return e.v;
      case 'str': return e.v;
      case 'bool': return e.v;
      case 'null': return null;
      case 'list': {
        const items: Val[] = [];
        for (const it of e.items) items.push(yield* this.evalExpr(it, env));
        return items;
      }
      case 'dict': {
        const d = dict();
        for (const { key, val } of e.entries) {
          d.map.set(toStr(yield* this.evalExpr(key, env)), yield* this.evalExpr(val, env));
        }
        return d;
      }
      case 'var': {
        const v = env.lookup(e.name);
        if (v !== undefined) return v;
        if (this.classes.has(e.name)) throw new FloatError(`「${e.name}」是类名，要创建实例请写 new ${e.name}(…)`, e);
        if (this.funcs.has(e.name)) throw new FloatError(`「${e.name}」是函数，调用要写 ${e.name}(…)`, e);
        if (env.selfObj && env.selfObj.fields.has(e.name)) return env.selfObj.fields.get(e.name) as Val;
        throw new FloatError(`变量「${e.name}」没有定义（先声明或赋值）`, e);
      }
      case 'self': {
        const obj = env.selfObj;
        if (!obj) throw new FloatError('self 只能在类的方法里使用', e);
        return obj;
      }
      case 'un': {
        const v = yield* this.evalExpr(e.a, env);
        if (e.op === '-') { const n = toNum(v); if (n === null) throw new FloatError(`一元 - 需要数字，得到 ${fmtVal(v)}`, e); return -n; }
        if (e.op === '+') { const n = toNum(v); if (n === null) throw new FloatError(`一元 + 需要数字，得到 ${fmtVal(v)}`, e); return n; }
        if (e.op === '!') return !truthy(v);
        if (e.op === '~') { const n = toNum(v); if (n === null) throw new FloatError('~ 需要数字', e); return ~n; }
        throw new FloatError(`未知一元运算符 ${e.op}`, e);
      }
      case 'bin': return yield* this.evalBin(e, env);
      case 'idx': {
        const obj = yield* this.evalExpr(e.obj, env);
        const idx = yield* this.evalExpr(e.index, env);
        return this.indexGet(obj, idx, e);
      }
      case 'member': {
        const obj = yield* this.evalExpr(e.obj, env);
        return this.memberGet(obj, e.name, e);
      }
      case 'call': {
        const args = yield* this.evalArgs(e.args, env);
        if (e.callee.k === 'var') {
          const name = e.callee.name;
          // super(…)：调用父类的同名初始化
          if (name === 'super') {
            const owner = this.ownerStack[this.ownerStack.length - 1] ?? null;
            const self = env.selfObj;
            if (!owner || !owner.base || !self) throw new FloatError('super(…) 只能在子类方法里使用', e);
            const init = findMethod(owner.base, 'init') ?? findMethod(owner.base, '__init__');
            if (!init) throw new FloatError(`父类 ${owner.base.name} 没有 init，无需 super(…)`, e);
            return yield* this.callUser(init.fn, args, self, e, init.owner);
          }
          const b = getBuiltin(name);
          if (b) return this.callBuiltin(name, args, e);
          // 方法内裸名调用本类方法
          if (env.selfObj) {
            const m = findMethod(env.selfObj.cls, name);
            if (m) return yield* this.callUser(m.fn, args, env.selfObj, e, m.owner);
          }
          const fn = this.funcs.get(name);
          if (fn) return yield* this.callUser(fn, args, null, e);
          const cls = this.classes.get(name);
          if (cls) return yield* this.instantiate(cls, args, e);
          throw new FloatError(`函数「${name}」没有定义`, e);
        }
        if (e.callee.k === 'member') {
          const obj = yield* this.evalExpr(e.callee.obj, env);
          // 内建方法：s.upper() / xs.push(1) / d.keys()
          try {
            const m = callBuiltinMethod(obj, e.callee.name, args);
            if (m.handled) return m.value;
          } catch (err) {
            if (err instanceof MethodError) throw new FloatError(err.message, e);
            throw err;
          }
          if (isInstance(obj)) {
            const found = findMethod(obj.cls, e.callee.name);
            if (!found) throw new FloatError(`类 ${obj.cls.name} 没有方法「${e.callee.name}」`, e);
            return yield* this.callUser(found.fn, args, obj, e, found.owner);
          }
          if (isDict(obj)) {
            const v = obj.map.get(e.callee.name);
            if (v === undefined) throw new FloatError(`字典里没有键「${e.callee.name}」，它也不是可调用的方法`, e);
            throw new FloatError(`字典的键「${e.callee.name}」不是函数，不能调用`, e);
          }
          throw new FloatError(`这个值没有方法「${e.callee.name}」：${fmtVal(obj)}`, e);
        }
        throw new FloatError('这个表达式不能被调用', e);
      }
      case 'new': {
        const cls = this.classes.get(e.cls);
        if (!cls) throw new FloatError(`类「${e.cls}」没有定义`, e);
        const args = yield* this.evalArgs(e.args, env);
        return yield* this.instantiate(cls, args, e);
      }
    }
  }

  private async *evalBin(e: Extract<Expr, { k: 'bin' }>, env: Env): ValGen {
    if (e.op === '&&') {
      return truthy(yield* this.evalExpr(e.a, env)) ? truthy(yield* this.evalExpr(e.b, env)) : false;
    }
    if (e.op === '||') {
      return truthy(yield* this.evalExpr(e.a, env)) ? true : truthy(yield* this.evalExpr(e.b, env));
    }
    const a = yield* this.evalExpr(e.a, env);
    const b = yield* this.evalExpr(e.b, env);

    switch (e.op) {
      case '+':
        if (typeof a === 'string' || typeof b === 'string') return toStr(a) + toStr(b);
        if (isList(a) && isList(b)) return [...a, ...b];
        return this.numOr(a, '+', e) + this.numOr(b, '+', e);
      case '-': return this.numOr(a, '-', e) - this.numOr(b, '-', e);
      case '*':
        if (typeof a === 'string' && typeof b === 'number') return a.repeat(Math.max(0, Math.trunc(b)));
        return this.numOr(a, '*', e) * this.numOr(b, '*', e);
      case '/': {
        const d = this.numOr(b, '/', e);
        if (d === 0) throw new FloatError('除数不能为 0', e);
        return this.numOr(a, '/', e) / d;
      }
      case '%': {
        const d = this.numOr(b, '%', e);
        if (d === 0) throw new FloatError('取模的除数不能为 0', e);
        return this.numOr(a, '%', e) % d;
      }
      case '**': return Math.pow(this.numOr(a, '**', e), this.numOr(b, '**', e));
      case '==': return deepEq(a, b);
      case '===': return deepEq(a, b) && typeof a === typeof b;
      case '!=': return !deepEq(a, b);
      case '!==': return !(deepEq(a, b) && typeof a === typeof b);
      case '<': return this.less(a, b, e);
      case '>': return this.less(b, a, e);
      case '<=': return !this.less(b, a, e);
      case '>=': return !this.less(a, b, e);
      case '&': return this.intOr(a, '&', e) & this.intOr(b, '&', e);
      case '|': return this.intOr(a, '|', e) | this.intOr(b, '|', e);
      case '^': return this.intOr(a, '^', e) ^ this.intOr(b, '^', e);
      case '<<': return this.intOr(a, '<<', e) << this.intOr(b, '<<', e);
      case '>>': return this.intOr(a, '>>', e) >> this.intOr(b, '>>', e);
    }
    throw new FloatError(`未知运算符 ${e.op}`, e);
  }

  private numOr(v: Val, op: string, e: Span | Expr): number {
    const n = toNum(v);
    if (n === null) throw new FloatError(`运算符「${op}」需要数字，得到 ${fmtVal(v)}`, spanOf(e));
    return n;
  }

  private intOr(v: Val, op: string, e: Span | Expr): number {
    return Math.trunc(this.numOr(v, op, e));
  }

  private less(a: Val, b: Val, e: Span | Expr): boolean {
    if (typeof a === 'string' && typeof b === 'string') return a < b;
    const x = toNum(a);
    const y = toNum(b);
    if (x === null || y === null) throw new FloatError(`比较大小需要两个数字或两个字符串，得到 ${fmtVal(a)} 与 ${fmtVal(b)}`, spanOf(e));
    return x < y;
  }

  private callBuiltin(name: string, args: Val[], e: Span | Expr): Val {
    const b = getBuiltin(name);
    if (!b) throw new FloatError(`内建函数「${name}」不存在`, spanOf(e));
    if (args.length < b.min || args.length > b.max) {
      const range = b.min === b.max ? `${b.min}` : `${b.min}~${b.max}`;
      throw new FloatError(`${b.sig} 需要 ${range} 个参数，实际给了 ${args.length} 个`, spanOf(e));
    }
    try {
      return b.fn(args);
    } catch (err) {
      throw new FloatError(err instanceof Error ? err.message : String(err), spanOf(e));
    }
  }

  private indexGet(obj: Val, idx: Val, e: Span | Expr): Val {
    if (isList(obj)) {
      const n = toNum(idx);
      if (n === null) throw new FloatError(`列表下标必须是数字，得到 ${fmtVal(idx)}`, spanOf(e));
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new FloatError(`下标 ${i} 超出范围（长度 ${obj.length}）`, spanOf(e));
      return obj[real];
    }
    if (typeof obj === 'string') {
      const n = toNum(idx);
      if (n === null) throw new FloatError(`字符下标必须是数字，得到 ${fmtVal(idx)}`, spanOf(e));
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new FloatError(`字符下标 ${i} 超出范围（长度 ${obj.length}）`, spanOf(e));
      return obj[real];
    }
    if (isDict(obj)) {
      const key = toStr(idx);
      if (!obj.map.has(key)) throw new FloatError(`字典里没有键「${key}」`, spanOf(e));
      return obj.map.get(key) as Val;
    }
    throw new FloatError(`这个值不能用下标访问：${fmtVal(obj)}`, spanOf(e));
  }

  private memberGet(obj: Val, name: string, e: Span | Expr): Val {
    if (isInstance(obj)) {
      if (obj.fields.has(name)) return obj.fields.get(name) as Val;
      if (findMethod(obj.cls, name)) {
        throw new FloatError(`「${name}」是 ${obj.cls.name} 的方法，要调用请写 ${name}(…)`, spanOf(e));
      }
      throw new FloatError(`${obj.cls.name} 没有字段「${name}」`, spanOf(e));
    }
    if (isDict(obj)) {
      if (!obj.map.has(name)) throw new FloatError(`字典里没有键「${name}」`, spanOf(e));
      return obj.map.get(name) as Val;
    }
    if (name === 'length' && (isList(obj) || typeof obj === 'string')) {
      return obj.length;
    }
    throw new FloatError(`这个值没有成员「${name}」：${fmtVal(obj)}`, spanOf(e));
  }

  private targetName(t: Target): string {
    if (t.k === 'var') return t.name;
    if (t.k === 'member') return `.${t.name}`;
    return '下标';
  }

  private async *readTarget(t: Target, env: Env, s: Stmt): ValGen {
    if (t.k === 'var') {
      const v = env.lookup(t.name);
      if (v === undefined) throw new FloatError(`变量「${t.name}」没有定义`, s);
      return v;
    }
    if (t.k === 'member') {
      const obj = yield* this.evalExpr(t.obj, env);
      return this.memberGet(obj, t.name, s);
    }
    const obj = yield* this.evalExpr(t.obj, env);
    const idx = yield* this.evalExpr(t.index, env);
    return this.indexGet(obj, idx, s);
  }

  private async *assignTo(t: Target, op: string, value: Val, env: Env, s: Stmt): SigGen {
    let next = value;
    if (op !== '=') {
      const cur = yield* this.readTarget(t, env, s);
      next = this.applyCompound(op, cur, value, s);
    }
    if (t.k === 'var') {
      env.assign(t.name, next);
      return 'normal';
    }
    if (t.k === 'member') {
      const obj = yield* this.evalExpr(t.obj, env);
      if (isInstance(obj)) {
        obj.fields.set(t.name, next);
        return 'normal';
      }
      if (isDict(obj)) {
        obj.map.set(t.name, next);
        return 'normal';
      }
      throw new FloatError(`不能给 ${fmtVal(obj)} 的成员赋值`, s);
    }
    const obj = yield* this.evalExpr(t.obj, env);
    const idx = yield* this.evalExpr(t.index, env);
    if (isList(obj)) {
      const n = toNum(idx);
      if (n === null) throw new FloatError('列表下标必须是数字', s);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new FloatError(`下标 ${i} 超出范围（长度 ${obj.length}）`, s);
      obj[real] = next;
      return 'normal';
    }
    if (isDict(obj)) {
      obj.map.set(toStr(idx), next);
      return 'normal';
    }
    throw new FloatError(`这个值不能按下标赋值：${fmtVal(obj)}`, s);
  }

  private applyCompound(op: string, cur: Val, rhs: Val, s: Stmt): Val {
    switch (op) {
      case '+=':
        if (typeof cur === 'string' || typeof rhs === 'string') return toStr(cur) + toStr(rhs);
        if (isList(cur) && isList(rhs)) return [...cur, ...rhs];
        return this.numOr(cur, '+=', s) + this.numOr(rhs, '+=', s);
      case '-=': return this.numOr(cur, '-=', s) - this.numOr(rhs, '-=', s);
      case '*=': return this.numOr(cur, '*=', s) * this.numOr(rhs, '*=', s);
      case '/=': {
        const d = this.numOr(rhs, '/=', s);
        if (d === 0) throw new FloatError('除数不能为 0', s);
        return this.numOr(cur, '/=', s) / d;
      }
      case '%=': {
        const d = this.numOr(rhs, '%=', s);
        if (d === 0) throw new FloatError('取模的除数不能为 0', s);
        return this.numOr(cur, '%=', s) % d;
      }
      case '**=': return Math.pow(this.numOr(cur, '**=', s), this.numOr(rhs, '**=', s));
      default: return rhs;
    }
  }

  /* ── 函数与类调用 ── */

  private async *callUser(fn: FuncDef, args: Val[], self: Instance | null, e: Span | Expr, owner: ClassDef | null = null): ValGen {
    if (args.length > fn.params.length) {
      throw new FloatError(`${fn.name} 只接受 ${fn.params.length} 个参数，实际给了 ${args.length} 个`, spanOf(e));
    }
    /* 递归层数上限：写错终止条件的递归不该甩给用户一句
     * 「Maximum call stack size exceeded」，那是引擎的锅不是他的锅。 */
    if (this.callDepth >= MAX_CALL_DEPTH) {
      throw new FloatError(
        `递归太深了（超过 ${MAX_CALL_DEPTH} 层）停了下来。多半是 ${fn.name} 的结束条件没写对，检查一下它是不是会不断调用自己。`,
        spanOf(e));
    }
    const env = new Env(this.globals, fn.name);
    env.selfObj = self;
    for (let k = 0; k < fn.params.length; k++) {
      const p = fn.params[k];
      if (k < args.length) {
        env.define(p.name, args[k]);
      } else if (p.def) {
        env.define(p.name, yield* this.evalExpr(p.def, env));
      } else {
        throw new FloatError(`调用 ${fn.name} 缺少参数「${p.name}」`, spanOf(e));
      }
    }

    const savedEnv = this.env;
    const savedRet = this.retValue;
    this.retValue = null;
    this.env = env;
    this.ownerStack.push(owner);
    this.callDepth += 1;
    const caller = this.frames.length ? this.frames[this.frames.length - 1] : '';
    this.frames.push(`${self ? `${self.cls.name}.` : ''}${fn.name}() 在 ${this.shortFile(fn.file)} 第 ${fn.line} 行${caller ? `（由 ${caller} 调用）` : ''}`);
    try {
      const sig = yield* this.execBlock(fn.body, env, this.depth + 1);
      const ret = this.retValue;
      this.retValue = savedRet;
      this.env = savedEnv;
      if (sig === 'stop') throw new StopSignal();
      if (sig === 'break' || sig === 'continue') {
        throw new FloatError(`${fn.name} 里出现了 break / balo，但函数体内没有对应循环`, fn);
      }
      return ret;
    } catch (err) {
      this.env = savedEnv;
      this.retValue = savedRet;
      if (err instanceof FloatError) {
        if (!err.span) err.span = fn;
        if (!err.frames.length) err.frames = [...this.frames].reverse();
        throw err;
      }
      throw err;
    } finally {
      this.frames.pop();
      this.ownerStack.pop();
      this.callDepth -= 1;
    }
  }

  /** 实例化：先铺字段默认值，再调用 init（init 里可以有 ask / wait，所以是生成器） */
  private async *instantiate(cls: ClassDef, args: Val[], e: Span | Expr): ValGen {
    const inst = instance(cls);
    const fieldEnv = new Env(this.globals, cls.name);
    fieldEnv.selfObj = inst;
    for (const f of collectFields(cls)) {
      inst.fields.set(f.name, f.init ? yield* this.evalExpr(f.init, fieldEnv) : defaultValue(f.type));
    }
    const init = findMethod(cls, 'init') ?? findMethod(cls, '__init__') ?? findMethod(cls, 'constructor');
    if (init) {
      yield* this.callUser(init.fn, args, inst, e, init.owner);
    } else if (args.length) {
      throw new FloatError(`类 ${cls.name} 没有 init 方法，不能接收参数`, spanOf(e));
    }
    return inst;
  }

  /* ── import ── */

  private async loadImport(target: string, fromFile: string, s: Stmt): Promise<Program | null> {
    const key = `${fromFile}::${target}`;
    if (this.imported.has(key)) return null;
    this.imported.add(key);

    const resolved = await this.resolveImport(target, fromFile);
    if (!resolved) throw new FloatError(`import 找不到「${target}」（相对于 ${fromFile} 与工程根目录）`, s);
    const cached = this.programs.get(resolved);
    if (cached) return cached;

    const src = this.sources().get(resolved);
    if (src === undefined) throw new FloatError(`import 读不到「${target}」的内容`, s);
    const program = parseProgram(resolved, src);
    const errors = program.diagnostics.filter((d) => d.severity === 'error');
    if (errors.length) {
      const first = errors[0];
      throw new FloatError(`${this.shortFile(resolved)} 第 ${first.span.line} 行有语法错误：${first.message}`, s);
    }
    this.programs.set(resolved, program);
    return program;
  }

  private async resolveImport(target: string, fromFile: string): Promise<string | null> {
    const sources = this.sources();
    const root = this.importRoot.replace(/\\/g, '/').replace(/\/+$/, '');
    const baseDir = fromFile.includes('/') ? fromFile.replace(/\\/g, '/').replace(/\/[^/]*$/, '') : root;
    const join = (a: string, b: string): string => a.replace(/\/+$/, '') + '/' + b;
    const candidates = target.includes('.')
      ? [join(baseDir, target), join(root, target)]
      : [join(baseDir, target + '.cw'), join(root, target + '.cw'), join(join(baseDir, target), 'main.cw'), join(join(root, target), 'main.cw')];
    for (const c of candidates) {
      const norm = c.replace(/\\/g, '/');
      if (sources.has(norm)) return norm;
    }
    const { api } = await import('../core/tauri');
    for (const c of candidates) {
      try {
        const text = await api.fsRead(c.replace(/\//g, '\\'));
        const norm = c.replace(/\\/g, '/');
        sources.set(norm, text);
        return norm;
      } catch { /* 试下一个 */ }
    }
    return null;
  }
}

/* ───────────────────────────── 小工具 ───────────────────────────── */

function spanOf(e: Span | Expr): Span {
  if ('k' in e) return { file: e.file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
  return e;
}

/** Target → 可求值表达式（读取容器本身） */
function targetExpr(t: Target): Expr {
  if (t.k === 'var') return { k: 'var', name: t.name, file: t.file, line: t.line, col: t.col, endLine: t.endLine, endCol: t.endCol };
  if (t.k === 'member') return { k: 'member', obj: t.obj, name: t.name, file: t.file, line: t.line, col: t.col, endLine: t.endLine, endCol: t.endCol };
  return { k: 'idx', obj: t.obj, index: t.index, file: t.file, line: t.line, col: t.col, endLine: t.endLine, endCol: t.endCol };
}
