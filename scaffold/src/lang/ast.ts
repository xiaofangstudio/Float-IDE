/* CW Script AST：语句与表达式两套节点，全部带文件/行/列。
 *
 * 设计原则：
 * 1. 指令参数不再是"字符串"，而是 Expr —— 所以 `for i in range(n)`、`add(a+1, b)`
 *    这类写法天然成立，不需要每处再补正则。
 * 2. 语句种类按语义分（循环 / 函数 / 类 / 集合操作 / IO），执行器与 IDE 分析共用同一棵树。
 * 3. Span 里的 file 供断点、出错定位、多文件调用栈使用。
 */

export type Span = {
  file: string;
  line: number;
  col: number;
  endLine: number;
  endCol: number;
};

export type TypeName = 'int' | 'float' | 'str' | 'bool' | 'list' | 'dict' | 'var';

/* ───────────────────────────── 表达式 ───────────────────────────── */

export type Expr =
  | ({ k: 'num'; v: number } & Span)
  | ({ k: 'str'; v: string } & Span)
  | ({ k: 'bool'; v: boolean } & Span)
  | ({ k: 'null' } & Span)
  | ({ k: 'var'; name: string } & Span)
  | ({ k: 'self' } & Span)
  | ({ k: 'list'; items: Expr[] } & Span)
  | ({ k: 'dict'; entries: { key: Expr; val: Expr }[] } & Span)
  | ({ k: 'un'; op: string; a: Expr } & Span)
  | ({ k: 'bin'; op: string; a: Expr; b: Expr } & Span)
  | ({ k: 'idx'; obj: Expr; index: Expr } & Span)
  | ({ k: 'member'; obj: Expr; name: string } & Span)
  | ({ k: 'call'; callee: Expr; args: Expr[] } & Span)
  | ({ k: 'new'; cls: string; args: Expr[] } & Span);

/* ───────────────────────────── 语句 ───────────────────────────── */

export type Param = {
  name: string;
  type: TypeName | null;
  def: Expr | null;
  line: number;
  col: number;
};

export type FieldDecl = {
  name: string;
  type: TypeName | null;
  init: Expr | null;
  line: number;
  col: number;
};

/** 可赋值位置：变量 / 成员 / 下标 */
export type Target =
  | ({ k: 'var'; name: string } & Span)
  | ({ k: 'member'; obj: Expr; name: string } & Span)
  | ({ k: 'idx'; obj: Expr; index: Expr } & Span);

export type AssignOp = '=' | '+=' | '-=' | '*=' | '/=' | '%=' | '**=';

export type IfBranch = { cond: Expr | null; body: Stmt[]; line: number };

export type Stmt =
  | ({ k: 'decl'; type: TypeName; name: string; init: Expr | null } & Span)
  | ({ k: 'assign'; target: Target; op: AssignOp; value: Expr } & Span)
  | ({ k: 'incdec'; target: Target; delta: 1 | -1; prefix: boolean } & Span)
  | ({ k: 'expr'; expr: Expr } & Span)
  | ({ k: 'print'; args: Expr[] } & Span)
  | ({ k: 'ask'; question: Expr } & Span)
  | ({ k: 'if'; branches: IfBranch[]; elseBody: Stmt[] | null } & Span)
  | ({ k: 'while'; cond: Expr; body: Stmt[] } & Span)
  | ({ k: 'repeat'; count: Expr; body: Stmt[] } & Span)
  | ({ k: 'forever'; body: Stmt[] } & Span)
  | ({ k: 'forRange'; varName: string; from: Expr | null; to: Expr; step: Expr | null; inclusive: boolean; body: Stmt[] } & Span)
  | ({ k: 'forIn'; varName: string; valueName: string | null; iter: Expr; body: Stmt[] } & Span)
  | ({ k: 'waitUntil'; cond: Expr; body: Stmt[] } & Span)
  | ({ k: 'wait'; seconds: Expr } & Span)
  | ({ k: 'break' } & Span)
  | ({ k: 'continue' } & Span)
  | ({ k: 'stop' } & Span)
  | ({ k: 'return'; value: Expr | null } & Span)
  | ({ k: 'throw'; value: Expr } & Span)
  | ({ k: 'try'; body: Stmt[]; catchName: string | null; catchBody: Stmt[] | null; finallyBody: Stmt[] | null } & Span)
  | ({ k: 'destructure'; names: { name: string; line: number; col: number }[]; value: Expr } & Span)
  | ({ k: 'ifOut'; cond: Expr } & Span)
  | ({ k: 'funcDef'; name: string; params: Param[]; retType: string | null; body: Stmt[] } & Span)
  | ({ k: 'classDef'; name: string; base: string | null; fields: FieldDecl[]; methods: Stmt[] } & Span)
  | ({ k: 'import'; target: string } & Span)
  | ({ k: 'listAdd'; value: Expr; target: Target } & Span)
  | ({ k: 'listInsert'; value: Expr; target: Target; index: Expr } & Span)
  | ({ k: 'listReplace'; target: Target; index: Expr; value: Expr } & Span)
  | ({ k: 'listDelete'; target: Target; index: Expr | null } & Span)
  | ({ k: 'listRead'; target: Target; index: Expr } & Span)
  | ({ k: 'lenOf'; value: Expr } & Span)
  | ({ k: 'dictSet'; target: Target; key: Expr; value: Expr } & Span)
  | ({ k: 'dictRead'; target: Target; key: Expr } & Span)
  | ({ k: 'dictDelete'; target: Target; key: Expr } & Span)
  | ({ k: 'timeRead' } & Span)
  | ({ k: 'timeReset' } & Span)
  | ({ k: 'comment'; text: string; group: boolean } & Span)
  | ({ k: 'unknown'; raw: string; hint: string | null } & Span);

export type Severity = 'error' | 'warning' | 'info';

export type Diagnostic = {
  severity: Severity;
  message: string;
  code: string;
  span: Span;
};

export type Program = {
  file: string;
  body: Stmt[];
  diagnostics: Diagnostic[];
};

/* ───────────────────────────── 类型判定工具 ───────────────────────────── */

export const BLOCK_STMTS = new Set<Stmt['k']>([
  'if', 'while', 'repeat', 'forever', 'forRange', 'forIn', 'waitUntil', 'funcDef', 'classDef', 'try'
]);

export function isBlock(s: Stmt): boolean {
  return BLOCK_STMTS.has(s.k);
}

/** 语句是否会产生一个可停靠的"执行步"（调试单步、变量快照） */
export function isExecutable(s: Stmt): boolean {
  return s.k !== 'comment' && s.k !== 'funcDef' && s.k !== 'classDef' && s.k !== 'import' && s.k !== 'unknown';
}
