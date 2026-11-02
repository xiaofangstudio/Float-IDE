/* 类型检查：让 `int hp = "abc"` 这种真的报错，而不是写着好看。
 *
 * 设计原则（跟编辑器智能一样，只做一次、对全部代码生效）：
 *   · 走 AST，不认源码文本；
 *   · 推断不出来就闭嘴（any），绝不瞎报错；
 *   · 只在"铁定不对"的地方报：声明初值、赋值、函数返回值、参数。
 *
 * 类型集合：int / float / str / bool / list / dict / ptr / void
 *   —— int 只是最常用的那个，不是唯一的类型。
 *   类名也当类型用（`Person p = new Person()`）。
 */

import type { Expr, Param, Stmt, TypeName } from './ast';
import type { Diagnostic } from './ast';
import type { TypeName as TN } from './ast';

/** 推断出来的类型：内置类型名、类名、unknown（推断不出来）、null（还没值） */
export type Ty = string;

const NUMERIC = new Set(['int', 'float']);

export type TypeEnv = {
  /** 变量名 → 类型 */
  vars: Map<string, Ty>;
  /** 函数名 → 返回类型 */
  funcs: Map<string, Ty>;
  /** 类名 → { 字段类型, 方法返回类型 } */
  classes: Map<string, { fields: Map<string, Ty>; methods: Map<string, Ty> }>;
  file: string;
  out: Diagnostic[];
};

function isClassType(env: TypeEnv, t: Ty): boolean {
  return env.classes.has(t);
}

/** 内置类型名（含 void） */
const BUILTIN_TYPES = new Set(['int', 'float', 'str', 'bool', 'list', 'dict', 'ptr', 'void']);

function typeName(t: Ty): string {
  return t === 'unknown' ? '未知类型' : t;
}

/** 这个值能不能放进"声明成 dst 类型"的变量里 */
function assignable(dst: Ty, src: Ty): boolean {
  if (dst === 'var' || dst === 'void') return true;      // var：不限定
  if (src === 'unknown' || src === 'null') return true;   // 推断不出来就别管
  if (dst === src) return true;
  if (dst === 'float' && src === 'int') return true;      // 整数放进 float 天经地义
  return false;
}

/** 报错文案：说清是什么类型、为什么不行、怎么改 */
function mismatch(dst: Ty, src: Ty, what: string): string {
  const fixes: string[] = [];
  if (dst === 'int' && src === 'float') fixes.push('要取整就写 int(值) 或 idiv(...)');
  else if (dst === 'int' && src === 'str') fixes.push('要转数字就写 int("…") 或 float("…")');
  else if (dst === 'str') fixes.push('要转文本就写 str(值)，或者用 "…" 插值');
  else if (dst === 'bool') fixes.push('要转布尔就写 bool(值)');
  const hint = fixes.length ? `；${fixes[0]}` : '';
  return `${what}声明成 ${dst}，但这个值是 ${typeName(src)}${hint}`;
}

/* ───────────────────────── 表达式类型推断 ───────────────────────── */

