/* CW Script 编译执行通道：AST → JavaScript 源码 → new Function。
 *
 * 为什么要有这一层：
 * - 树遍历解释器每执行一条语句都要走一次函数调用与状态机，比 CPython 慢；
 * - 把 AST 编译成 JS 后交给 V8，循环与算术接近原生速度，稳定快过 Python；
 * - 编译发生在按 F5 的一瞬间（毫秒级），不存在"编译要等很久"的问题。
 *
 * 与调试器的分工：
 * - 运行（F5）走编译通道，最快；
 * - 调试（F6，需要断点/单步）走解释器，保证每行都能停。
 * 两条通道共用同一套值模型（values.ts），语义一致。
 */

import type { Expr, Param, Program, Stmt, Target, TypeName } from './ast';
import { builtinList, getBuiltin } from './builtins';
import {
  cloneVal, deepEq, dict, fmtVal, isDict, isInstance, isList, toNum, toStr, truthy, typeName,
  type Val
} from './values';
import { callBuiltinMethod, MethodError } from './methods';

/* ───────────────────────────── 类型 ───────────────────────────── */

export type CClass = {
  name: string;
  base: CClass | null;
  fields: { name: string; type: TypeName | null; init: ((rt: CompiledRuntime) => Val) | null }[];
  methods: Map<string, CompiledFn>;
};

export type CInst = {
  __cinst: 1;
  cls: CClass;
  f: Record<string, Val>;
};

export type CompiledFn = (rt: CompiledRuntime, self: CInst | null, ...args: Val[]) => Val;

/** 与解释通道一致的递归上限（见 interp.ts MAX_CALL_DEPTH） */
export const MAX_CALL_DEPTH = 400;

export const isCInst = (v: unknown): v is CInst =>
  typeof v === 'object' && v !== null && (v as CInst).__cinst === 1;

export type CompiledYield =
  | { kind: 'ask'; q: string }
  | { kind: 'wait'; seconds: number };

export class RuntimeError extends Error {
  constructor(message: string, public line: number, public frames: string[] = []) {
    super(message);
    this.name = 'RuntimeError';
  }
}

export class StopSignal extends Error {
  constructor() { super('stop'); this.name = 'StopSignal'; }
}

/* ───────────────────────────── 运行时 ───────────────────────────── */

export class CompiledRuntime {
  v: Record<string, Val> = {};
  /** 顶层函数 */
  f: Record<string, CompiledFn> = {};
  /** 方法（按 类_方法 命名，避免和函数重名） */
  mf: Record<string, CompiledFn> = {};
  classes: Record<string, CClass> = {};
  self: CInst | null = null;
  ln = 0;
  file = '';
  steps = 0;
  t0 = typeof performance !== 'undefined' ? performance.now() : 0;
  out: (line: string) => void = () => {};
  /** 报告通道：进右侧「报告」面板，不进控制台 */
  rep: (line: string, sourceLine: number) => void = () => {};
  private stack: string[] = [];

  /** 编译通道也要拦无限递归：否则用户只会看到一句「Maximum call stack size exceeded」，
   *  既没有行列号也不知道是哪一层出问题。和解释通道用同一个上限与同一套措辞。 */
  push(name: string): void {
    if (this.stack.length >= MAX_CALL_DEPTH) {
      const hint = `递归太深了（超过 ${MAX_CALL_DEPTH} 层）停了下来。多半是 ${name}() 的结束条件没写对，检查一下它是不是会不断调用自己。`;
      throw new RuntimeError(hint, this.ln);
    }
    this.stack.push(name);
  }
  pop(): void { this.stack.pop(); }
  frames(): string[] { return [...this.stack].reverse(); }

  /** 变量读取：未定义就报错，而不是静默 undefined */
  g(name: string): Val {
    if (!(name in this.v)) throw new RuntimeError(`变量「${name}」没有定义（先声明或赋值）`, this.ln);
    return this.v[name];
  }

  tick(): void {
    if (++this.steps > 20_000_000) {
      throw new RuntimeError('执行步数超过 2000 万，脚本可能陷入死循环（用 out / break / stop 结束）', this.ln);
    }
  }

  answerOf(r: unknown): string {
    if (r && typeof r === 'object' && 'answer' in r) {
      const a = (r as { answer?: unknown }).answer;
      return a == null ? '' : String(a);
    }
    return '';
  }

  print(parts: Val[]): void {
    this.out(parts.map((p) => toStr(p)).join(' '));
  }

  /** 报告：不带参数时报告当前所有变量（"现在都有什么"） */
  report(parts: Val[]): void {
    if (parts.length === 0) {
      const rows = Object.entries(this.v).map(([k, v]) => `  ${k} = ${toStr(v)}（${typeName(v)}）`);
      this.rep(rows.length ? `当前变量：\n${rows.join('\n')}` : '当前变量：（还没有）', this.ln);
      return;
    }
    this.rep(parts.map((p) => toStr(p)).join(' '), this.ln);
  }

  timeRead(): number { return (performance.now() - this.t0) / 1000; }
  timeReset(): void { this.t0 = performance.now(); }

  stop(): never { throw new StopSignal(); }

  callFn(name: string, args: Val[]): Val {
    const fn = this.f[name];
    if (!fn) throw new RuntimeError(`函数「${name}」没有定义`, this.ln);
    return fn(this, null, ...args);
  }

  /** catch 里拿到的错误对象：e.message / e.line / e.file */
  errObj(e: unknown): Val {
    const d = dict();
    if (e instanceof RuntimeError) {
      d.map.set('message', e.message);
      d.map.set('line', e.line || this.ln);
      d.map.set('file', this.file);
    } else {
      d.map.set('message', e instanceof Error ? e.message : String(e));
      d.map.set('line', this.ln);
      d.map.set('file', this.file);
    }
    return d;
  }

