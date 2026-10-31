/* CW Script 语法层：缩进定块 + 行式语句 + Pratt 表达式 → AST。
 *
 * 与 0.1.0 的差别（也是这次重写的目的）：
 * - 所有指令参数都是表达式：`for i in range(n)` 的 n 可以是变量、`range(n-1)`、`range(a*2)`
 * - 循环变量可以是任意名字，且真正进入作用域（嵌套循环各用各的）
 * - 函数支持多参数、默认值、返回值，可在表达式里调用
 * - 新增 class / new / self / 成员访问 / 继承
 * - 解析错误不再中断整个文件：逐行恢复并产出诊断，交给编辑器打波浪线
 */

import {
  type Diagnostic, type Expr, type IfBranch, type Param, type Program, type Span,
  type Stmt, type Target, type TypeName, type AssignOp, type FieldDecl
} from './ast';
import { codePart, describeTok, LexError, tokenizeLine, type Token } from './token';

const TYPE_WORDS = new Set<string>(['int', 'float', 'str', 'bool', 'list', 'dict', 'var']);
const FUNC_WORDS = new Set(['function', 'Function', 'fn', 'func', 'f']);
const FUNC_MODIFIERS = new Set(['bool', 'repo', 'int', 'float', 'str', 'void', 'list', 'dict']);
const ASSIGN_OPS = new Set(['=', '+=', '-=', '*=', '/=', '%=', '**=']);

class ParseError extends Error {
  constructor(message: string, public tok: Token, public code = 'parse') {
    super(message);
    this.name = 'ParseError';
  }
}

/* ───────────────────────────── 表达式解析 ───────────────────────────── */

const PREC: Record<string, number> = {
  '||': 10, '&&': 15, '|': 20, '^': 25, '&': 30,
  '==': 35, '!=': 35, '===': 35, '!==': 35,
  '<': 40, '>': 40, '<=': 40, '>=': 40,
  '<<': 45, '>>': 45,
  '+': 50, '-': 50,
  '*': 60, '/': 60, '%': 60,
  '**': 70
};

const EOF: Token = { k: 'eof', v: '', line: 0, col: 0, endCol: 0 };

const startOf = (e: Expr): Token => ({ k: 'ident', v: '', line: e.line, col: e.col, endCol: e.col });
const endOf = (e: Expr): Token => ({ k: 'ident', v: '', line: e.endLine, col: e.endCol, endCol: e.endCol });

class ExprParser {
  pos = 0;
  constructor(public toks: Token[], public file: string) {}

  get cur(): Token { return this.toks[this.pos] ?? EOF; }
  peek(off = 0): Token { return this.toks[Math.min(this.pos + off, this.toks.length - 1)] ?? EOF; }
  at(v: string): boolean { return this.cur.v === v && this.cur.k !== 'eof'; }
  atEnd(): boolean { return this.cur.k === 'eof'; }
  eat(): Token { return this.toks[this.pos++]; }

  expect(v: string, what = v): Token {
    if (!this.at(v)) throw new ParseError(`这里应该是「${what}」，实际是${describeTok(this.cur)}`, this.cur, 'expect');
    return this.eat();
  }

  span(start: Token, end?: Token): Span {
    const e = end ?? start;
    return { file: this.file, line: start.line, col: start.col, endLine: e.line, endCol: e.endCol };
  }

  parseAll(): Expr {
    const e = this.parseExpr(0);
    if (!this.atEnd()) throw new ParseError(`表达式后面多了${describeTok(this.cur)}`, this.cur, 'trailing');
    return e;
  }

  /** 逗号分隔的表达式列表，用于 print / return 这种「参数可以裸写」的场合 */
  parseAllList(): Expr[] {
    if (this.atEnd()) return [];
    const out: Expr[] = [this.parseExpr(0)];
    while (this.at(',')) { this.eat(); out.push(this.parseExpr(0)); }
    return out;
  }

  parseExpr(minPrec: number): Expr {
    let left = this.parseUnary();
    for (;;) {
      const t = this.cur;
      if (t.k !== 'op' || !(t.v in PREC)) break;
      const prec = PREC[t.v];
      if (prec < minPrec) break;
      this.eat();
      const right = t.v === '**' ? this.parseExpr(prec) : this.parseExpr(prec + 1);
      left = { k: 'bin', op: t.v, a: left, b: right, ...this.span(startOf(left), endOf(right)) };
    }
    return left;
  }

  parseUnary(): Expr {
    const t = this.cur;
    if (t.k === 'op' && (t.v === '-' || t.v === '+' || t.v === '!' || t.v === '~')) {
      this.eat();
      const a = this.parseUnary();
      return { k: 'un', op: t.v, a, ...this.span(t, endOf(a)) };
    }
    return this.parsePostfix(this.parsePrimary());
  }

