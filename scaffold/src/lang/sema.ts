/* 静态检查：在运行之前就把「名字写错」找出来。
 * 只做保守检查——文件里有 import 时跳过，避免把导入进来的全局变量误判。
 */

import type { Diagnostic, Expr, Stmt, Span, Target } from './ast';
import { builtinList } from './builtins';
import { SPECIAL_VARS } from './spec';

const BUILTINS = new Set(builtinList().map((b) => b.name));
const SPECIAL = new Set(SPECIAL_VARS.map((s) => s.name));

type Ref = { name: string; span: Span; call: boolean };

function collect(body: Stmt[]): { declared: Set<string>; refs: Ref[]; hasImport: boolean } {
  const declared = new Set<string>();
  const refs: Ref[] = [];
  let hasImport = false;

  const exprRefs = (e: Expr | null | undefined): void => {
    if (!e) return;
    switch (e.k) {
      case 'var': refs.push({ name: e.name, span: e, call: false }); break;
      case 'un': exprRefs(e.a); break;
      case 'bin': exprRefs(e.a); exprRefs(e.b); break;
      case 'idx': exprRefs(e.obj); exprRefs(e.index); break;
      case 'member': exprRefs(e.obj); break;
      case 'list': for (const it of e.items) exprRefs(it); break;
      case 'dict': for (const en of e.entries) { exprRefs(en.key); exprRefs(en.val); } break;
      case 'call':
        if (e.callee.k === 'var') refs.push({ name: e.callee.name, span: e.callee, call: true });
        else exprRefs(e.callee);
        for (const a of e.args) exprRefs(a);
        break;
      case 'new': for (const a of e.args) exprRefs(a); break;
      case 'lambda':
        // lambda 的参数在函数体里就是名字，先当声明过（和 funcDef 同一策略）
        for (const p of e.params) declared.add(p.name);
        exprRefs(e.body);
        break;
      case 'slice': exprRefs(e.obj); exprRefs(e.from); exprRefs(e.to); break;
      default: break;
    }
  };

  const targetRef = (t: Target): void => {
    if (t.k === 'var') refs.push({ name: t.name, span: t, call: false });
    else if (t.k === 'member') exprRefs(t.obj);
    else if (t.k === 'deref') exprRefs(t.ptr);
    else { exprRefs(t.obj); exprRefs(t.index); }
  };

  const walk = (list: Stmt[]): void => {
    for (const s of list) {
      switch (s.k) {
        case 'decl': declared.add(s.name); exprRefs(s.init); break;
        case 'assign':
          // 赋值本身就等于新建这个名字，别把它当成「引用了没定义的东西」
          if (s.target.k === 'var') declared.add(s.target.name);
          else targetRef(s.target);
          exprRefs(s.value);
          break;
        case 'incdec':
          if (s.target.k === 'var') declared.add(s.target.name);
          else targetRef(s.target);
          break;
        case 'destructure': for (const n of s.names) declared.add(n.name); exprRefs(s.value); break;
        case 'funcDef':
          declared.add(s.name);
          for (const p of s.params) { declared.add(p.name); exprRefs(p.def); }
          walk(s.body);
          break;
        case 'classDef':
          declared.add(s.name);
          for (const f of s.fields) { declared.add(f.name); exprRefs(f.init); }
          for (const m of s.methods) walk([m]);
          break;
        case 'if':
          for (const b of s.branches) { exprRefs(b.cond); walk(b.body); }
          if (s.elseBody) walk(s.elseBody);
          break;
        case 'while': exprRefs(s.cond); walk(s.body); break;
        case 'repeat': exprRefs(s.count); walk(s.body); break;
        case 'forever': walk(s.body); break;
        case 'waitUntil': exprRefs(s.cond); walk(s.body); break;
        case 'forRange':
          declared.add(s.varName);
          exprRefs(s.from); exprRefs(s.to); exprRefs(s.step);
          walk(s.body);
          break;
        case 'forIn':
          declared.add(s.varName);
          if (s.valueName) declared.add(s.valueName);
          exprRefs(s.iter);
          walk(s.body);
          break;
        case 'print': for (const a of s.args) exprRefs(a); break;
        case 'ask': exprRefs(s.question); break;
        case 'wait': exprRefs(s.seconds); break;
        case 'return': exprRefs(s.value); break;
        case 'throw': exprRefs(s.value); break;
        case 'try':
          walk(s.body);
          if (s.catchName) declared.add(s.catchName);
          if (s.catchBody) walk(s.catchBody);
          if (s.finallyBody) walk(s.finallyBody);
          break;
        case 'ifOut': exprRefs(s.cond); break;
        case 'expr': exprRefs(s.expr); break;
        case 'listAdd': exprRefs(s.value); targetRef(s.target); break;
        case 'listInsert': exprRefs(s.value); exprRefs(s.index); targetRef(s.target); break;
        case 'listReplace': exprRefs(s.index); exprRefs(s.value); targetRef(s.target); break;
        case 'listDelete': exprRefs(s.index); targetRef(s.target); break;
        case 'listRead': exprRefs(s.index); targetRef(s.target); break;
        case 'lenOf': exprRefs(s.value); break;
        case 'dictSet': exprRefs(s.key); exprRefs(s.value); targetRef(s.target); break;
        case 'dictRead': exprRefs(s.key); targetRef(s.target); break;
        case 'dictDelete': exprRefs(s.key); targetRef(s.target); break;
        case 'import': hasImport = true; break;
        default: break;
      }
    }
  };

  walk(body);
  return { declared, refs, hasImport };
}


