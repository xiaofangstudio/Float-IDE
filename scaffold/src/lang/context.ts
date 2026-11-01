/* 光标在语法里的位置 —— 编辑器里所有智能功能（补全 / 悬停 / 参数提示 / 重命名）
 * 都问这一个函数，不再各自拿正则去猜当前行。
 *
 * 判断依据只有两样东西：
 *   1. 词法单元（token）：字符串/注释边界、`.` 前面是谁、是不是在起名字；
 *   2. AST：当前语句、所在类、变量类型。
 *
 * 这样"改一处"就对所有代码生效：新增语法只要在 AST/词法里表达清楚，
 * 上面这些功能自动跟着对，不必再给每个语法补一条正则。
 */

import type { Analysis, ClassSym } from './analyze';
import { analyze } from './analyze';
import { childBodies, type Expr, type Stmt } from './ast';
import { codePart, tokenizeLine, type Token } from './token';

export type CursorContext =
  /** 光标在注释里 */
  | { kind: 'comment' }
  /** 光标在普通字符串里（不该补全） */
  | { kind: 'string' }
  /** 光标在 `import "…"` 的路径里：要补文件名 */
  | { kind: 'importPath'; typed: string }
  /** 光标正在给新东西起名字（声明 / 函数 / 类 / 循环变量 / 参数 / 字段） */
  | { kind: 'naming'; what: 'decl' | 'func' | 'class' | 'for' | 'param' }
  /** 光标在 `对象.` 后面：要补这个对象的成员 */
  | { kind: 'member'; object: string; type: string | null; cls: string | null }
  /** 普通代码位置 */
  | { kind: 'code' };

/** 起名字的关键字（这些词后面紧跟的标识符是"新名字"，别去骚扰） */
const NAMING_HEADS: Record<string, 'decl' | 'func' | 'class' | 'for'> = {
  int: 'decl', float: 'decl', str: 'decl', bool: 'decl',
  list: 'decl', dict: 'decl', ptr: 'decl', var: 'decl',
  fn: 'func', function: 'func', func: 'func', Function: 'func',
  class: 'class',
  for: 'for'
};

/** 光标是不是落在某个 token 里（含紧贴它末尾的位置，因为用户正打到一半） */
function inToken(t: Token, col: number, allowTail = true): boolean {
  if (col < t.col) return false;
  return allowTail ? col <= t.endCol : col < t.endCol;
}

/** 找到光标所在行里，位置在 col 之前（含）的最后一个 token 下标 */
function lastTokenAt(toks: Token[], col: number): number {
  let at = -1;
  for (let i = 0; i < toks.length; i++) {
    if (toks[i].k === 'eof') break;
    if (toks[i].col < col) at = i;
  }
  return at;
}

export function contextAt(src: string, line: number, col: number, analysis?: Analysis): CursorContext {
  const lines = src.split('\n');
  const text = lines[line - 1] ?? '';
  const a = analysis ?? analyze('main.cw', src);

  /* ── 1. 注释：光标落在 `--` 之后的注释文字里 ── */
  // codePart 返回的 head 就是 `--` 之前那段，它的长度就是注释起始下标
  const head = codePart(text).code;
  const commentStart = head.length;
  if (commentStart < text.length && col - 1 > commentStart) {
    return { kind: 'comment' };
  }
  const code = head;

  /* ── 2. 词法：字符串 / 数字 / 标识符 ── */
  let toks: Token[] = [];
  try {
    toks = tokenizeLine(code, line, 1);
  } catch {
    toks = [];
  }

  const strTok = toks.find((t) => t.k === 'str' && col > t.col && col < t.endCol);
  if (strTok) {
    // import "…" 里的字符串：补文件名
    const stmt = importAt(a, line);
    if (stmt) {
      const typo = text.slice(strTok.col, col - 1);
      return { kind: 'importPath', typed: typo };
    }
    return { kind: 'string' };
  }

  const wordStart = wordStartCol(text, col);
  const word = text.slice(wordStart - 1, col - 1);

  /* ── 3. 成员：`对象.` 后面 ── */
  const memberObj = memberObjectBefore(toks, col);
  if (memberObj) {
    const { type, cls } = typeOf(a, memberObj, line);
    return { kind: 'member', object: memberObj, type, cls };
  }

  /* ── 4. 起名字：声明 / 函数 / 类 / for / 参数 ── */
  const naming = namingAt(a, line, wordStart, word, toks);
  if (naming) return { kind: 'naming', what: naming };

  return { kind: 'code' };
}

/** 光标所在词的开头列（1 基）：中文/字母/数字/下划线都算同一个词 */
function wordStartCol(text: string, col: number): number {
  let i = col - 1;
  while (i > 0 && /[\w\u4e00-\u9fa5]/.test(text[i - 1] ?? '')) i--;
  return i + 1;
}

/** `a.` / `self.` / `xs[-1].` 之后取对象名；不是成员访问返回 null */
function memberObjectBefore(toks: Token[], col: number): string | null {
  const at = lastTokenAt(toks, col);
  if (at < 0) return null;
  // 允许 `.` 后面已经打了半个名字
  let i = at;
  const cur = toks[i];
  if (cur.k === 'ident' && cur.col <= col && col <= cur.endCol) i--;   // 正在打成员名
  if (i < 1 || toks[i].v !== '.') return null;
  const obj = toks[i - 1];
  return obj.k === 'ident' ? obj.v : null;
}