  parsePrimary(): Expr {
    const t = this.cur;

    if (t.k === 'num') {
      this.eat();
      return { k: 'num', v: t.num ?? 0, ...this.span(t) };
    }
    if (t.k === 'str') {
      this.eat();
      return this.stringNode(t);
    }
    if (t.k === 'kw' && t.v === 'new') {
      this.eat();
      const nameTok = this.cur;
      if (nameTok.k !== 'ident') throw new ParseError('new 后面要跟类名', nameTok, 'new');
      this.eat();
      const args = this.at('(') ? this.parseArgs() : [];
      const last = args.length ? endOf(args[args.length - 1]) : nameTok;
      return { k: 'new', cls: nameTok.v, args, ...this.span(nameTok, last) };
    }
    if (t.k === 'kw' && t.v === 'super') {
      this.eat();
      return { k: 'var', name: 'super', ...this.span(t) };
    }
    if (t.k === 'ident') {
      this.eat();
      if (t.v === 'true' || t.v === 'false') return { k: 'bool', v: t.v === 'true', ...this.span(t) };
      if (t.v === 'null' || t.v === 'nil' || t.v === 'none') return { k: 'null', ...this.span(t) };
      if (t.v === 'self' || t.v === 'this') return { k: 'self', ...this.span(t) };
      return { k: 'var', name: t.v, ...this.span(t) };
    }
    if (t.k === 'punc' && t.v === '(') {
      this.eat();
      const e = this.parseExpr(0);
      const close = this.expect(')', '右括号 )');
      return { ...e, ...this.span(startOf(e), close) };
    }
    if (t.k === 'punc' && t.v === '[') {
      this.eat();
      const items: Expr[] = [];
      if (!this.at(']')) {
        for (;;) {
          items.push(this.parseExpr(0));
          if (this.at(',')) { this.eat(); if (this.at(']')) break; continue; }
          break;
        }
      }
      const close = this.expect(']', '列表结束 ]');
      return { k: 'list', items, ...this.span(t, close) };
    }
    if (t.k === 'punc' && t.v === '{') {
      this.eat();
      const entries: { key: Expr; val: Expr }[] = [];
      if (!this.at('}')) {
        for (;;) {
          const kt = this.cur;
          let key: Expr;
          if (kt.k === 'ident' && this.peek(1).v === ':') {
            this.eat();
            key = { k: 'str', v: kt.v, ...this.span(kt) };
          } else {
            key = this.parseExpr(0);
          }
          this.expect(':', '字典的冒号 :');
          entries.push({ key, val: this.parseExpr(0) });
          if (this.at(',')) { this.eat(); if (this.at('}')) break; continue; }
          break;
        }
      }
      const close = this.expect('}', '字典结束 }');
      return { k: 'dict', entries, ...this.span(t, close) };
    }
    // 报错要"说人话"：不光说该有什么，还说要怎么改
    if (t.k === 'eof') {
      throw new ParseError('这行还没写完：少了一个值，可能是括号没关、或者漏了右半边', t, 'value');
    }
    throw new ParseError(
      `这里应该填一个值（数字、字符串、变量都行），现在是${describeTok(t)}`, t, 'value'
    );
  }

  /** 字符串字面量 → 支持 {变量/表达式} 插值，编译成字符串拼接 */
  stringNode(t: Token): Expr {
    const text = t.v;
    if (!text.includes('{') && !text.includes('\\{')) {
      return { k: 'str', v: text.replace(/\\}/g, '}'), ...this.span(t) };
    }

    const parts: Expr[] = [];
    let buf = '';
    let i = 0;
    const pushStr = (): void => {
      if (buf) { parts.push({ k: 'str', v: buf, ...this.span(t) }); buf = ''; }
    };

    while (i < text.length) {
      const c = text[i];
      if (c === '\\' && (text[i + 1] === '{' || text[i + 1] === '}')) { buf += text[i + 1]; i += 2; continue; }
      if (c !== '{') { buf += c; i++; continue; }

      const close = text.indexOf('}', i + 1);
      if (close < 0) { buf += c; i++; continue; }
      const inner = text.slice(i + 1, close).trim();
      if (!inner) { buf += '{}'; i = close + 1; continue; }

      pushStr();
      const toks = tokenizeLine(inner, t.line, t.col + i + 1);
      const sub = new ExprParser(toks, this.file);
      try {
        parts.push(sub.parseAll());
      } catch {
        // 插值里写错了：当成普通文本，交给外面的语法检查
        buf += text.slice(i, close + 1);
      }
      i = close + 1;
    }
    pushStr();

    if (parts.length === 1) return parts[0];
    return parts.reduce((acc, cur) => ({ k: 'bin', op: '+', a: acc, b: cur, ...this.span(t) }) as Expr);
  }

  parseArgs(): Expr[] {
    this.expect('(', '左括号 (');
    const args: Expr[] = [];
    if (!this.at(')')) {
      for (;;) {
        args.push(this.parseExpr(0));
        if (this.at(',')) { this.eat(); if (this.at(')')) break; continue; }
        break;
      }
    }
    this.expect(')', '右括号 )');
    return args;
  }

  parsePostfix(base: Expr): Expr {
    let e = base;
    for (;;) {
      if (this.at('.')) {
        this.eat();
        const nameTok = this.cur;
        if (nameTok.k !== 'ident') throw new ParseError('「.」后面要跟成员名', nameTok, 'member');
        this.eat();
        e = { k: 'member', obj: e, name: nameTok.v, ...this.span(startOf(e), nameTok) };
        continue;
      }
      if (this.at('[')) {
        this.eat();
        const index = this.parseExpr(0);
        const close = this.expect(']', '下标结束 ]');
        e = { k: 'idx', obj: e, index, ...this.span(startOf(e), close) };
        continue;
      }
      if (this.at('(')) {
        const args = this.parseArgs();
        const last = args.length ? endOf(args[args.length - 1]) : this.toks[this.pos - 1];
        e = { k: 'call', callee: e, args, ...this.span(startOf(e), last) };
        continue;
      }
      return e;
    }
  }
}