export function inferType(e: Expr, env: TypeEnv): Ty {
  switch (e.k) {
    case 'num': return Number.isInteger(e.v) ? 'int' : 'float';
    case 'str': return 'str';
    case 'bool': return 'bool';
    case 'null': return 'null';
    case 'list': return 'list';
    case 'dict': return 'dict';
    case 'self': return env.vars.get('self') ?? 'unknown';
    case 'var': return env.vars.get(e.name) ?? 'unknown';
    case 'new': return env.classes.has(e.cls) ? e.cls : 'unknown';
    case 'un': {
      /* &x 取地址 → ptr；*p 解引用 → 指向什么由那块内存决定，检查器不猜。
       * 以前这行按"数值运算"处理，导致 ptr p = &x 被误报成「值是 int」。 */
      if (e.op === '&') return 'ptr';
      if (e.op === '*') return 'unknown';
      const a = inferType(e.a, env);
      if (e.op === '!') return 'bool';
      return NUMERIC.has(a) ? a : 'unknown';
    }
    case 'ternary': {
      const a = inferType(e.a, env);
      const b = inferType(e.b, env);
      return a === b ? a : (a === 'unknown' ? b : (b === 'unknown' ? a : 'unknown'));
    }
    case 'bin': {
      const a = inferType(e.a, env);
      const b = inferType(e.b, env);
      switch (e.op) {
        case '==': case '!=': case '===': case '!==':
        case '<': case '>': case '<=': case '>=':
        case '&&': case '||':
          return 'bool';
        case '+':
          // 有一边是字符串就是拼接
          if (a === 'str' || b === 'str') return 'str';
          if (a === 'float' || b === 'float') return 'float';
          if (a === 'int' && b === 'int') return 'int';
          return 'unknown';
        case '-': case '*': case '%': case '**':
          if (a === 'int' && b === 'int') return 'int';
          if (NUMERIC.has(a) && NUMERIC.has(b)) return 'float';
          return 'unknown';
        case '/':
          if (NUMERIC.has(a) && NUMERIC.has(b)) return 'float';
          return 'unknown';
        case '//':
          if (NUMERIC.has(a) && NUMERIC.has(b)) return 'int';
          return 'unknown';
        default:
          return 'unknown';
      }
    }
    case 'idx': {
      const obj = inferType(e.obj, env);
      if (obj === 'list' || obj === 'dict') return 'unknown';
      if (obj === 'str') return 'str';
      return 'unknown';
    }
    case 'member': {
      const obj = inferType(e.obj, env);
      const cls = env.classes.get(obj);
      if (cls) return cls.fields.get(e.name) ?? cls.methods.get(e.name) ?? 'unknown';
      if (obj === 'str') return 'str';
      return 'unknown';
    }
    case 'call': {
      if (e.callee.k === 'var') {
        const f = env.funcs.get(e.callee.name);
        if (f) return f;
        if (e.callee.name === 'str') return 'str';
        if (e.callee.name === 'int' || e.callee.name === 'len' || e.callee.name === 'idiv') return 'int';
        if (e.callee.name === 'float' || e.callee.name === 'sqrt' || e.callee.name === 'near') {
          return e.callee.name === 'near' ? 'bool' : 'float';
        }
        if (e.callee.name === 'bool') return 'bool';
        if (e.callee.name === 'alloc') return 'ptr';
        if (e.callee.name === 'range') return 'list';
      }
      if (e.callee.k === 'member') {
        const objTy = inferType(e.callee.obj, env);
        const cls = env.classes.get(objTy);
        if (cls) return cls.methods.get(e.callee.name) ?? 'unknown';
        if (objTy === 'str') return 'str';
      }
      return 'unknown';
    }
    default:
      return 'unknown';
  }
}

/* ───────────────────────── 收集环境 ───────────────────────── */

function collectEnv(env: TypeEnv, body: Stmt[]): void {
  for (const s of body) {
    if (s.k === 'funcDef') {
      const ret = (s.retType as Ty) ?? 'unknown';
      env.funcs.set(s.name, ret);
      const saved = new Map(env.vars);
      env.vars.set('self', 'unknown');
      for (const p of s.params) if (p.type) env.vars.set(p.name, p.type as Ty);
      collectEnv(env, s.body);
      env.vars = saved;
      continue;
    }
    if (s.k === 'classDef') {
      const fields = new Map<string, Ty>();
      for (const f of s.fields) fields.set(f.name, (f.type as Ty) ?? 'unknown');
      const methods = new Map<string, Ty>();
      for (const m of s.methods) {
        if (m.k === 'funcDef') methods.set(m.name, (m.retType as Ty) ?? 'unknown');
      }
      env.classes.set(s.name, { fields, methods });
    }
    if (s.k === 'decl' && s.type !== 'var') env.vars.set(s.name, s.type as Ty);
    for (const child of childBodiesOf(s)) collectEnv(env, child);
  }
}

function childBodiesOf(s: Stmt): Stmt[][] {
  switch (s.k) {
    case 'if': return [...s.branches.map((b) => b.body), ...(s.elseBody ? [s.elseBody] : [])];
    case 'while': case 'repeat': case 'forever': case 'forRange': case 'forIn':
    case 'waitUntil':
      return [s.body];
    case 'try':
      return [s.body, ...(s.catchBody ? [s.catchBody] : []), ...(s.finallyBody ? [s.finallyBody] : [])];
    default: return [];
  }
}