/** 编辑距离，用来给「是不是想写 xxx」 */
function distance(a: string, b: string): number {
  const m = a.length;
  const n = b.length;
  if (m === 0) return n;
  if (n === 0) return m;
  let prev = Array.from({ length: n + 1 }, (_, j) => j);
  for (let i = 1; i <= m; i++) {
    const cur = [i];
    for (let j = 1; j <= n; j++) {
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    }
    prev = cur;
  }
  return prev[n];
}

function suggest(name: string, pool: Iterable<string>): string | null {
  let best: string | null = null;
  let bestD = 3;
  for (const cand of pool) {
    if (cand === name) continue;
    const d = distance(name, cand);
    if (d < bestD) { bestD = d; best = cand; }
  }
  return best;
}

/** 只取这个文件顶层声明的名字（跨文件总表用，不跑语义检查） */
export function toplevelNames(body: Stmt[]): string[] {
  return [...collect(body).declared];
}

/** 在解析诊断之外，补上「未定义的名字」这类语义提示
 * `project` 是「本文件 + 它 import 的文件」汇总出来的名字总表：
 * 有 import 的文件不再一律免检，而是先查总表，表里也没有才报「没定义」。
 */
export function semanticDiagnostics(file: string, body: Stmt[], project?: Set<string>): Diagnostic[] {
  const { declared, refs, hasImport } = collect(body);
  // 有 import 又拿不到总表（依赖还没解析出来）：保持沉默，宁可不报也不误报
  if (hasImport && !project) return [];

  const known = new Set<string>([...declared, ...BUILTINS, ...SPECIAL, 'self', 'this', 'super', 'ans']);
  if (project) for (const n of project) known.add(n);
  const out: Diagnostic[] = [];
  const seen = new Set<string>();

  for (const r of refs) {
    if (known.has(r.name)) continue;
    const key = `${r.name}@${r.span.line}:${r.span.col}`;
    if (seen.has(key)) continue;
    seen.add(key);

    const hint = suggest(r.name, known);
    const what = r.call ? '函数' : '变量';
    out.push({
      severity: 'warning',
      code: r.call ? 'undefined-fn' : 'undefined-var',
      message: `${what}「${r.name}」没有定义${hint ? `，是不是想写「${hint}」？` : (hasImport ? '（本文件和它 import 的文件里都没有）' : '（先声明或赋值）')}`,
      span: {
        file,
        line: r.span.line,
        col: r.span.col,
        endLine: r.span.endLine,
        endCol: r.span.endCol
      }
    });
  }
  return out;
}

/** 方法里参数 / 局部变量与字段同名且被赋值：赋值只改局部，字段不会变。
 *  这是静默坑——本意改 self.hp 结果只改了局部。只提示，不改行为
 *  （方法内先找局部再找字段是有意设计）。 */
export function shadowDiagnostics(file: string, body: Stmt[]): Diagnostic[] {
  const out: Diagnostic[] = [];
  const clsMap = new Map<string, Extract<Stmt, { k: 'classDef' }>>();

  const gather = (list: Stmt[]): void => {
    for (const s of list) {
      if (s.k === 'classDef') {
        clsMap.set(s.name, s);
        gather(s.methods);
      }
    }
  };
  gather(body);

  /** 字段集 = 自己的 + 沿 extends 链收上来的（继承字段也识别） */
  const fieldsOf = (name: string): Set<string> | null => {
    const cls = clsMap.get(name);
    if (!cls) return null;
    const all = new Set(cls.fields.map((f) => f.name));
    let base = cls.base;
    const seen = new Set([name]);
    while (base && !seen.has(base)) {
      seen.add(base);
      const b = clsMap.get(base);
      if (!b) break;
      for (const f of b.fields) all.add(f.name);
      base = b.base;
    }
    return all;
  };

  for (const cls of clsMap.values()) {
    const fields = fieldsOf(cls.name);
    if (!fields || fields.size === 0) continue;
    for (const m of cls.methods) {
      if (m.k !== 'funcDef') continue;
      const params = new Set(m.params.map((p) => p.name));
      const locals = new Set<string>();
      const warned = new Set<string>();
      const warnAt = (name: string, role: string, span: Span): void => {
        if (warned.has(name)) return;
        warned.add(name);
        out.push({
          severity: 'warning',
          code: 'shadow-field',
          message: `方法「${m.name}」里的${role}「${name}」和字段 self.${name} 同名：` +
            `这里的赋值只改${role}，self.${name} 不会变（要改字段就写 self.${name} = …）`,
          span: { file, line: span.line, col: span.col, endLine: span.endLine, endCol: span.endCol }
        });
      };
      const scan = (list: Stmt[]): void => {
        for (const s of list) {
          switch (s.k) {
            case 'decl':
              if (fields.has(s.name) && !params.has(s.name) && !locals.has(s.name)) {
                warnAt(s.name, '局部变量', s);
              }
              locals.add(s.name);
              break;
            case 'assign':
            case 'incdec':
              if (s.target.k === 'var') {
                const n = s.target.name;
                if (fields.has(n) && params.has(n)) warnAt(n, '参数', s);
                else if (fields.has(n) && locals.has(n)) warnAt(n, '局部变量', s);
              }
              break;
            case 'if':
              for (const b of s.branches) scan(b.body);
              if (s.elseBody) scan(s.elseBody);
              break;
            case 'while':
            case 'repeat':
            case 'forever':
            case 'forRange':
            case 'forIn':
              scan(s.body);
              break;
            case 'try':
              scan(s.body);
              if (s.catchBody) scan(s.catchBody);
              if (s.finallyBody) scan(s.finallyBody);
              break;
            default:
              break;
          }
        }
      };
      scan(m.body);
    }
  }
  return out;
}