  throwVal(v: Val): never {
    const msg = isDict(v) && v.map.has('message') ? toStr(v.map.get('message') as Val) : toStr(v);
    throw new RuntimeError(msg, this.ln);
  }

  badBreak(what: string): never {
    throw new RuntimeError(`${what} 只能用在循环里`, this.ln);
  }

  /* ── 类 ── */

  makClass(
    name: string,
    baseName: string | null,
    fields: { name: string; type: TypeName | null; init: ((rt: CompiledRuntime) => Val) | null }[],
    methods: Record<string, CompiledFn>
  ): CClass {
    const base = baseName ? this.classes[baseName] ?? null : null;
    if (baseName && !base) throw new RuntimeError(`父类「${baseName}」还没有定义（先写父类，再写子类）`, this.ln);
    const m = new Map<string, CompiledFn>();
    for (const [k, v] of Object.entries(methods)) m.set(k, v);
    return { name, base, fields, methods: m };
  }

  newInst(name: string, args: Val[]): CInst {
    const cls = this.classes[name];
    if (!cls) throw new RuntimeError(`类「${name}」没有定义`, this.ln);
    const inst: CInst = { __cinst: 1, cls, f: {} };
    const chain: CClass[] = [];
    let c: CClass | null = cls;
    while (c) { chain.unshift(c); c = c.base; }
    for (const cd of chain) {
      for (const fd of cd.fields) {
        inst.f[fd.name] = fd.init ? fd.init(this) : defaultFor(fd.type);
      }
    }
    const init = this.findMethod(cls, 'init') ?? this.findMethod(cls, '__init__');
    if (init) init(this, inst, ...args);
    else if (args.length) throw new RuntimeError(`类 ${name} 没有 init 方法，不能接收参数`, this.ln);
    return inst;
  }

  findMethod(cls: CClass | null, name: string): CompiledFn | null {
    let c = cls;
    while (c) {
      const fn = c.methods.get(name);
      if (fn) return fn;
      c = c.base;
    }
    return null;
  }

  /** super(args)：调用父类 init */
  callSuper(className: string, args: Val[]): Val {
    const cls = this.classes[className];
    if (!cls || !cls.base) throw new RuntimeError('super(…) 只能用在有父类的类里', this.ln);
    const init = this.findMethod(cls.base, 'init') ?? this.findMethod(cls.base, '__init__');
    if (!init) throw new RuntimeError(`父类 ${cls.base.name} 没有 init，无需 super(…)`, this.ln);
    return init(this, this.self, ...args);
  }
}

function defaultFor(type: TypeName | null): Val {
  switch (type) {
    case 'int':
    case 'float': return 0;
    case 'str': return '';
    case 'bool': return false;
    case 'list': return [];
    case 'dict': return dict();
    default: return null;
  }
}

/* ───────────────────────────── 运算辅助（语义与解释器一致） ───────────────────────────── */