/** 单独解析一段表达式文本（供 IDE / 指令参数复用） */
export function parseExpression(file: string, text: string, line = 1, col = 1): Expr {
  const toks = tokenizeLine(text, line, col);
  return new ExprParser(toks, file).parseAll();
}

/* ───────────────────────────── 文件级解析 ───────────────────────────── */

type Line = {
  no: number;
  indent: number;
  /** 去掉注释、行尾块花括号之后的代码 */
  code: string;
  raw: string;
  comment: string | null;
  isComment: boolean;
  skipped: boolean;
};

function splitSource(src: string): Line[] {
  const out: Line[] = [];
  const text = src.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  text.split('\n').forEach((rawLine, idx) => {
    const expanded = rawLine.replace(/\t/g, '    ');
    const indent = expanded.length - expanded.trimStart().length;
    const trimmed = expanded.trim();
    if (trimmed === '') return;
    if (trimmed.startsWith('--')) {
      out.push({ no: idx + 1, indent, code: '', raw: trimmed, comment: null, isComment: false, skipped: true });
      return;
    }
    const { code, comment } = codePart(trimmed);
    const isComment = trimmed.startsWith('//');
    // 行尾的 { 是"块开始"标记，不是字典字面量（字典字面量必然以 } 结尾）
    const cleaned = isComment ? '' : code.trim().replace(/\{\s*$/, '').trim();
    out.push({ no: idx + 1, indent, code: cleaned, raw: trimmed, comment, isComment, skipped: false });
  });
  return out;
}

export function parseProgram(file: string, src: string): Program {
  const lines = splitSource(src);
  const diagnostics: Diagnostic[] = [];
  let i = 0;

  const diag = (severity: Diagnostic['severity'], message: string, span: Span, code = 'parse'): void => {
    diagnostics.push({ severity, message, code, span });
  };

  const spanAt = (line: number, col: number, endCol = col + 1): Span => ({ file, line, col, endLine: line, endCol });

  const stmtSpan = (ln: Line, col = ln.indent + 1, endCol = ln.indent + Math.max(1, ln.code.length)): Span => ({
    file, line: ln.no, col, endLine: ln.no, endCol
  });

  const tokensOf = (ln: Line, code = ln.code, col = ln.indent + 1): Token[] => {
    const toks = tokenizeLine(code, ln.no, col);
    return toks.slice(0, -1); // 去掉 eof，按需补
  };

  const parserFor = (toks: Token[]): ExprParser => {
    const last = toks[toks.length - 1];
    const tail: Token = last ? { k: 'eof', v: '', line: last.line, col: last.endCol, endCol: last.endCol } : { ...EOF, line: 1, col: 1 };
    return new ExprParser([...toks, tail], file);
  };

  const exprOf = (toks: Token[]): Expr => parserFor(toks).parseAll();

  const stripParens = (s: string): string => {
    const t = s.trim().replace(/\{\s*$/, '').trim();
    if (t.startsWith('(') && t.endsWith(')')) return t.slice(1, -1).trim();
    return t;
  };

  const toTarget = (e: Expr): Target => {
    if (e.k === 'var') return { k: 'var', name: e.name, file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
    if (e.k === 'member') return { k: 'member', obj: e.obj, name: e.name, file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
    if (e.k === 'idx') return { k: 'idx', obj: e.obj, index: e.index, file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
    diag('error', '这里只能给变量、成员或下标赋值', {
      file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol
    }, 'assign-target');
    return { k: 'var', name: '_', file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
  };

  const parseBlock = (minIndent: number): Stmt[] => {
    const body: Stmt[] = [];
    while (i < lines.length) {
      const ln = lines[i];
      if (ln.skipped) { i++; continue; }
      if (ln.indent < minIndent) break;
      const stmt = parseStatement();
      if (stmt) body.push(stmt);
    }
    return body;
  };

  const parseBodyOf = (indent: number): Stmt[] => parseBlock(indent + 1);

  const unknown = (ln: Line, hint: string | null): Stmt => ({ k: 'unknown', raw: ln.code || ln.raw, hint, ...stmtSpan(ln) });

  const recover = (ln: Line, e: unknown): Stmt => {
    const msg = e instanceof ParseError ? e.message : e instanceof LexError ? e.message : String(e);
    const where = e instanceof ParseError
      ? spanAt(e.tok.line, e.tok.col, e.tok.endCol)
      : e instanceof LexError
        ? spanAt(e.line, e.col)
        : stmtSpan(ln);
    diag('error', msg, where, e instanceof ParseError ? e.code : 'lex');
    while (i < lines.length && lines[i].indent > ln.indent) i++;
    return unknown(ln, suggestFor(ln.code));
  };

  /* ── 分支合并：} else { / } elif (…) { 与独立行 else / elif ── */
  const mergeBranches = (branches: IfBranch[], indent: number, elseBox: { body: Stmt[] | null }): void => {
    for (;;) {
      const ln = lines[i];
      if (!ln || ln.skipped) return;
      let code = ln.code;
      let braceClose = false;
      if (code.startsWith('}')) {
        braceClose = true;
        code = code.replace(/^\}\s*/, '').trim();
      }
      if (!code) return;
      const isElif = /^elif\b/.test(code);
      const isElse = /^else\b/.test(code);
      if (!isElif && !isElse) return;
      const sameIndent = ln.indent === indent || (braceClose && ln.indent <= indent + 1);
      if (!sameIndent) return;

      i++;
      if (isElse) {
        elseBox.body = parseBodyOf(ln.indent);
        return;
      }
      const condSrc = stripParens(code.replace(/^elif\b/, ''));
      let cond: Expr | null = null;
      if (condSrc) {
        try {
          const col = ln.indent + 1 + code.indexOf('elif') + 4;
          cond = exprOf(tokensOf(ln, condSrc, col));
        } catch (e) {
          recover(ln, e);
          return;
        }
      }
      branches.push({ cond, body: parseBodyOf(ln.indent), line: ln.no });
    }
  };

  const parseStatement = (): Stmt | null => {
    const ln = lines[i];
    if (ln.skipped) { i++; return null; }
    if (ln.isComment) {
      i++;
      const text = (ln.comment ?? '').trim();
      const group = /^[─-].*[─-]$/.test(text);
      return { k: 'comment', text, group, ...stmtSpan(ln) };
    }
    try {
      return parseLine(ln);
    } catch (e) {
      i++;
      return recover(ln, e);
    }
  };

  const parseLine = (ln: Line): Stmt | null => {
    let code = ln.code;
    if (code.startsWith('}')) code = code.replace(/^\}\s*/, '').trim();
    if (!code) { i++; return null; }

    const toks = tokensOf(ln, code);
    if (!toks.length) { i++; return null; }
    const first = toks[0];
    const word = first.v;
    // function / return / import / class 在词法层是关键字，这里与普通标识符一视同仁
    const isIdent = first.k === 'ident' || first.k === 'kw';

    if (isIdent) {
      switch (word) {
        case 'class': return parseClass(ln, toks);
        case 'if': return parseIf(ln, toks);
        case 'elif': throw new ParseError('elif 前面没有对应的 if', first, 'stray-elif');
        case 'else': throw new ParseError('else 前面没有对应的 if', first, 'stray-else');
        case 'while': return parseWhile(ln, toks);
        case 'repeat': return parseRepeat(ln, toks);
        case 'for': return parseFor(ln, toks);
        case 'forever': return parseForever(ln);
        case 'main':
          if (toks[1]?.v === 'loop') return parseForever(ln);
          break;
        case 'wait': return parseWait(ln, toks);
        case 'break':
        case 'out':
          // 只有单独成句才是跳转；out += 1 这类要按变量处理
          if (toks.length === 1) { i++; return { k: 'break', ...stmtSpan(ln) }; }
          break;
        case 'balo':
        case 'continue':
          if (toks.length === 1) { i++; return { k: 'continue', ...stmtSpan(ln) }; }
          break;
        case 'stop':
          if (toks.length === 1 || toks[1]?.v === 'all' || toks[1]?.v === 'now' || toks[1]?.v === 'script') {
            i++; return { k: 'stop', ...stmtSpan(ln) };
          }
          break;
        case 'return': {
          i++;
          const rest = toks.slice(1);
          if (!rest.length) return { k: 'return', value: null, ...stmtSpan(ln) };
          // return a, b → 返回列表 [a, b]
          const rp = parserFor(rest);
          const first = rp.parseExpr(0);
          if (rp.at(',')) {
            const items: Expr[] = [first];
            while (rp.at(',')) { rp.eat(); items.push(rp.parseExpr(0)); }
            if (!rp.atEnd()) throw new ParseError('return 后面多了内容', rp.cur, 'return');
            return { k: 'return', value: { k: 'list', items, file, line: ln.no, col: 1, endLine: ln.no, endCol: 1 }, ...stmtSpan(ln) };
          }
          if (!rp.atEnd()) throw new ParseError('return 后面多了内容', rp.cur, 'return');
          return { k: 'return', value: first, ...stmtSpan(ln) };
        }
        case 'throw': {
          i++;
          const rest = toks.slice(1);
          if (!rest.length) throw new ParseError('throw 后面要跟要抛出的值', toks[0], 'throw');
          return { k: 'throw', value: exprOf(rest), ...stmtSpan(ln) };
        }
        case 'try': return parseTry(ln, toks);
        case 'catch': throw new ParseError('catch 要跟在 try 块后面', toks[0], 'stray-catch');
        case 'finally': throw new ParseError('finally 要跟在 try 块后面', toks[0], 'stray-finally');
        case 'ask': return parseAsk(ln, toks);
        case 'print':
          return parsePrint(ln, toks);   // print(a) 与 print a 都走这里，括号可选
        case '报告':
        case 'report':
          return parseReport(ln, toks);  // 写法跟 print 一样，输出到右侧「报告」
        case 'import': return parseImport(ln, toks);
        case 'time':
          if (toks[1]?.v === 'read') { i++; return { k: 'timeRead', ...stmtSpan(ln) }; }
          if (toks[1]?.v === 'reset') { i++; return { k: 'timeReset', ...stmtSpan(ln) }; }
          break;
        case 'list':
          if (toks.length === 2 && toks[1].k === 'ident') {
            i++;
            return { k: 'decl', type: 'list', name: toks[1].v, init: null, ...stmtSpan(ln) };
          }
          break;
        case 'dict':
          if (toks[1]?.k === 'ident') return parseDict(ln, toks);
          break;
        case 'add':
          if (toks[1]?.v !== '(') return parseListAdd(ln, toks);
          break;
        case 'insert':
          if (toks[1]?.v !== '(') return parseListInsert(ln, toks);
          break;
        case 'replace':
          if (toks[1]?.v !== '(' && toks[1]?.v !== '=') return parseListReplace(ln, toks);
          break;
        case 'read':
          if (toks[1]?.v !== '(' && toks[1]?.v !== '=') return parseListRead(ln, toks);
          break;
        case 'del':
          if (toks[1]?.v !== '(' && toks[1]?.v !== '=') return parseListDelete(ln, toks);
          break;
        case 'len':
          if (toks[1]?.v !== '(' && toks[1]?.v !== '=') return parseLen(ln, toks);
          break;
      }

      if (FUNC_WORDS.has(word) && (word !== 'f' || toks[1]?.k === 'ident')) return parseFuncDef(ln, toks);
    }

    if (first.k === 'kw' && first.v === 'class') return parseClass(ln, toks);

    /* 解构赋值：a, b = f() / int a, int b = f() */
    if (isIdent) {
      const names: { name: string; line: number; col: number }[] = [];
      let k = 0;
      let ok = true;
      for (;;) {
        // 允许写类型：int low, int high = f()
        if (toks[k]?.k === 'ident' && TYPE_WORDS.has(toks[k].v) && toks[k + 1]?.k === 'ident') k++;
        const t = toks[k];
        if (!t || t.k !== 'ident') { ok = false; break; }
        names.push({ name: t.v, line: t.line, col: t.col });
        k++;
        if (toks[k]?.v === ',') { k++; continue; }
        break;
      }
      if (ok && names.length >= 2 && toks[k]?.v === '=') {
        const dp = parserFor(toks.slice(k + 1));
        const value = dp.parseExpr(0);
        if (!dp.atEnd()) throw new ParseError('解构赋值右边多了内容', dp.cur, 'destructure');
        i++;
        return { k: 'destructure', names, value, ...stmtSpan(ln) };
      }
    }

    /* 变量声明：int a = 1 */
    if (isIdent && TYPE_WORDS.has(word) && toks[1]?.k === 'ident') {
      const p = parserFor(toks.slice(2));
      let init: Expr | null = null;
      if (p.at('=')) { p.eat(); init = p.parseExpr(0); }
      else if (!p.atEnd()) throw new ParseError(`变量声明「${word} ${toks[1].v}」后面多出了内容`, p.cur, 'decl');
      i++;
      return { k: 'decl', type: word as TypeName, name: toks[1].v, init, ...stmtSpan(ln) };
    }

    /* 类类型声明：Person p = new Person(…) —— 类型名只作标注，按 var 处理 */
    if (isIdent && toks.length >= 4 && toks[1]?.k === 'ident' && toks[2]?.v === '=') {
      const p = parserFor(toks.slice(3));
      const init = p.parseExpr(0);
      if (!p.atEnd()) throw new ParseError('声明语句右边多了内容', p.cur, 'decl');
      i++;
      return { k: 'decl', type: 'var', name: toks[1].v, init, ...stmtSpan(ln) };
    }

    /* 赋值 / 自增自减 / 表达式语句 */
    const p = parserFor(toks);
    const firstExpr = p.parseExpr(0);

    if (!p.atEnd() && p.cur.k === 'op' && ASSIGN_OPS.has(p.cur.v)) {
      const op = p.eat().v as AssignOp;
      const value = p.parseExpr(0);
      if (!p.atEnd()) throw new ParseError('赋值号右边多了内容', p.cur, 'assign');
      i++;
      return { k: 'assign', target: toTarget(firstExpr), op, value, ...stmtSpan(ln) };
    }
    if (!p.atEnd() && (p.cur.v === '++' || p.cur.v === '--')) {
      const delta = (p.cur.v === '++' ? 1 : -1) as 1 | -1;
      p.eat();
      if (!p.atEnd()) throw new ParseError('自增自减后面不能再有内容', p.cur, 'incdec');
      i++;
      return { k: 'incdec', target: toTarget(firstExpr), delta, prefix: false, ...stmtSpan(ln) };
    }
    if (!p.atEnd()) throw new ParseError(`这条语句看不懂：${describeTok(p.cur)}`, p.cur, 'trailing');

    i++;
    return { k: 'expr', expr: firstExpr, ...stmtSpan(ln) };
  };

  /* ── 各语句 ── */

  const parseIf = (ln: Line, toks: Token[]): Stmt => {
    i++;
    let rest = toks.slice(1);
    let isIfOut = false;
    if (rest[0]?.v === 'out') { isIfOut = true; rest = rest.slice(1); }

    let cond: Expr | null = null;
    if (rest.length) cond = parserFor(rest).parseExpr(0);
    if (isIfOut) {
      if (!cond) throw new ParseError('if out 需要一个条件', toks[0], 'if-out');
      return { k: 'ifOut', cond, ...stmtSpan(ln) };
    }
    // 光秃秃的 if 不能默默放行：那样整段分支会变成永远不执行的死代码
    if (!cond) throw new ParseError('if 后面要跟条件，例如 if (a > 1)', toks[0], 'if-no-cond');
    const branches: IfBranch[] = [{ cond, body: parseBodyOf(ln.indent), line: ln.no }];
    const elseBox: { body: Stmt[] | null } = { body: null };
    mergeBranches(branches, ln.indent, elseBox);
    return { k: 'if', branches, elseBody: elseBox.body, ...stmtSpan(ln) };
  };

  const parseWhile = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const cond = parserFor(toks.slice(1)).parseExpr(0);
    return { k: 'while', cond, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };

  const parseRepeat = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const count = parserFor(toks.slice(1)).parseExpr(0);
    return { k: 'repeat', count, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };

  const parseForever = (ln: Line): Stmt => {
    i++;
    return { k: 'forever', body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };

  const parseWait = (ln: Line, toks: Token[]): Stmt => {
    i++;
    if (toks[1]?.v === 'until') {
      const cond = parserFor(toks.slice(2)).parseExpr(0);
      return { k: 'waitUntil', cond, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
    }
    const seconds = parserFor(toks.slice(1)).parseExpr(0);
    return { k: 'wait', seconds, ...stmtSpan(ln) };
  };

  const parseTry = (ln: Line, _toks: Token[]): Stmt => {
    i++;
    const body = parseBodyOf(ln.indent);
    let catchName: string | null = null;
    let catchBody: Stmt[] | null = null;
    let finallyBody: Stmt[] | null = null;

    for (;;) {
      const L = lines[i];
      if (!L || L.skipped) break;
      let code = L.code;
      let brace = false;
      if (code.startsWith('}')) { brace = true; code = code.replace(/^\}\s*/, '').trim(); }
      if (!code) break;
      const sameIndent = L.indent === ln.indent || (brace && L.indent <= ln.indent + 1);
      if (!sameIndent) break;

      if (/^catch\b/.test(code)) {
        const rest = code.replace(/^catch\b/, '').trim();
        catchName = rest ? rest.split(/[\s(]/)[0].replace(/\)$/, '') : null;
        i++;
        catchBody = parseBodyOf(L.indent);
        continue;
      }
      if (/^finally\b/.test(code)) {
        i++;
        finallyBody = parseBodyOf(L.indent);
        continue;
      }
      break;
    }

    if (!catchBody && !finallyBody) {
      diag('warning', 'try 后面没有 catch 也没有 finally，块里的错误会直接抛出去', stmtSpan(ln), 'try-bare');
    }
    return { k: 'try', body, catchName, catchBody, finallyBody, ...stmtSpan(ln) };
  };

  const parseFor = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const varTok = toks[1];
    if (!varTok || varTok.k !== 'ident') throw new ParseError('for 后面要跟循环变量名', toks[0], 'for');

    let valueName: string | null = null;
    let cursor = 2;
    if (toks[2]?.v === ',') {
      const v2 = toks[3];
      if (!v2 || v2.k !== 'ident') throw new ParseError('for a, b 里的第二项要是变量名', toks[2], 'for');
      valueName = v2.v;
      cursor = 4;
    }

    if (toks[cursor]?.v === '=') {
      const p = parserFor(toks.slice(cursor + 1));
      const from = p.parseExpr(0);
      if (!p.at('to')) throw new ParseError('计数循环要写成「for i = 1 to 10」', p.cur, 'for');
      p.eat();
      const to = p.parseExpr(0);
      let step: Expr | null = null;
      if (!p.atEnd()) {
        if (!p.at('step')) throw new ParseError('这里只能写 step 步长', p.cur, 'for');
        p.eat();
        step = p.parseExpr(0);
      }
      return {
        k: 'forRange', varName: varTok.v, from, to, step, inclusive: true,
        body: parseBodyOf(ln.indent), ...stmtSpan(ln)
      };
    }
    if (toks[cursor]?.v !== 'in') {
      throw new ParseError('for 的写法：「for i in 集合」/「for i in range(n)」/「for i = 1 to 10」', toks[cursor] ?? toks[0], 'for');
    }
    const iter = parserFor(toks.slice(cursor + 1)).parseExpr(0);

    if (iter.k === 'call' && iter.callee.k === 'var' && iter.callee.name === 'range') {
      const a = iter.args;
      if (!a.length) throw new ParseError('range 需要至少一个参数', toks[0], 'range');
      if (a.length > 3) throw new ParseError('range 最多接受三个参数（起点, 终点, 步长）', toks[0], 'range');
      const from = a.length >= 2 ? a[0] : null;
      const to = a.length >= 2 ? a[1] : a[0];
      const step = a.length >= 3 ? a[2] : null;
      return {
        k: 'forRange', varName: varTok.v, from, to, step, inclusive: false,
        body: parseBodyOf(ln.indent), ...stmtSpan(ln)
      };
    }
    return { k: 'forIn', varName: varTok.v, valueName, iter, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };

  const parseWaitUntilUnused = 0;
  void parseWaitUntilUnused;

  const parseDict = (ln: Line, toks: Token[]): Stmt => {
    const nameTok = toks[1];
    const p = parserFor(toks.slice(2));
    const target: Target = { k: 'var', name: nameTok.v, file, line: nameTok.line, col: nameTok.col, endLine: nameTok.line, endCol: nameTok.endCol };

    if (p.at('=')) {
      p.eat();
      const init = p.parseExpr(0);
      i++;
      return { k: 'decl', type: 'dict', name: nameTok.v, init, ...stmtSpan(ln) };
    }
    if (p.atEnd()) {
      i++;
      return { k: 'decl', type: 'dict', name: nameTok.v, init: null, ...stmtSpan(ln) };
    }
    if (p.at('key')) {
      p.eat();
      const key = p.parseExpr(0);
      if (!p.at('=')) throw new ParseError('dict 写入要写成「dict d key 键 = 值」', p.cur, 'dict');
      p.eat();
      const value = p.parseExpr(0);
      i++;
      return { k: 'dictSet', target, key, value, ...stmtSpan(ln) };
    }
    if (p.at('read') || p.at('del')) {
      const isRead = p.eat().v === 'read';
      const key = p.parseExpr(0);
      i++;
      return isRead
        ? { k: 'dictRead', target, key, ...stmtSpan(ln) }
        : { k: 'dictDelete', target, key, ...stmtSpan(ln) };
    }
    throw new ParseError('dict 的写法：「dict d key 键 = 值」/「dict d read 键」/「dict d del 键」/「dict d」', p.cur, 'dict');
  };

  const parseAsk = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const rest = toks.slice(1);
    const question = rest.length
      ? parserFor(rest[0].v === '(' ? rest.slice(1, rest[rest.length - 1]?.v === ')' ? -1 : undefined) : rest).parseAll()
      : ({ k: 'str', v: '', ...stmtSpan(ln) } as Expr);
    return { k: 'ask', question, ...stmtSpan(ln) };
  };

  /** 报告：写法跟 print 一样，只是输出到右侧「报告」面板 */
  const parseReport = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    const args = p.at('(') ? p.parseArgs() : p.parseAllList();
    if (!p.atEnd()) throw new ParseError('报告 后面多了内容', p.cur, 'report');
    return { k: 'report', args, ...stmtSpan(ln) };
  };

  const parsePrint = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    // print(a, b) 与 print a, b 都算数：括号是可选的，别逼用户记语法
    const args = p.at('(') ? p.parseArgs() : p.parseAllList();
    if (!p.atEnd()) throw new ParseError('print 后面多了内容', p.cur, 'print');
    return { k: 'print', args, ...stmtSpan(ln) };
  };

  const parseImport = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const t = toks[1];
    if (!t || t.k !== 'str') throw new ParseError('import 后面要跟字符串路径，例如 import "lib.cw"', toks[0], 'import');
    return { k: 'import', target: t.v, ...stmtSpan(ln) };
  };

  const parseListAdd = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    const value = p.parseExpr(0);
    if (!p.at('to')) throw new ParseError('追加要写成「add 值 to 列表」', p.cur, 'list-add');
    p.eat();
    const target = toTarget(p.parseExpr(0));
    return { k: 'listAdd', value, target, ...stmtSpan(ln) };
  };

  const parseListInsert = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    const value = p.parseExpr(0);
    if (!p.at('to')) throw new ParseError('插入要写成「insert 值 to 列表 at 下标」', p.cur, 'list-insert');
    p.eat();
    const target = toTarget(p.parseExpr(0));
    if (!p.at('at')) throw new ParseError('插入要写成「insert 值 to 列表 at 下标」', p.cur, 'list-insert');
    p.eat();
    const index = p.parseExpr(0);
    return { k: 'listInsert', value, target, index, ...stmtSpan(ln) };
  };

  const parseListReplace = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    const target = toTarget(p.parseExpr(0));
    const index = p.parseExpr(0);
    if (!p.at('with')) throw new ParseError('替换要写成「replace 列表 下标 with 值」', p.cur, 'list-replace');
    p.eat();
    const value = p.parseExpr(0);
    return { k: 'listReplace', target, index, value, ...stmtSpan(ln) };
  };

  const parseListRead = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    const target = toTarget(p.parseExpr(0));
    const index = p.parseExpr(0);
    return { k: 'listRead', target, index, ...stmtSpan(ln) };
  };

  const parseListDelete = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    const target = toTarget(p.parseExpr(0));
    const index = p.atEnd() ? null : p.parseExpr(0);
    return { k: 'listDelete', target, index, ...stmtSpan(ln) };
  };

  const parseLen = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const p = parserFor(toks.slice(1));
    return { k: 'lenOf', value: p.parseExpr(0), ...stmtSpan(ln) };
  };

  const parseFuncDef = (ln: Line, toks: Token[]): Stmt => {
    i++;
    let cursor = 1;
    let retType: string | null = null;

    if (toks[cursor]?.k === 'ident' && FUNC_MODIFIERS.has(toks[cursor].v) && toks[cursor + 1]?.k === 'ident') {
      retType = toks[cursor].v;
      cursor++;
    }
    const nameTok = toks[cursor];
    if (!nameTok || nameTok.k !== 'ident') throw new ParseError('函数定义要写函数名，例如 function hello(a, b)', toks[0], 'func-name');
    cursor++;

    let params: Param[] = [];
    if (toks[cursor]?.v === '(') {
      const close = findClose(toks, cursor);
      params = parseParams(toks.slice(cursor + 1, close));
    }

    return { k: 'funcDef', name: nameTok.v, params, retType, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };

  const findClose = (toks: Token[], openIdx: number): number => {
    let depth = 0;
    for (let k = openIdx; k < toks.length; k++) {
      if (toks[k].v === '(') depth++;
      else if (toks[k].v === ')') {
        depth--;
        if (depth === 0) return k;
      }
    }
    throw new ParseError('缺少右括号 )', toks[openIdx], 'paren');
  };

  const parseParams = (toks: Token[]): Param[] => {
    const out: Param[] = [];
    let idx = 0;
    while (idx < toks.length) {
      let type: TypeName | null = null;
      if (toks[idx]?.k === 'ident' && TYPE_WORDS.has(toks[idx].v) && toks[idx + 1]?.k === 'ident') {
        type = toks[idx].v as TypeName;
        idx++;
      }
      const nameTok = toks[idx];
      if (!nameTok || nameTok.k !== 'ident') throw new ParseError('函数参数要写参数名', nameTok ?? toks[0], 'param');
      idx++;
      let def: Expr | null = null;
      if (toks[idx]?.v === '=') {
        idx++;
        const sub: Token[] = [];
        let depth = 0;
        while (idx < toks.length) {
          const t = toks[idx];
          if (t.v === '(' || t.v === '[' || t.v === '{') depth++;
          if (t.v === ')' || t.v === ']' || t.v === '}') depth--;
          if (depth === 0 && t.v === ',') break;
          sub.push(t);
          idx++;
        }
        if (!sub.length) throw new ParseError(`参数 ${nameTok.v} 的默认值为空`, nameTok, 'param-default');
        def = exprOf(sub);
      }
      out.push({ name: nameTok.v, type, def, line: nameTok.line, col: nameTok.col });
      if (toks[idx]?.v === ',') { idx++; continue; }
      break;
    }
    return out;
  };

  const parseClass = (ln: Line, toks: Token[]): Stmt => {
    i++;
    const nameTok = toks[1];
    if (!nameTok || nameTok.k !== 'ident') throw new ParseError('class 后面要跟类名', toks[0], 'class');
    let base: string | null = null;
    if (toks[2]?.v === 'extends') {
      const b = toks[3];
      if (!b || b.k !== 'ident') throw new ParseError('extends 后面要跟父类名', toks[2], 'class');
      base = b.v;
    }

    const fields: FieldDecl[] = [];
    const methods: Stmt[] = [];

    while (i < lines.length && lines[i].indent > ln.indent) {
      const L = lines[i];
      if (L.skipped || L.isComment) { i++; continue; }
      const lt = tokensOf(L);
      if (!lt.length) { i++; continue; }
      const w = lt[0].v;
      if (lt[0].k === 'kw' && lt[0].v === 'function' || FUNC_WORDS.has(w)) {
        try {
          methods.push(parseFuncDef(L, lt));
        } catch (e) {
          i++;
          recover(L, e);
        }
        continue;
      }
      try {
        let cursor = 0;
        let type: TypeName | null = null;
        if (TYPE_WORDS.has(lt[0].v) && lt[1]?.k === 'ident') {
          type = lt[0].v as TypeName;
          cursor = 1;
        }
        const fname = lt[cursor];
        if (!fname || fname.k !== 'ident') throw new ParseError('类体里只能写字段或方法', lt[0], 'class-body');
        cursor++;
        let init: Expr | null = null;
        if (lt[cursor]?.v === '=') init = exprOf(lt.slice(cursor + 1));
        else if (lt.length > cursor) throw new ParseError('字段初始化要写成「name = 值」', lt[cursor], 'class-field');
        fields.push({ name: fname.v, type, init, line: fname.line, col: fname.col });
        i++;
      } catch (e) {
        i++;
        recover(L, e);
      }
    }

    if (!fields.length && !methods.length) {
      diag('warning', `类 ${nameTok.v} 是空的（既没有字段也没有方法）`, stmtSpan(ln), 'empty-class');
    }
    return { k: 'classDef', name: nameTok.v, base, fields, methods, ...stmtSpan(ln) };
  };

  const body = parseBlock(0);

  /* 顶层语义体检：语法上合法、但写在这儿一定不是本意的情况。
   * 顶层 return 没有任何作用，与其静默吞掉，不如直接指出来。 */
  for (const s of body) {
    if (s.k === 'return') {
      diag('error', 'return 只能写在函数或方法里', { file, line: s.line, col: s.col, endLine: s.endLine, endCol: s.endCol }, 'stray-return');
    } else if (s.k === 'break' || s.k === 'continue') {
      diag('error', `${s.k === 'break' ? 'out / break' : 'balo / continue'} 只能写在循环里`, { file, line: s.line, col: s.col, endLine: s.endLine, endCol: s.endCol }, 'stray-jump');
    }
  }

  return { file, body, diagnostics };
}

/* ───────────────────────────── 指令近似建议 ───────────────────────────── */

const SUGGEST: Record<string, string> = {
  printer: 'print(…)', pri: 'print(…)', prnt: 'print(…)', pr: 'print(…)',
  ifo: 'if out(条件)', whl: 'while (条件)', wile: 'while (条件)',
  fnc: 'function name()', fnt: 'function name()', fun: 'function name()', funcion: 'function name()',
  intger: 'int x = 数字', strn: 'str x = "文本"', lst: 'list x', dct: 'dict x',
  forevr: 'forever', rep: 'repeat(n)', wate: 'wait 0.5', imprt: 'import "文件"',
  clas: 'class Name', classe: 'class Name', ne: 'new ClassName(…)', slf: 'self'
};

export function suggestFor(raw: string): string | null {
  const w = raw.trim().replace(/^[（(]+/, '').split(/[\s(（]/)[0];
  return SUGGEST[w] ?? null;
}