/** 光标是不是正在"起名字"：优先用 AST 里记录的名字列号，
 *  AST 里还没有这个节点（代码刚写一半）就退回词法判断。 */
function namingAt(
  a: Analysis, line: number, wordStart: number, word: string, toks: Token[]
): 'decl' | 'func' | 'class' | 'for' | 'param' | null {
  // ① AST：声明 / 函数 / 类 / 循环变量 / 参数 / 字段，都记了精确列号
  for (const st of walkStmts(a.program.body)) {
    if (st.k === 'funcDef') {
      for (const p of st.params) {
        if (p.line === line && sameWord(p.col, p.name, wordStart, word)) return 'param';
      }
      if (st.line === line && sameWord(st.nameCol, st.name, wordStart, word)) return 'func';
    }
    if (st.k === 'classDef') {
      if (st.line === line && sameWord(st.nameCol, st.name, wordStart, word)) return 'class';
      for (const f of st.fields) {
        if (f.line === line && sameWord(f.col, f.name, wordStart, word)) return 'decl';
      }
    }
    if (st.k === 'decl' && st.line === line && sameWord(st.nameCol, st.name, wordStart, word)) return 'decl';
    if ((st.k === 'forRange' || st.k === 'forIn') && st.line === line
      && sameWord(st.nameCol, st.varName, wordStart, word)) return 'for';
  }

  // ② 词法兜底：`int ▮` / `fn ▮` / `class ▮` / `for ▮` —— 这时 AST 里还没节点
  const head = toks.find((t) => (t.k === 'ident' || t.k === 'kw') && NAMING_HEADS[t.v]);
  if (head && wordStart > head.endCol) {
    const kind = NAMING_HEADS[head.v];
    // 关键字后面紧跟的那个词就是新名字（空着也算）
    const next = toks.find((t) => t.col > head.endCol && t.k !== 'eof');
    if (!next) return kind;
    if (next.k === 'ident' && next.col === wordStart) return kind;
  }
  return null;
}

function sameWord(astCol: number, astName: string, wordStart: number, word: string): boolean {
  // 允许用户还在打字：光标词是名字的前缀
  if (wordStart !== astCol) return false;
  return astName.startsWith(word) || word === '';
}

/** 遍历所有语句（含嵌套体） */
function* walkStmts(body: Stmt[]): Generator<Stmt> {
  for (const s of body) {
    yield s;
    for (const child of childBodies(s)) yield* walkStmts(child);
  }
}

/** 这一行的 import 语句 */
function importAt(a: Analysis, line: number): Extract<Stmt, { k: 'import' }> | null {
  for (const s of walkStmts(a.program.body)) {
    if (s.k === 'import' && s.line === line) return s;
  }
  return null;
}

/** 变量的类型信息：内建类型看声明，类实例看类名 */
function typeOf(a: Analysis, name: string, line: number): { type: string | null; cls: string | null } {
  if (name === 'self' || name === 'this') {
    const c = enclosingClass(a, line);
    return { type: c ? c.name : null, cls: c ? c.name : null };
  }
  const v = a.vars.filter((x) => x.name === name).pop();
  if (!v) return { type: null, cls: null };
  return { type: v.type, cls: v.cls };
}

/** 光标所在的最内层类（靠 AST 的 span 包含关系，不是"往上找最近的 class 行"） */
export function enclosingClass(a: Analysis, line: number): ClassSym | null {
  let best: ClassSym | null = null;
  const visit = (stmts: Stmt[], current: ClassSym | null): void => {
    for (const s of stmts) {
      const cls = s.k === 'classDef' ? a.classes.find((c) => c.name === s.name) ?? null : current;
      if (s.k === 'classDef' && line >= s.line && line <= s.endLine) best = pickInner(best, cls);
      for (const child of childBodies(s)) visit(child, cls);
    }
  };
  visit(a.program.body, null);
  return best;
}

function pickInner(a: ClassSym | null, b: ClassSym | null): ClassSym | null {
  if (!a) return b;
  if (!b) return a;
  return b.line > a.line ? b : a;
}

/** 表达式里所有成员访问的目标（给悬停/跳转复用） */
export function memberExprAt(expr: Expr, line: number, col: number): Extract<Expr, { k: 'member' }> | null {
  if (line < expr.line || line > expr.endLine) return null;
  if (line === expr.line && col < expr.col) return null;
  if (line === expr.endLine && col > expr.endCol) return null;
  if (expr.k === 'member') {
    const inner = memberExprAt(expr.obj, line, col);
    return inner ?? expr;
  }
  for (const e of childExprs(expr)) {
    const hit = memberExprAt(e, line, col);
    if (hit) return hit;
  }
  return null;
}

function childExprs(e: Expr): Expr[] {
  switch (e.k) {
    case 'bin': return [e.a, e.b];
    case 'ternary': return [e.cond, e.a, e.b];
    case 'un': return [e.a];
    case 'idx': return [e.obj, e.index];
    case 'member': return [e.obj];
    case 'call': return [e.callee, ...e.args];
    case 'new': return e.args;
    case 'list': return e.items;
    case 'dict': return e.entries.flatMap((x) => [x.key, x.val]);
    default: return [];
  }
}
