/* 语法树之上的一层：把 AST 压成 IDE 需要的符号表（函数、类、字段、变量、大纲）。
 *
 * 编辑器补全、悬停、成员提示、大纲都吃这一份分析结果，不再用正则扫源码。
 */

import type { Diagnostic, Param, Program, Stmt } from './ast';
import { parseProgram } from './parse';
import { semanticDiagnostics } from './sema';

export type FuncSym = {
  name: string;
  params: Param[];
  line: number;
  retType: string | null;
  /** 所在类（方法）；顶层函数为 null */
  owner: string | null;
  doc: string | null;
};

export type FieldSym = { name: string; type: string | null; line: number };

export type ClassSym = {
  name: string;
  base: string | null;
  fields: FieldSym[];
  methods: FuncSym[];
  line: number;
};

export type VarSym = {
  name: string;
  type: string;
  line: number;
  /** 已知属于哪个类（`Person p = new Person()` 之类） */
  cls: string | null;
};

export type OutlineIcon = 'group' | 'section' | 'func' | 'var' | 'class' | 'method' | 'field';
export type OutlineItem = { line: number; text: string; icon: OutlineIcon; depth: number };

export type Analysis = {
  program: Program;
  funcs: FuncSym[];
  classes: ClassSym[];
  vars: VarSym[];
  outline: OutlineItem[];
  diagnostics: Diagnostic[];
};

const isClassDecl = (s: Stmt): s is Extract<Stmt, { k: 'decl' }> => s.k === 'decl';

export function analyze(file: string, src: string): Analysis {
  const program = parseProgram(file, src);
  const funcs: FuncSym[] = [];
  const classes: ClassSym[] = [];
  const vars: VarSym[] = [];
  const outline: OutlineItem[] = [];

  const classTypeHint = (s: Extract<Stmt, { k: 'decl' }>): string | null => {
    if (s.init && s.init.k === 'new') return s.init.cls;
    if (s.type !== 'var' && s.type !== 'int' && s.type !== 'float' && s.type !== 'str' && s.type !== 'bool' && s.type !== 'list' && s.type !== 'dict') return s.type;
    return null;
  };

  const walk = (body: Stmt[], owner: string | null, depth: number): void => {
    for (const s of body) {
      switch (s.k) {
        case 'comment': {
          const pad = '  '.repeat(depth);
          if (s.group) outline.push({ line: s.line, text: s.text.replace(/^[─-]+\s*|\s*[─-]+$/g, ''), icon: 'group', depth });
          void pad;
          break;
        }
        case 'funcDef': {
          const sym: FuncSym = {
            name: s.name, params: s.params, line: s.line, retType: s.retType, owner, doc: null
          };
          if (owner) {
            const cls = classes.find((c) => c.name === owner);
            cls?.methods.push(sym);
            outline.push({ line: s.line, text: `${s.name}(${s.params.map((p) => p.name).join(', ')})`, icon: 'method', depth });
          } else {
            funcs.push(sym);
            outline.push({ line: s.line, text: `${s.name}(${s.params.map((p) => p.name).join(', ')})`, icon: 'func', depth });
          }
          for (const p of s.params) vars.push({ name: p.name, type: p.type ?? 'var', line: p.line, cls: null });
          walk(s.body, owner, depth + 1);
          break;
        }
        case 'classDef': {
          const methods: FuncSym[] = [];
          const fields: FieldSym[] = [];
          const sym: ClassSym = { name: s.name, base: s.base, fields, methods, line: s.line };
          classes.push(sym);
          outline.push({ line: s.line, text: s.name + (s.base ? ` : ${s.base}` : ''), icon: 'class', depth });
          for (const f of s.fields) {
            fields.push({ name: f.name, type: f.type, line: f.line });
            outline.push({ line: f.line, text: f.name, icon: 'field', depth: depth + 1 });
          }
          walk(s.methods, s.name, depth + 1);
          break;
        }
        case 'decl': {
          const cls = classTypeHint(s);
          vars.push({ name: s.name, type: s.type, line: s.line, cls });
          if (depth === 0 && isClassDecl(s)) {
            const label = cls ?? (s.type === 'var' ? '' : s.type);
            outline.push({ line: s.line, text: `${label} ${s.name}`.trim(), icon: 'var', depth });
          }
          break;
        }
        case 'if':
          for (const b of s.branches) walk(b.body, owner, depth);
          if (s.elseBody) walk(s.elseBody, owner, depth);
          break;
        case 'while':
        case 'repeat':
        case 'forever':
        case 'waitUntil':
          walk(s.body, owner, depth);
          break;
        case 'forRange':
        case 'forIn':
          vars.push({ name: s.varName, type: 'int', line: s.line, cls: null });
          if (s.k === 'forIn' && s.valueName) vars.push({ name: s.valueName, type: 'var', line: s.line, cls: null });
          walk(s.body, owner, depth);
          break;
        default:
          break;
      }
    }
  };

  walk(program.body, null, 0);

  const diagnostics = [...program.diagnostics, ...semanticDiagnostics(file, program.body)];
  return { program, funcs, classes, vars, outline, diagnostics };
}

