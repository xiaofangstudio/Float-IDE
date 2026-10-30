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
      default: break;
    }
  };

  const targetRef = (t: Target): void => {
    if (t.k === 'var') refs.push({ name: t.name, span: t, call: false });
    else if (t.k === 'member') exprRefs(t.obj);
    else { exprRefs(t.obj); exprRefs(t.index); }
  };

  const walk = (list: Stmt[]): void => {
    for (const s of list) {
      switch (s.k) {
        case 'decl': declared.add(s.name); exprRefs(s.init); break;
        case 'assign':
          if (s.target.k === 'var') declared.add(s.target.name);
          else if (s.target.k === 'member') exprRefs(s.target.obj);
          else { exprRefs(s.target.obj); exprRefs(s.target.index); }
          exprRefs(s.value);
          break;
        case 'incdec':
          if (s.target.k === 'var') declared.add(s.target.name);
          else if (s.target.k === 'member') exprRefs(s.target.obj);
          else { exprRefs(s.target.obj); exprRefs(s.target.index); }
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

/** 在解析诊断之外，补上「未定义的名字」这类语义提示 */
export function semanticDiagnostics(file: string, body: Stmt[]): Diagnostic[] {
  const { declared, refs, hasImport } = collect(body);
  if (hasImport) return [];

  const known = new Set<string>([...declared, ...BUILTINS, ...SPECIAL, 'self', 'this', 'super', 'ans']);
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
      message: `${what}「${r.name}」没有定义${hint ? `，是不是想写「${hint}」？` : '（先声明或赋值）'}`,
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