export const O = {
  t: (v: Val): boolean => truthy(v),

  num(v: Val, who = '运算'): number {
    const n = toNum(v);
    if (n === null) throw new RuntimeError(`${who}需要数字，得到 ${fmtVal(v)}`, 0);
    return n;
  },

  add(a: Val, b: Val): Val {
    if (typeof a === 'number' && typeof b === 'number') return a + b;
    if (typeof a === 'string' && typeof b === 'string') return a + b;
    if (typeof a === 'string' || typeof b === 'string') return toStr(a) + toStr(b);
    if (isList(a) && isList(b)) return [...a, ...b];
    return O.num(a, '「+」') + O.num(b, '「+」');
  },
  sub(a: Val, b: Val): Val {
    if (typeof a === 'number' && typeof b === 'number') return a - b;
    return O.num(a, '「-」') - O.num(b, '「-」');
  },
  mul(a: Val, b: Val): Val {
    if (typeof a === 'number' && typeof b === 'number') return a * b;
    if (typeof a === 'string' && typeof b === 'number') return a.repeat(Math.max(0, Math.trunc(b)));
    return O.num(a, '「*」') * O.num(b, '「*」');
  },
  div(a: Val, b: Val): Val {
    if (typeof a === 'number' && typeof b === 'number') {
      if (b === 0) throw new RuntimeError('除数不能为 0', 0);
      return a / b;
    }
    const d = O.num(b, '「/」');
    if (d === 0) throw new RuntimeError('除数不能为 0', 0);
    return O.num(a, '「/」') / d;
  },
  mod(a: Val, b: Val): Val {
    if (typeof a === 'number' && typeof b === 'number') {
      if (b === 0) throw new RuntimeError('取模的除数不能为 0', 0);
      return a % b;
    }
    const d = O.num(b, '「%」');
    if (d === 0) throw new RuntimeError('取模的除数不能为 0', 0);
    return O.num(a, '「%」') % d;
  },
  pow(a: Val, b: Val): Val {
    if (typeof a === 'number' && typeof b === 'number') return Math.pow(a, b);
    return Math.pow(O.num(a, '「**」'), O.num(b, '「**」'));
  },
  neg(a: Val): Val { return typeof a === 'number' ? -a : -O.num(a, '一元 -'); },
  bitnot(a: Val): Val { return ~Math.trunc(O.num(a, '~')); },
  band(a: Val, b: Val): Val { return Math.trunc(O.num(a, '&')) & Math.trunc(O.num(b, '&')); },
  bor(a: Val, b: Val): Val { return Math.trunc(O.num(a, '|')) | Math.trunc(O.num(b, '|')); },
  bxor(a: Val, b: Val): Val { return Math.trunc(O.num(a, '^')) ^ Math.trunc(O.num(b, '^')); },
  shl(a: Val, b: Val): Val { return Math.trunc(O.num(a, '<<')) << Math.trunc(O.num(b, '<<')); },
  shr(a: Val, b: Val): Val { return Math.trunc(O.num(a, '>>')) >> Math.trunc(O.num(b, '>>')); },

  inc(v: Val, delta: number): Val {
    const n = toNum(v);
    if (n === null) throw new RuntimeError(`「${fmtVal(v)}」不是数字，不能 ${delta > 0 ? '++' : '--'}`, 0);
    return n + delta;
  },

  lt(a: Val, b: Val): boolean {
    if (typeof a === 'number' && typeof b === 'number') return a < b;
    if (typeof a === 'string' && typeof b === 'string') return a < b;
    const x = toNum(a);
    const y = toNum(b);
    if (x === null || y === null) throw new RuntimeError(`比较大小需要两个数字或两个字符串，得到 ${fmtVal(a)} 与 ${fmtVal(b)}`, 0);
    return x < y;
  },
  eq(a: Val, b: Val): boolean {
    if (typeof a === 'number' && typeof b === 'number') return a === b;
    if (typeof a === 'string' && typeof b === 'string') return a === b;
    if (typeof a === 'boolean' && typeof b === 'boolean') return a === b;
    return deepEq(a, b);
  },
  eq3(a: Val, b: Val): boolean { return typeof a === typeof b && deepEq(a, b); },

  comp(op: string, a: Val, b: Val): Val {
    switch (op) {
      case '+=': return O.add(a, b);
      case '-=': return O.sub(a, b);
      case '*=': return O.mul(a, b);
      case '/=': return O.div(a, b);
      case '%=': return O.mod(a, b);
      case '**=': return O.pow(a, b);
      default: return b;
    }
  },

  get(obj: Val, idx: Val): Val {
    if (isPtr(obj)) {
      if (obj.freed) throw new RuntimeError('这块内存已经 free 了，不能再读', 0);
      const n = toNum(idx);
      if (n === null) throw new RuntimeError(`指针下标必须是数字，得到 ${fmtVal(idx)}`, 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.buf.length + i : i;
      if (real < 0 || real >= obj.buf.length) throw new RuntimeError(`下标 ${i} 超出范围（长度 ${obj.buf.length}）`, 0);
      return obj.buf[real];
    }
    if (Array.isArray(obj)) {
      const n = toNum(idx);
      if (n === null) throw new RuntimeError(`列表下标必须是数字，得到 ${fmtVal(idx)}`, 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new RuntimeError(`下标 ${i} 超出范围（长度 ${obj.length}）`, 0);
      return obj[real];
    }
    if (typeof obj === 'string') {
      const n = toNum(idx);
      if (n === null) throw new RuntimeError(`字符下标必须是数字，得到 ${fmtVal(idx)}`, 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new RuntimeError(`字符下标 ${i} 超出范围（长度 ${obj.length}）`, 0);
      return obj[real];
    }
    if (isDict(obj)) {
      const k = toStr(idx);
      if (!obj.map.has(k)) throw new RuntimeError(`字典里没有键「${k}」`, 0);
      return obj.map.get(k) as Val;
    }
    if (isCInst(obj)) {
      const k = toStr(idx);
      if (k in obj.f) return obj.f[k];
      throw new RuntimeError(`${obj.cls.name} 没有字段「${k}」`, 0);
    }
    throw new RuntimeError(`这个值不能用下标访问：${fmtVal(obj)}`, 0);
  },

  mem(obj: Val, name: string): Val {
    if (isCInst(obj)) {
      if (name in obj.f) return obj.f[name];
      throw new RuntimeError(`${obj.cls.name} 没有字段「${name}」`, 0);
    }
    if (isDict(obj)) {
      if (!obj.map.has(name)) throw new RuntimeError(`字典里没有键「${name}」`, 0);
      return obj.map.get(name) as Val;
    }
    if (name === 'length' && (isList(obj) || typeof obj === 'string')) return obj.length;
    if (isInstance(obj)) {
      if (obj.fields.has(name)) return obj.fields.get(name) as Val;
      throw new RuntimeError(`${obj.cls.name} 没有字段「${name}」`, 0);
    }
    throw new RuntimeError(`这个值没有成员「${name}」：${fmtVal(obj)}`, 0);
  },

  smem(obj: Val, name: string, val: Val): void {
    if (isCInst(obj)) { obj.f[name] = val; return; }
    if (isDict(obj)) { obj.map.set(name, val); return; }
    if (isInstance(obj)) { obj.fields.set(name, val); return; }
    throw new RuntimeError(`不能给 ${fmtVal(obj)} 的成员赋值`, 0);
  },

  sidx(obj: Val, idx: Val, val: Val): void {
    if (Array.isArray(obj)) {
      const n = toNum(idx);
      if (n === null) throw new RuntimeError('列表下标必须是数字', 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new RuntimeError(`下标 ${i} 超出范围（长度 ${obj.length}）`, 0);
      obj[real] = val;
      return;
    }
    if (isDict(obj)) { obj.map.set(toStr(idx), val); return; }
    if (isCInst(obj)) { obj.f[toStr(idx)] = val; return; }
    throw new RuntimeError(`这个值不能按下标赋值：${fmtVal(obj)}`, 0);
  },

  listAdd(target: Val, val: Val): void {
    if (!isList(target)) throw new RuntimeError(`「${fmtVal(target)}」不是列表`, 0);
    target.push(cloneVal(val));
  },

  listInsert(target: Val, at: Val, val: Val): void {
    if (!isList(target)) throw new RuntimeError(`「${fmtVal(target)}」不是列表`, 0);
    const n = toNum(at);
    if (n === null) throw new RuntimeError('插入位置必须是数字', 0);
    const i = Math.trunc(n);
    if (i < 0 || i > target.length) throw new RuntimeError(`插入位置 ${i} 超出范围（长度 ${target.length}）`, 0);
    target.splice(i, 0, cloneVal(val));
  },

  listReplace(target: Val, at: Val, val: Val): void {
    if (!isList(target)) throw new RuntimeError(`「${fmtVal(target)}」不是列表`, 0);
    const n = toNum(at);
    if (n === null) throw new RuntimeError('下标必须是数字', 0);
    const i = Math.trunc(n);
    if (i < 0 || i >= target.length) throw new RuntimeError(`下标 ${i} 超出范围（长度 ${target.length}）`, 0);
    target[i] = cloneVal(val);
  },

  listDelete(target: Val, at: Val | null): void {
    if (!isList(target)) throw new RuntimeError(`「${fmtVal(target)}」不是列表`, 0);
    if (at === null) { target.length = 0; return; }
    const n = toNum(at);
    if (n === null) throw new RuntimeError('下标必须是数字', 0);
    const i = Math.trunc(n);
    if (i < 0 || i >= target.length) throw new RuntimeError(`下标 ${i} 超出范围（长度 ${target.length}）`, 0);
    target.splice(i, 1);
  },

  listRead(target: Val, at: Val): Val {
    const n = toNum(at);
    if (n === null) throw new RuntimeError('下标必须是数字', 0);
    const i = Math.trunc(n);
    if (Array.isArray(target)) {
      const real = i < 0 ? target.length + i : i;
      if (real < 0 || real >= target.length) throw new RuntimeError(`下标 ${i} 超出范围（长度 ${target.length}）`, 0);
      return target[real];
    }
    if (typeof target === 'string') {
      const real = i < 0 ? target.length + i : i;
      if (real < 0 || real >= target.length) throw new RuntimeError(`字符下标 ${i} 超出范围（长度 ${target.length}）`, 0);
      return target[real];
    }
    throw new RuntimeError(`「${fmtVal(target)}」不是列表`, 0);
  },

  lenOf(v: Val): number {
    if (isList(v)) return v.length;
    if (typeof v === 'string') return v.length;
    if (isDict(v)) return v.map.size;
    if (isCInst(v)) return Object.keys(v.f).length;
    throw new RuntimeError(`len 只接受列表 / 字符串 / 字典，得到 ${fmtVal(v)}`, 0);
  },

  dictSet(target: Val, key: Val, val: Val): void {
    if (!isDict(target)) throw new RuntimeError(`「${fmtVal(target)}」不是字典`, 0);
    target.map.set(toStr(key), cloneVal(val));
  },

  dictRead(target: Val, key: Val): Val {
    if (!isDict(target)) throw new RuntimeError(`「${fmtVal(target)}」不是字典`, 0);
    const k = toStr(key);
    if (!target.map.has(k)) throw new RuntimeError(`字典里没有键「${k}」`, 0);
    return target.map.get(k) as Val;
  },

  dictDelete(target: Val, key: Val): void {
    if (!isDict(target)) throw new RuntimeError(`「${fmtVal(target)}」不是字典`, 0);
    target.map.delete(toStr(key));
  },

  /** 单变量 for：列表/字符串给元素，字典给键 */
  each(v: Val): Val[] {
    if (isList(v)) return v;
    if (typeof v === 'string') return v.split('');
    if (isDict(v)) return [...v.map.keys()];
    throw new RuntimeError(`for … in 只能遍历列表、字符串或字典，得到 ${fmtVal(v)}`, 0);
  },

  /** 双变量 for：字典给 [键, 值] */
  pairs(v: Val): [Val, Val][] {
    if (isDict(v)) return [...v.map];
    throw new RuntimeError('for a, b in … 只能用在字典上', 0);
  },

  mcall(obj: Val, name: string, args: Val[], rt: CompiledRuntime): Val {
    try {
      const m = callBuiltinMethod(obj, name, args);
      if (m.handled) return m.value;
    } catch (e) {
      if (e instanceof MethodError) throw new RuntimeError(e.message, rt.ln);
      throw e;
    }
    if (isCInst(obj)) {
      const fn = rt.findMethod(obj.cls, name);
      if (!fn) throw new RuntimeError(`类 ${obj.cls.name} 没有方法「${name}」`, rt.ln);
      return fn(rt, obj, ...args);
    }
    if (isInstance(obj)) {
      throw new RuntimeError(`类 ${obj.cls.name} 的方法「${name}」需要在解释模式下运行`, rt.ln);
    }
    throw new RuntimeError(`这个值没有方法「${name}」：${fmtVal(obj)}`, rt.ln);
  }
};

/* ───────────────────────────── 编译器 ───────────────────────────── */

type Ctx = {
  locals: Set<string>;
  top: boolean;
  fields: Set<string>;
  className: string | null;
  inLoop: boolean;
};

class CompileError extends Error {}

export type CompiledUnit = {
  main: (rt: CompiledRuntime) => Generator<CompiledYield, void, unknown>;
  functions: number;
  src?: string;
};

/** 退回解释模式的提示只打一次，别每次运行都刷 */
let warnedFallback = false;

const BUILTINS_REF: Record<string, (...args: Val[]) => Val> = {};
for (const b of builtinList()) BUILTINS_REF[b.name] = (...args: Val[]) => b.fn(args);

const VALUES_REF = { toStr, dict };

export function compileProgram(program: Program): CompiledUnit | null {
  const fnNames = new Set<string>();
  const classInfo = new Map<string, { fields: Set<string>; methods: Map<string, { params: Param[]; body: Stmt[] }>; base: string | null }>();

  /* 预扫描：函数名、类结构（供先用后定义与裸字段访问） */
  const scan = (body: Stmt[], inClass = false): void => {
    for (const s of body) {
      if (s.k === 'funcDef') {
        if (!inClass) fnNames.add(s.name);
        scan(s.body, inClass);
      } else if (s.k === 'classDef') {
        const fields = new Set<string>();
        for (const f of s.fields) fields.add(f.name);
        const methods = new Map<string, { params: Param[]; body: Stmt[] }>();
        for (const m of s.methods) {
          if (m.k === 'funcDef') methods.set(m.name, { params: m.params, body: m.body });
        }
        classInfo.set(s.name, { fields, methods, base: s.base });
        scan(s.methods, true);
      } else if (s.k === 'import') {
        throw new CompileError('有 import，交回解释模式');
      } else if (s.k === 'unknown') {
        throw new CompileError('有无法识别的语句，交回解释模式');
      } else {
        scan(nestedBlocks(s));
      }
    }
  };

  const fieldsOf = (name: string | null): Set<string> => {
    const out = new Set<string>();
    let cur = name;
    const guard = new Set<string>();
    while (cur && !guard.has(cur)) {
      guard.add(cur);
      const info = classInfo.get(cur);
      if (!info) break;
      for (const f of info.fields) out.add(f);
      cur = info.base;
    }
    return out;
  };

  const methodsOf = (name: string | null): Set<string> => {
    const out = new Set<string>();
    let cur = name;
    const guard = new Set<string>();
    while (cur && !guard.has(cur)) {
      guard.add(cur);
      const info = classInfo.get(cur);
      if (!info) break;
      for (const m of info.methods.keys()) out.add(m);
      cur = info.base;
    }
    return out;
  };

  try {
    scan(program.body);
  } catch {
    return null;
  }

  try {
    const body = genProgramBody(program.body, {
      locals: new Set(), top: true, fields: new Set(), className: null, inLoop: false
    });

    const src = `"use strict";
return function* __floatMain($) {
${body}
};`;
    const factory = new Function('B', 'O', 'V', src) as
      (b: typeof BUILTINS_REF, o: typeof O, v: typeof VALUES_REF) => CompiledUnit['main'];
    const main = factory(BUILTINS_REF, O, VALUES_REF);
    return { main, functions: fnNames.size, src };
  } catch (e) {
    // 同一个问题只提示一次：否则每次运行都会往浏览器控制台刷一条，看着很吵
    if (!(e instanceof CompileError) && !warnedFallback) {
      warnedFallback = true;
      console.warn('[Float 编译器] 退回解释模式（只提示一次）：' + (e instanceof Error ? e.message : String(e)));
    }
    return null;
  }

  /* ── 语句 ── */

  function genProgramBody(stmts: Stmt[], ctx: Ctx): string {
    let out = '';
    for (const s of stmts) {
      if (s.k === 'funcDef') out += genFuncDef(s, ctx);
      else if (s.k === 'classDef') out += genClassDef(s);
    }
    for (const s of stmts) {
      if (s.k === 'funcDef' || s.k === 'classDef') continue;
      out += genStmt(s, ctx);
    }
    return out;
  }

  function genBlock(stmts: Stmt[], ctx: Ctx): string {
    let out = '';
    for (const s of stmts) out += genStmt(s, ctx);
    return out;
  }

  function localDecls(stmts: Stmt[], seed: Set<string>): Set<string> {
    const out = new Set(seed);
    const walk = (list: Stmt[]): void => {
      for (const s of list) {
        switch (s.k) {
          case 'decl': out.add(s.name); break;
          case 'funcDef': out.add(s.name); break;
          case 'destructure': for (const n of s.names) out.add(n.name); break;
          case 'if':
            for (const b of s.branches) walk(b.body);
            if (s.elseBody) walk(s.elseBody);
            break;
          case 'while':
          case 'repeat':
          case 'forever':
          case 'waitUntil':
            walk(s.body);
            break;
          case 'forRange':
            out.add(s.varName);
            walk(s.body);
            break;
          case 'forIn':
            out.add(s.varName);
            if (s.valueName) out.add(s.valueName);
            walk(s.body);
            break;
          default: break;
        }
      }
    };
    walk(stmts);
    return out;
  }

  function genFuncDef(s: Extract<Stmt, { k: 'funcDef' }>, outer: Ctx): string {
    const locals = localDecls(s.body, new Set(s.params.map((p) => p.name)));
    const ctx: Ctx = { locals, top: false, fields: new Set(), className: outer.className, inLoop: false };
    const extra = [...locals].filter((n) => !s.params.some((p) => p.name === n));
    let out = `$.f[${JSON.stringify(s.name)}] = function($, ${s.params.map((p) => p.name).join(', ')}) {\n`;
    out += `$.push(${JSON.stringify(s.name)});\n`;
    out += `try {\n`;
    if (extra.length) out += `let ${extra.join(', ')};\n`;
    for (const p of s.params) {
      if (p.def) out += `if (${p.name} === undefined) ${p.name} = ${genExpr(p.def, ctx)};\n`;
    }
    out += genBlock(s.body, ctx);
    out += `\nreturn null;\n} finally { $.pop(); }\n};\n`;
    return out;
  }

  function genClassDef(s: Extract<Stmt, { k: 'classDef' }>): string {
    const fctx: Ctx = { locals: new Set(), top: false, fields: new Set(), className: s.name, inLoop: false };
    const fields = s.fields.map((f) => {
      const init = f.init ? `function($) { return ${genExpr(f.init, fctx)}; }` : 'null';
      return `{ name: ${JSON.stringify(f.name)}, type: ${JSON.stringify(f.type)}, init: ${init} }`;
    });

    let out = '';
    const methodEntries: string[] = [];
    for (const m of s.methods) {
      if (m.k !== 'funcDef') continue;
      const locals = localDecls(m.body, new Set(m.params.map((p) => p.name)));
      const ctx: Ctx = { locals, top: false, fields: fieldsOf(s.name), className: s.name, inLoop: false };
      const extra = [...locals].filter((n) => !m.params.some((p) => p.name === n));
      const key = `__m_${s.name}_${m.name}`;
      out += `$.mf[${JSON.stringify(key)}] = function($, self, ${m.params.map((p) => p.name).join(', ')}) {\n`;
      out += `const __keepSelf = $.self, __keepLn = $.ln; $.self = self; $.push(${JSON.stringify(m.name)});\n`;
      out += `try {\n`;
      if (extra.length) out += `let ${extra.join(', ')};\n`;
      for (const p of m.params) {
        if (p.def) out += `if (${p.name} === undefined) ${p.name} = ${genExpr(p.def, ctx)};\n`;
      }
      out += genBlock(m.body, ctx);
      out += `\nreturn null;\n} finally { $.self = __keepSelf; $.ln = __keepLn; $.pop(); }\n};\n`;
      methodEntries.push(`${JSON.stringify(m.name)}: $.mf[${JSON.stringify(key)}]`);
    }

    out += `$.classes[${JSON.stringify(s.name)}] = $.makClass(${JSON.stringify(s.name)}, ${JSON.stringify(s.base)}, [${fields.join(', ')}], {${methodEntries.join(', ')}});\n`;
    return out;
  }

  function genStmt(s: Stmt, ctx: Ctx): string {
    const ln = `$.ln = ${s.line};\n`;
    switch (s.k) {
      case 'comment':
        return '';
      case 'unknown':
        throw new CompileError('无法识别的语句');

      case 'decl': {
        const val = s.init ? genExpr(s.init, ctx) : defaultLiteral(s.type);
        if (ctx.top || !ctx.locals.has(s.name)) return `${ln}$.v[${JSON.stringify(s.name)}] = ${val};\n`;
        return `${ln}${s.name} = ${val};\n`;
      }

      case 'assign':
        return `${ln}${genAssign(s.target, s.op, genExpr(s.value, ctx), ctx)};\n`;

      case 'incdec': {
        const cur = genRead(s.target, ctx);
        const val = `O.inc(${cur}, ${s.delta})`;
        return `${ln}${genAssign(s.target, '=', val, ctx)};\n`;
      }

      case 'expr': {
        const e = s.expr;
        if (e.k === 'call' && e.callee.k === 'var') {
          const b = getBuiltin(e.callee.name);
          if (b) {
            const call = `B.${e.callee.name}(${e.args.map((a) => genExpr(a, ctx)).join(', ')})`;
            return b.writes ? `${ln}$.v[${JSON.stringify(b.writes)}] = ${call};\n` : `${ln}${call};\n`;
          }
        }
        if (e.k === 'var' && e.name === 'ans') return `${ln}$.v.__last_ans = $.v.ans;\n`;
        return `${ln}${genExpr(e, ctx)};\n`;
      }

      case 'print':
        return `${ln}$.print([${s.args.map((a) => genExpr(a, ctx)).join(', ')}]);\n`;

      case 'report':
        // 报告走同一个运行时，只是换个回调：进右侧「报告」面板
        return `${ln}$.report([${s.args.map((a) => genExpr(a, ctx)).join(', ')}]);\n`;

      case 'ask': {
        const q = genExpr(s.question, ctx);
        return `${ln}$.v.ans = $.answerOf(yield { kind: 'ask', q: V.toStr(${q}) }); $.v.__last_ans = $.v.ans;\n`;
      }

      case 'wait':
        return `${ln}yield { kind: 'wait', seconds: O.num(${genExpr(s.seconds, ctx)}, 'wait') };\n`;

      case 'if': {
        let out = `${ln}`;
        let first = true;
        for (const br of s.branches) {
          const test = br.cond ? `O.t(${genExpr(br.cond, ctx)})` : 'true';
          out += `${first ? 'if' : 'else if'} (${test}) {\n${genBlock(br.body, ctx)}} `;
          first = false;
        }
        if (s.elseBody) out += `else {\n${genBlock(s.elseBody, ctx)}}`;
        return out + '\n';
      }

      case 'while': {
        const c2: Ctx = { ...ctx, inLoop: true };
        return `${ln}while (O.t(${genExpr(s.cond, ctx)})) {\n$.tick();\n${genBlock(s.body, c2)}}\n`;
      }

      case 'forever': {
        const c2: Ctx = { ...ctx, inLoop: true };
        return `${ln}for (;;) {\n$.tick();\n${genBlock(s.body, c2)}}\n`;
      }

      case 'repeat': {
        const c2: Ctx = { ...ctx, inLoop: true };
        return `${ln}for (let __r = 0, __rn = Math.trunc(O.num(${genExpr(s.count, ctx)}, 'repeat')); __r < __rn; __r++) {\n$.tick();\n${genBlock(s.body, c2)}}\n`;
      }

      case 'forRange': {
        const c2: Ctx = { ...ctx, inLoop: true, locals: new Set([...ctx.locals, s.varName]) };
        const from = s.from ? genExpr(s.from, ctx) : '0';
        const to = genExpr(s.to, ctx);
        const step = s.step ? genExpr(s.step, ctx) : '1';
        const cmp = s.inclusive
          ? '(__st > 0 ? __i < __end + 1e-9 : __i > __end - 1e-9)'
          : '(__st > 0 ? __i < __end : __i > __end)';
        return `${ln}{
  const __st = O.num(${step}, 'for 步长') || 1;
  const __end = O.num(${to}, 'for 终点');
  for (let __i = O.num(${from}, 'for 起点'); ${cmp}; __i += __st) {
    $.tick();
    let ${s.varName} = __i;
${genBlock(s.body, c2)}  }
}\n`;
      }

      case 'forIn': {
        const names = s.valueName ? [s.varName, s.valueName] : [s.varName];
        const c2: Ctx = { ...ctx, inLoop: true, locals: new Set([...ctx.locals, ...names]) };
        const iter = genExpr(s.iter, ctx);
        if (s.valueName) {
          return `${ln}for (const __p of O.pairs(${iter})) {\n$.tick();\nlet ${s.varName} = __p[0], ${s.valueName} = __p[1];\n${genBlock(s.body, c2)}}\n`;
        }
        return `${ln}for (const __x of O.each(${iter})) {\n$.tick();\nlet ${s.varName} = __x;\n${genBlock(s.body, c2)}}\n`;
      }

      case 'waitUntil': {
        const c2: Ctx = { ...ctx, inLoop: true };
        return `${ln}while (!O.t(${genExpr(s.cond, ctx)})) {\n$.tick();\n${genBlock(s.body, c2)}}\n`;
      }

      case 'break':
        return ctx.inLoop ? `${ln}break;\n` : `${ln}$.badBreak('break / out');\n`;
      case 'continue':
        return ctx.inLoop ? `${ln}continue;\n` : `${ln}$.badBreak('balo');\n`;
      case 'stop':
        return `${ln}$.stop();\n`;
      case 'ifOut':
        return ctx.inLoop
          ? `${ln}if (O.t(${genExpr(s.cond, ctx)})) break;\n`
          : `${ln}$.badBreak('if out');\n`;
      case 'return':
        return `${ln}return ${s.value ? genExpr(s.value, ctx) : 'null'};\n`;

      case 'throw':
        return `${ln}$.throwVal(${genExpr(s.value, ctx)});\n`;

      case 'try': {
        const c2: Ctx = { ...ctx };
        const body = genBlock(s.body, c2);
        const fin = s.finallyBody ? ` finally {\n${genBlock(s.finallyBody, ctx)}}` : '';
        if (!s.catchBody && !s.finallyBody) return `${ln}${body}`;
        if (!s.catchBody) return `${ln}try {\n${body}}${fin}\n`;
        const bind = s.catchName ? `let ${s.catchName} = $.errObj(__e);\n` : '';
        const cctx: Ctx = s.catchName
          ? { ...ctx, locals: new Set([...ctx.locals, s.catchName]) }
          : c2;
        return `${ln}try {\n${body}} catch (__e) {\n${bind}${genBlock(s.catchBody, cctx)}}${fin}\n`;
      }

      case 'destructure': {
        const tmp = `__d${s.line}_${s.col}`;
        let out = `${ln}{ const ${tmp} = ${genExpr(s.value, ctx)};\n`;
        s.names.forEach((n, idx) => {
          out += `${genAssign({ k: 'var', name: n.name, file: s.file, line: n.line, col: n.col, endLine: n.line, endCol: n.col }, '=', `${tmp}[${idx}]`, ctx)};\n`;
        });
        return out + '}\n';
      }

      case 'funcDef':
        return genFuncDef(s, ctx);
      case 'classDef':
        return genClassDef(s);

      case 'import':
        throw new CompileError('import 交回解释模式');

      case 'listAdd':
        return `${ln}O.listAdd(${genRead(s.target, ctx)}, ${genExpr(s.value, ctx)});\n`;
      case 'listInsert':
        return `${ln}O.listInsert(${genRead(s.target, ctx)}, ${genExpr(s.index, ctx)}, ${genExpr(s.value, ctx)});\n`;
      case 'listReplace':
        return `${ln}O.listReplace(${genRead(s.target, ctx)}, ${genExpr(s.index, ctx)}, ${genExpr(s.value, ctx)});\n`;
      case 'listDelete':
        return `${ln}O.listDelete(${genRead(s.target, ctx)}, ${s.index ? genExpr(s.index, ctx) : 'null'});\n`;
      case 'listRead':
        return `${ln}$.v.__last_read = O.listRead(${genRead(s.target, ctx)}, ${genExpr(s.index, ctx)});\n`;
      case 'lenOf':
        return `${ln}$.v.__last_len = O.lenOf(${genExpr(s.value, ctx)});\n`;
      case 'dictSet':
        return `${ln}O.dictSet(${genRead(s.target, ctx)}, ${genExpr(s.key, ctx)}, ${genExpr(s.value, ctx)});\n`;
      case 'dictRead':
        return `${ln}$.v.__last_read = O.dictRead(${genRead(s.target, ctx)}, ${genExpr(s.key, ctx)});\n`;
      case 'dictDelete':
        return `${ln}O.dictDelete(${genRead(s.target, ctx)}, ${genExpr(s.key, ctx)});\n`;
      case 'timeRead':
        return `${ln}$.v.__last_time = $.timeRead();\n`;
      case 'timeReset':
        return `${ln}$.timeReset();\n`;
    }
    throw new CompileError('未知语句');
  }

  function genRead(t: Target, ctx: Ctx): string {
    if (t.k === 'var') return readVar(t.name, ctx);
    if (t.k === 'member') return `O.mem(${genExpr(t.obj, ctx)}, ${JSON.stringify(t.name)})`;
    return `O.get(${genExpr(t.obj, ctx)}, ${genExpr(t.index, ctx)})`;
  }

  function readVar(name: string, ctx: Ctx): string {
    if (ctx.locals.has(name)) return name;
    if (ctx.fields.has(name)) return `O.mem($.self, ${JSON.stringify(name)})`;
    return `$.g(${JSON.stringify(name)})`;
  }

  function genAssign(t: Target, op: string, valCode: string, ctx: Ctx): string {
    if (t.k === 'var') {
      const cur = op === '=' ? null : readVar(t.name, ctx);
      const value = cur ? `O.comp(${JSON.stringify(op)}, ${cur}, ${valCode})` : valCode;
      if (ctx.locals.has(t.name)) return `${t.name} = ${value}`;
      if (ctx.fields.has(t.name)) return `O.smem($.self, ${JSON.stringify(t.name)}, ${value})`;
      return `($.v[${JSON.stringify(t.name)}] = ${value})`;
    }
    if (t.k === 'member') {
      const obj = genExpr(t.obj, ctx);
      if (op === '=') return `O.smem(${obj}, ${JSON.stringify(t.name)}, ${valCode})`;
      return `(() => { const __o = ${obj}; O.smem(__o, ${JSON.stringify(t.name)}, O.comp(${JSON.stringify(op)}, O.mem(__o, ${JSON.stringify(t.name)}), ${valCode})); return null; })()`;
    }
    const obj = genExpr(t.obj, ctx);
    const idx = genExpr(t.index, ctx);
    if (op === '=') return `O.sidx(${obj}, ${idx}, ${valCode})`;
    return `(() => { const __o = ${obj}, __i = ${idx}; O.sidx(__o, __i, O.comp(${JSON.stringify(op)}, O.get(__o, __i), ${valCode})); return null; })()`;
  }

  function genExpr(e: Expr, ctx: Ctx): string {
    switch (e.k) {
      case 'num': return JSON.stringify(e.v);
      case 'str': return JSON.stringify(e.v);
      case 'bool': return e.v ? 'true' : 'false';
      case 'null': return 'null';
      case 'list': return `[${e.items.map((i) => genExpr(i, ctx)).join(', ')}]`;
      case 'dict':
        return `V.dict([${e.entries.map((en) => `[V.toStr(${genExpr(en.key, ctx)}), ${genExpr(en.val, ctx)}]`).join(', ')}])`;
      case 'var': return readVar(e.name, ctx);
      case 'self': return '$.self';
      case 'un':
        if (e.op === '!') return `(!O.t(${genExpr(e.a, ctx)}))`;
        if (e.op === '-') return `O.neg(${genExpr(e.a, ctx)})`;
        if (e.op === '+') return `O.num(${genExpr(e.a, ctx)}, '一元 +')`;
        return `O.bitnot(${genExpr(e.a, ctx)})`;
      case 'bin': {
        const a = genExpr(e.a, ctx);
        const b = genExpr(e.b, ctx);
        switch (e.op) {
          case '&&': return `(O.t(${a}) ? O.t(${b}) : false)`;
          case '||': return `(O.t(${a}) ? true : O.t(${b}))`;
          case '+': return `O.add(${a}, ${b})`;
          case '-': return `O.sub(${a}, ${b})`;
          case '*': return `O.mul(${a}, ${b})`;
          case '/': return `O.div(${a}, ${b})`;
          case '%': return `O.mod(${a}, ${b})`;
          case '**': return `O.pow(${a}, ${b})`;
          case '&': return `O.band(${a}, ${b})`;
          case '|': return `O.bor(${a}, ${b})`;
          case '^': return `O.bxor(${a}, ${b})`;
          case '<<': return `O.shl(${a}, ${b})`;
          case '>>': return `O.shr(${a}, ${b})`;
          case '==': return `O.eq(${a}, ${b})`;
          case '!=': return `(!O.eq(${a}, ${b}))`;
          case '===': return `O.eq3(${a}, ${b})`;
          case '!==': return `(!O.eq3(${a}, ${b}))`;
          case '<': return `O.lt(${a}, ${b})`;
          case '>': return `O.lt(${b}, ${a})`;
          case '<=': return `(!O.lt(${b}, ${a}))`;
          case '>=': return `(!O.lt(${a}, ${b}))`;
          default: throw new CompileError(`不支持的运算符 ${e.op}`);
        }
      }
      case 'idx': return `O.get(${genExpr(e.obj, ctx)}, ${genExpr(e.index, ctx)})`;
      case 'member': return `O.mem(${genExpr(e.obj, ctx)}, ${JSON.stringify(e.name)})`;
      case 'call': return genCall(e, ctx);
      case 'new': {
        const args = e.args.map((a) => genExpr(a, ctx)).join(', ');
        return `$.newInst(${JSON.stringify(e.cls)}, [${args}])`;
      }
    }
  }

  function genCall(e: Extract<Expr, { k: 'call' }>, ctx: Ctx): string {
    const args = e.args.map((a) => genExpr(a, ctx)).join(', ');
    if (e.callee.k === 'var') {
      const name = e.callee.name;
      if (name === 'super') {
        if (!ctx.className) throw new CompileError('super 只能在类里用');
        return `$.callSuper(${JSON.stringify(ctx.className)}, [${args}])`;
      }
      if (getBuiltin(name)) return `B.${name}(${args})`;
      if (fnNames.has(name)) return `$.f[${JSON.stringify(name)}]($, ${args})`;
      if (classInfo.has(name)) return `$.newInst(${JSON.stringify(name)}, [${args}])`;
      if (ctx.className && methodsOf(ctx.className).has(name) && !ctx.locals.has(name)) {
        return `O.mcall($.self, ${JSON.stringify(name)}, [${args}], $)`;
      }
      return `$.callFn(${JSON.stringify(name)}, [${args}])`;
    }
    if (e.callee.k === 'member') {
      return `O.mcall(${genExpr(e.callee.obj, ctx)}, ${JSON.stringify(e.callee.name)}, [${args}], $)`;
    }
    throw new CompileError('这个表达式不能被调用');
  }
}

function defaultLiteral(type: TypeName): string {
  switch (type) {
    case 'int':
    case 'float': return '0';
    case 'str': return '""';
    case 'bool': return 'false';
    case 'list': return '[]';
    case 'dict': return 'V.dict([])';
    default: return 'null';
  }
}

function nestedBlocks(s: Stmt): Stmt[] {
  switch (s.k) {
    case 'if': return [...s.branches.flatMap((b) => b.body), ...(s.elseBody ?? [])];
    case 'while':
    case 'repeat':
    case 'forever':
    case 'waitUntil': return s.body;
    case 'forRange':
    case 'forIn': return s.body;
    default: return [];
  }
}