function diag(env: TypeEnv, severity: 'error' | 'warning', code: string, message: string, span: { line: number; col: number; endLine: number; endCol: number }): void {
  env.out.push({
    severity,
    code,
    message,
    span: { file: env.file, line: span.line, col: span.col, endLine: span.endLine, endCol: span.endCol }
  });
}

/* ───────────────────────── 逐句检查 ───────────────────────── */

function checkStmts(env: TypeEnv, body: Stmt[], retType: Ty | null): void {
  for (const s of body) {
    switch (s.k) {
      case 'decl': {
        env.vars.set(s.name, s.type as Ty);
        if (!s.init) break;
        const got = inferType(s.init, env);
        if (s.type !== 'var' && got !== 'unknown' && !assignable(s.type as Ty, got)) {
          diag(env, 'error', 'type-mismatch', mismatch(s.type as Ty, got, `变量「${s.name}」`), s);
        }
        break;
      }
      case 'assign': {
        if (s.target.k === 'var') {
          const want = env.vars.get(s.target.name);
          const got = inferType(s.value, env);
          // 赋值只报警告：动态语言里变量中途换内容是允许的，但类型标注了就该守住
          if (want && want !== 'var' && got !== 'unknown' && !assignable(want, got)) {
            diag(env, 'warning', 'assign-type',
              `${mismatch(want, got, `变量「${s.target.name}」`)}（第 ${s.line} 行）`, s);
          }
        }
        break;
      }
      case 'return': {
        if (!retType || retType === 'void') {
          if (retType === 'void' && s.value) {
            diag(env, 'error', 'return-type', '这个函数声明成 void（不返回值），就不要写 return 值了', s);
          }
          break;
        }
        const got = s.value ? inferType(s.value, env) : 'null';
        if (got !== 'unknown' && got !== 'null' && !assignable(retType, got)) {
          diag(env, 'error', 'return-type', mismatch(retType, got, '函数返回'), s);
        }
        break;
      }
      case 'funcDef': {
        const saved = new Map(env.vars);
        for (const p of s.params) {
          if (p.type) env.vars.set(p.name, p.type as Ty);
          if (p.def) {
            const got = inferType(p.def, env);
            if (p.type && got !== 'unknown' && !assignable(p.type as Ty, got)) {
              diag(env, 'error', 'param-type', mismatch(p.type as Ty, got, `参数「${p.name}」`), {
                line: p.line, col: p.col, endLine: p.line, endCol: p.col + p.name.length
              });
            }
          }
        }
        checkStmts(env, s.body, (s.retType as Ty) ?? null);
        env.vars = saved;
        break;
      }
      case 'classDef': {
        const saved = new Map(env.vars);
        env.vars.set('self', s.name);
        // 字段初值也要对得上类型：`int n = "x"` 直接报错
        for (const f of s.fields) {
          if (!f.init || !f.type) continue;
          const got = inferType(f.init, env);
          if (got !== 'unknown' && !assignable(f.type as Ty, got)) {
            diag(env, 'error', 'field-type', mismatch(f.type as Ty, got, `字段「${f.name}」`), {
              line: f.line, col: f.col, endLine: f.line, endCol: f.col + f.name.length
            });
          }
        }
        for (const m of s.methods) checkStmts(env, [m], null);
        env.vars = saved;
        break;
      }
      case 'forRange':
      case 'forIn': {
        const saved = new Map(env.vars);
        env.vars.set(s.varName, 'unknown');
        checkStmts(env, s.body, retType);
        env.vars = saved;
        break;
      }
      default: {
        for (const child of childBodiesOf(s)) checkStmts(env, child, retType);
      }
    }
  }
}

/** 对一个程序做类型检查 */
export function checkTypes(file: string, body: Stmt[]): Diagnostic[] {
  const env: TypeEnv = {
    vars: new Map(),
    funcs: new Map(),
    classes: new Map(),
    file,
    out: []
  };
  collectEnv(env, body);
  checkStmts(env, body, null);
  // 同一条只报一次（语法残缺时会重复走到）
  const seen = new Set<string>();
  return env.out.filter((d) => {
    const k = `${d.code}:${d.span.line}:${d.span.col}:${d.message}`;
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

/** 供别处查"这个类型名合不合法" */
export function isKnownType(t: string): boolean {
  return BUILTIN_TYPES.has(t);
}

export type { TN };