/** 类沿继承链找方法（含父类） */
export function classMethods(a: Analysis, name: string): FuncSym[] {
  const out: FuncSym[] = [];
  const seen = new Set<string>();
  const visit = (n: string | null): void => {
    if (!n || seen.has(n)) return;
    seen.add(n);
    const c = a.classes.find((x) => x.name === n);
    if (!c) return;
    for (const m of c.methods) if (!out.some((o) => o.name === m.name)) out.push(m);
    visit(c.base);
  };
  visit(name);
  return out;
}

/** 类沿继承链找字段（含父类） */
export function classFields(a: Analysis, name: string): FieldSym[] {
  const out: FieldSym[] = [];
  const seen = new Set<string>();
  const visit = (n: string | null): void => {
    if (!n || seen.has(n)) return;
    seen.add(n);
    const c = a.classes.find((x) => x.name === n);
    if (!c) return;
    for (const f of c.fields) if (!out.some((o) => o.name === f.name)) out.push(f);
    visit(c.base);
  };
  visit(name);
  return out;
}

/** 按行号找最内层的成员上下文（用于 self. 补全） */
export function classAtLine(a: Analysis, line: number): ClassSym | null {
  let best: ClassSym | null = null;
  for (const c of a.classes) {
    const end = classEndLine(a, c);
    if (line >= c.line && line <= end) {
      if (!best || c.line > best.line) best = c;
    }
  }
  return best;
}

function classEndLine(a: Analysis, c: ClassSym): number {
  const idx = a.outline.findIndex((o) => o.icon === 'class' && o.line === c.line);
  if (idx < 0) return c.line;
  for (let k = idx + 1; k < a.outline.length; k++) {
    const o = a.outline[k];
    if ((o.icon === 'class' || o.icon === 'func' || o.icon === 'group') && o.depth <= 0) return o.line - 1;
  }
  return Number.MAX_SAFE_INTEGER;
}

/** 光标所在位置是否处于某个函数体内 */
export function funcAtLine(a: Analysis, line: number): FuncSym | null {
  let best: FuncSym | null = null;
  for (const f of [...a.funcs, ...a.classes.flatMap((c) => c.methods)]) {
    if (line >= f.line && line < fnEndLine(a, f)) {
      if (!best || f.line > best.line) best = f;
    }
  }
  return best;
}

function fnEndLine(a: Analysis, f: FuncSym): number {
  const idx = a.outline.findIndex((o) => o.line === f.line && (o.icon === 'func' || o.icon === 'method'));
  if (idx < 0) return f.line + 1;
  for (let k = idx + 1; k < a.outline.length; k++) {
    if (a.outline[k].depth <= a.outline[idx].depth) return a.outline[k].line;
  }
  return Number.MAX_SAFE_INTEGER;
}
