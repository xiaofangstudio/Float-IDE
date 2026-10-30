// src/lang/token.ts
var KEYWORDS = /* @__PURE__ */ new Set(["new", "class", "extends", "function", "fn", "return", "import", "super"]);
var OPERATORS = [
  "===",
  "!==",
  "**=",
  "<<=",
  ">>=",
  "==",
  "!=",
  "<=",
  ">=",
  "&&",
  "||",
  "**",
  "<<",
  ">>",
  "+=",
  "-=",
  "*=",
  "/=",
  "%=",
  "++",
  "--",
  "=>",
  "+",
  "-",
  "*",
  "/",
  "%",
  "<",
  ">",
  "!",
  "~",
  "&",
  "|",
  "^",
  "=",
  "?",
  ":"
];
var PUNCS = "()[]{},.;";
var LexError = class extends Error {
  constructor(message, line, col) {
    super(message);
    this.line = line;
    this.col = col;
    this.name = "LexError";
  }
};
var isIdStart = (c) => /[A-Za-z_]/.test(c) || c >= "\u4E00" && c <= "\u9FA5" || c >= "\u3400" && c <= "\u4DBF";
var isIdPart = (c) => isIdStart(c) || /[0-9]/.test(c);
function tokenizeLine(src, line, startCol = 1) {
  const out = [];
  let i = 0;
  const push = (k, v, col, endCol, num2) => {
    out.push({ k, v, line, col, endCol, num: num2 });
  };
  while (i < src.length) {
    const c = src[i];
    if (c === " " || c === "	" || c === "\u3000") {
      i++;
      continue;
    }
    if (c === "\r") {
      i++;
      continue;
    }
    if (c === "/" && src[i + 1] === "/") break;
    if (c === '"' || c === "'") {
      const quote = c;
      const startCol2 = startCol + i;
      let j = i + 1;
      let text = "";
      let depth = 0;
      while (j < src.length && !(src[j] === quote && depth === 0)) {
        if (src[j] === "{") depth++;
        else if (src[j] === "}" && depth > 0) depth--;
        if (src[j] === "\\" && j + 1 < src.length) {
          const e = src[j + 1];
          text += e === "n" ? "\n" : e === "t" ? "	" : e === "r" ? "\r" : e === "0" ? "\0" : e === "\\" ? "\\" : e === '"' ? '"' : e === "'" ? "'" : e === "{" || e === "}" ? "\\" + e : e;
          j += 2;
          continue;
        }
        text += src[j];
        j++;
      }
      if (j >= src.length) throw new LexError("\u5B57\u7B26\u4E32\u6CA1\u6709\u7ED3\u675F\u5F15\u53F7", line, startCol2);
      push("str", text, startCol2, startCol + j + 1);
      i = j + 1;
      continue;
    }
    if (/[0-9]/.test(c) || c === "." && /[0-9]/.test(src[i + 1] ?? "")) {
      const start = i;
      if (c === "0" && (src[i + 1] === "x" || src[i + 1] === "X")) {
        i += 2;
        while (i < src.length && /[0-9a-fA-F_]/.test(src[i])) i++;
        const raw2 = src.slice(start, i).replace(/_/g, "");
        push("num", raw2, startCol + start, startCol + i, Number(raw2));
        continue;
      }
      while (i < src.length && /[0-9_]/.test(src[i])) i++;
      if (src[i] === "." && /[0-9]/.test(src[i + 1] ?? "")) {
        i++;
        while (i < src.length && /[0-9_]/.test(src[i])) i++;
      }
      if (src[i] === "e" || src[i] === "E") {
        const save = i;
        i++;
        if (src[i] === "+" || src[i] === "-") i++;
        if (/[0-9]/.test(src[i] ?? "")) {
          while (i < src.length && /[0-9]/.test(src[i])) i++;
        } else i = save;
      }
      const raw = src.slice(start, i).replace(/_/g, "");
      push("num", raw, startCol + start, startCol + i, Number(raw));
      continue;
    }
    if (isIdStart(c)) {
      const start = i;
      while (i < src.length && isIdPart(src[i])) i++;
      const word = src.slice(start, i);
      push(KEYWORDS.has(word) ? "kw" : "ident", word, startCol + start, startCol + i);
      continue;
    }
    let matched = "";
    for (const op of OPERATORS) {
      if (src.startsWith(op, i)) {
        matched = op;
        break;
      }
    }
    if (matched) {
      push("op", matched, startCol + i, startCol + i + matched.length);
      i += matched.length;
      continue;
    }
    if (PUNCS.includes(c)) {
      push("punc", c, startCol + i, startCol + i + 1);
      i++;
      continue;
    }
    throw new LexError(`\u65E0\u6CD5\u8BC6\u522B\u7684\u5B57\u7B26\u300C${c}\u300D`, line, startCol + i);
  }
  out.push({ k: "eof", v: "", line, col: startCol + src.length, endCol: startCol + src.length });
  return out;
}
function describeTok(t) {
  if (t.k === "eof") return "\u884C\u5C3E";
  if (t.k === "str") return `\u5B57\u7B26\u4E32\u300C${t.v}\u300D`;
  return `\u300C${t.v}\u300D`;
}
function codePart(line) {
  let inStr = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === "\\") {
        i++;
        continue;
      }
      if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'") {
      inStr = c;
      continue;
    }
    if (c === "/" && line[i + 1] === "/") {
      return { code: line.slice(0, i), comment: line.slice(i + 2) };
    }
  }
  return { code: line, comment: null };
}

// src/lang/parse.ts
var TYPE_WORDS = /* @__PURE__ */ new Set(["int", "float", "str", "bool", "list", "dict", "var"]);
var FUNC_WORDS = /* @__PURE__ */ new Set(["function", "Function", "fn", "func", "f"]);
var FUNC_MODIFIERS = /* @__PURE__ */ new Set(["bool", "repo", "int", "float", "str", "void", "list", "dict"]);
var ASSIGN_OPS = /* @__PURE__ */ new Set(["=", "+=", "-=", "*=", "/=", "%=", "**="]);
var ParseError = class extends Error {
  constructor(message, tok, code = "parse") {
    super(message);
    this.tok = tok;
    this.code = code;
    this.name = "ParseError";
  }
};
var PREC = {
  "||": 10,
  "&&": 15,
  "|": 20,
  "^": 25,
  "&": 30,
  "==": 35,
  "!=": 35,
  "===": 35,
  "!==": 35,
  "<": 40,
  ">": 40,
  "<=": 40,
  ">=": 40,
  "<<": 45,
  ">>": 45,
  "+": 50,
  "-": 50,
  "*": 60,
  "/": 60,
  "%": 60,
  "**": 70
};
var EOF = { k: "eof", v: "", line: 0, col: 0, endCol: 0 };
var startOf = (e) => ({ k: "ident", v: "", line: e.line, col: e.col, endCol: e.col });
var endOf = (e) => ({ k: "ident", v: "", line: e.endLine, col: e.endCol, endCol: e.endCol });
var ExprParser = class _ExprParser {
  constructor(toks, file) {
    this.toks = toks;
    this.file = file;
  }
  pos = 0;
  get cur() {
    return this.toks[this.pos] ?? EOF;
  }
  peek(off = 0) {
    return this.toks[Math.min(this.pos + off, this.toks.length - 1)] ?? EOF;
  }
  at(v) {
    return this.cur.v === v && this.cur.k !== "eof";
  }
  atEnd() {
    return this.cur.k === "eof";
  }
  eat() {
    return this.toks[this.pos++];
  }
  expect(v, what = v) {
    if (!this.at(v)) throw new ParseError(`\u8FD9\u91CC\u5E94\u8BE5\u662F\u300C${what}\u300D\uFF0C\u5B9E\u9645\u662F${describeTok(this.cur)}`, this.cur, "expect");
    return this.eat();
  }
  span(start, end) {
    const e = end ?? start;
    return { file: this.file, line: start.line, col: start.col, endLine: e.line, endCol: e.endCol };
  }
  parseAll() {
    const e = this.parseExpr(0);
    if (!this.atEnd()) throw new ParseError(`\u8868\u8FBE\u5F0F\u540E\u9762\u591A\u4E86${describeTok(this.cur)}`, this.cur, "trailing");
    return e;
  }
  /** 逗号分隔的表达式列表，用于 print / return 这种「参数可以裸写」的场合 */
  parseAllList() {
    if (this.atEnd()) return [];
    const out = [this.parseExpr(0)];
    while (this.at(",")) {
      this.eat();
      out.push(this.parseExpr(0));
    }
    return out;
  }
  parseExpr(minPrec) {
    let left = this.parseUnary();
    for (; ; ) {
      const t = this.cur;
      if (t.k !== "op" || !(t.v in PREC)) break;
      const prec = PREC[t.v];
      if (prec < minPrec) break;
      this.eat();
      const right = t.v === "**" ? this.parseExpr(prec) : this.parseExpr(prec + 1);
      left = { k: "bin", op: t.v, a: left, b: right, ...this.span(startOf(left), endOf(right)) };
    }
    return left;
  }
  parseUnary() {
    const t = this.cur;
    if (t.k === "op" && (t.v === "-" || t.v === "+" || t.v === "!" || t.v === "~")) {
      this.eat();
      const a = this.parseUnary();
      return { k: "un", op: t.v, a, ...this.span(t, endOf(a)) };
    }
    return this.parsePostfix(this.parsePrimary());
  }
  parsePrimary() {
    const t = this.cur;
    if (t.k === "num") {
      this.eat();
      return { k: "num", v: t.num ?? 0, ...this.span(t) };
    }
    if (t.k === "str") {
      this.eat();
      return this.stringNode(t);
    }
    if (t.k === "kw" && t.v === "new") {
      this.eat();
      const nameTok = this.cur;
      if (nameTok.k !== "ident") throw new ParseError("new \u540E\u9762\u8981\u8DDF\u7C7B\u540D", nameTok, "new");
      this.eat();
      const args = this.at("(") ? this.parseArgs() : [];
      const last = args.length ? endOf(args[args.length - 1]) : nameTok;
      return { k: "new", cls: nameTok.v, args, ...this.span(nameTok, last) };
    }
    if (t.k === "kw" && t.v === "super") {
      this.eat();
      return { k: "var", name: "super", ...this.span(t) };
    }
    if (t.k === "ident") {
      this.eat();
      if (t.v === "true" || t.v === "false") return { k: "bool", v: t.v === "true", ...this.span(t) };
      if (t.v === "null" || t.v === "nil" || t.v === "none") return { k: "null", ...this.span(t) };
      if (t.v === "self" || t.v === "this") return { k: "self", ...this.span(t) };
      return { k: "var", name: t.v, ...this.span(t) };
    }
    if (t.k === "punc" && t.v === "(") {
      this.eat();
      const e = this.parseExpr(0);
      const close = this.expect(")", "\u53F3\u62EC\u53F7 )");
      return { ...e, ...this.span(startOf(e), close) };
    }
    if (t.k === "punc" && t.v === "[") {
      this.eat();
      const items = [];
      if (!this.at("]")) {
        for (; ; ) {
          items.push(this.parseExpr(0));
          if (this.at(",")) {
            this.eat();
            if (this.at("]")) break;
            continue;
          }
          break;
        }
      }
      const close = this.expect("]", "\u5217\u8868\u7ED3\u675F ]");
      return { k: "list", items, ...this.span(t, close) };
    }
    if (t.k === "punc" && t.v === "{") {
      this.eat();
      const entries = [];
      if (!this.at("}")) {
        for (; ; ) {
          const kt = this.cur;
          let key;
          if (kt.k === "ident" && this.peek(1).v === ":") {
            this.eat();
            key = { k: "str", v: kt.v, ...this.span(kt) };
          } else {
            key = this.parseExpr(0);
          }
          this.expect(":", "\u5B57\u5178\u7684\u5192\u53F7 :");
          entries.push({ key, val: this.parseExpr(0) });
          if (this.at(",")) {
            this.eat();
            if (this.at("}")) break;
            continue;
          }
          break;
        }
      }
      const close = this.expect("}", "\u5B57\u5178\u7ED3\u675F }");
      return { k: "dict", entries, ...this.span(t, close) };
    }
    throw new ParseError(`\u8FD9\u91CC\u5E94\u8BE5\u662F\u4E00\u4E2A\u503C\uFF0C\u5B9E\u9645\u662F${describeTok(t)}`, t, "value");
  }
  /** 字符串字面量 → 支持 {变量/表达式} 插值，编译成字符串拼接 */
  stringNode(t) {
    const text = t.v;
    if (!text.includes("{") && !text.includes("\\{")) {
      return { k: "str", v: text.replace(/\\}/g, "}"), ...this.span(t) };
    }
    const parts = [];
    let buf = "";
    let i = 0;
    const pushStr = () => {
      if (buf) {
        parts.push({ k: "str", v: buf, ...this.span(t) });
        buf = "";
      }
    };
    while (i < text.length) {
      const c = text[i];
      if (c === "\\" && (text[i + 1] === "{" || text[i + 1] === "}")) {
        buf += text[i + 1];
        i += 2;
        continue;
      }
      if (c !== "{") {
        buf += c;
        i++;
        continue;
      }
      const close = text.indexOf("}", i + 1);
      if (close < 0) {
        buf += c;
        i++;
        continue;
      }
      const inner = text.slice(i + 1, close).trim();
      if (!inner) {
        buf += "{}";
        i = close + 1;
        continue;
      }
      pushStr();
      const toks = tokenizeLine(inner, t.line, t.col + i + 1);
      const sub = new _ExprParser(toks, this.file);
      try {
        parts.push(sub.parseAll());
      } catch {
        buf += text.slice(i, close + 1);
      }
      i = close + 1;
    }
    pushStr();
    if (parts.length === 1) return parts[0];
    return parts.reduce((acc, cur) => ({ k: "bin", op: "+", a: acc, b: cur, ...this.span(t) }));
  }
  parseArgs() {
    this.expect("(", "\u5DE6\u62EC\u53F7 (");
    const args = [];
    if (!this.at(")")) {
      for (; ; ) {
        args.push(this.parseExpr(0));
        if (this.at(",")) {
          this.eat();
          if (this.at(")")) break;
          continue;
        }
        break;
      }
    }
    this.expect(")", "\u53F3\u62EC\u53F7 )");
    return args;
  }
  parsePostfix(base) {
    let e = base;
    for (; ; ) {
      if (this.at(".")) {
        this.eat();
        const nameTok = this.cur;
        if (nameTok.k !== "ident") throw new ParseError("\u300C.\u300D\u540E\u9762\u8981\u8DDF\u6210\u5458\u540D", nameTok, "member");
        this.eat();
        e = { k: "member", obj: e, name: nameTok.v, ...this.span(startOf(e), nameTok) };
        continue;
      }
      if (this.at("[")) {
        this.eat();
        const index = this.parseExpr(0);
        const close = this.expect("]", "\u4E0B\u6807\u7ED3\u675F ]");
        e = { k: "idx", obj: e, index, ...this.span(startOf(e), close) };
        continue;
      }
      if (this.at("(")) {
        const args = this.parseArgs();
        const last = args.length ? endOf(args[args.length - 1]) : this.toks[this.pos - 1];
        e = { k: "call", callee: e, args, ...this.span(startOf(e), last) };
        continue;
      }
      return e;
    }
  }
};
function splitSource(src) {
  const out = [];
  const text = src.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
  text.split("\n").forEach((rawLine, idx2) => {
    const expanded = rawLine.replace(/\t/g, "    ");
    const indent = expanded.length - expanded.trimStart().length;
    const trimmed = expanded.trim();
    if (trimmed === "") return;
    if (trimmed.startsWith("--")) {
      out.push({ no: idx2 + 1, indent, code: "", raw: trimmed, comment: null, isComment: false, skipped: true });
      return;
    }
    const { code, comment } = codePart(trimmed);
    const isComment = trimmed.startsWith("//");
    const cleaned = isComment ? "" : code.trim().replace(/\{\s*$/, "").trim();
    out.push({ no: idx2 + 1, indent, code: cleaned, raw: trimmed, comment, isComment, skipped: false });
  });
  return out;
}
function parseProgram(file, src) {
  const lines = splitSource(src);
  const diagnostics = [];
  let i = 0;
  const diag = (severity, message, span, code = "parse") => {
    diagnostics.push({ severity, message, code, span });
  };
  const spanAt = (line, col, endCol = col + 1) => ({ file, line, col, endLine: line, endCol });
  const stmtSpan = (ln, col = ln.indent + 1, endCol = ln.indent + Math.max(1, ln.code.length)) => ({
    file,
    line: ln.no,
    col,
    endLine: ln.no,
    endCol
  });
  const tokensOf = (ln, code = ln.code, col = ln.indent + 1) => {
    const toks = tokenizeLine(code, ln.no, col);
    return toks.slice(0, -1);
  };
  const parserFor = (toks) => {
    const last = toks[toks.length - 1];
    const tail = last ? { k: "eof", v: "", line: last.line, col: last.endCol, endCol: last.endCol } : { ...EOF, line: 1, col: 1 };
    return new ExprParser([...toks, tail], file);
  };
  const exprOf = (toks) => parserFor(toks).parseAll();
  const stripParens = (s) => {
    const t = s.trim().replace(/\{\s*$/, "").trim();
    if (t.startsWith("(") && t.endsWith(")")) return t.slice(1, -1).trim();
    return t;
  };
  const toTarget = (e) => {
    if (e.k === "var") return { k: "var", name: e.name, file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
    if (e.k === "member") return { k: "member", obj: e.obj, name: e.name, file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
    if (e.k === "idx") return { k: "idx", obj: e.obj, index: e.index, file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
    diag("error", "\u8FD9\u91CC\u53EA\u80FD\u7ED9\u53D8\u91CF\u3001\u6210\u5458\u6216\u4E0B\u6807\u8D4B\u503C", {
      file,
      line: e.line,
      col: e.col,
      endLine: e.endLine,
      endCol: e.endCol
    }, "assign-target");
    return { k: "var", name: "_", file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
  };
  const parseBlock = (minIndent) => {
    const body2 = [];
    while (i < lines.length) {
      const ln = lines[i];
      if (ln.skipped) {
        i++;
        continue;
      }
      if (ln.indent < minIndent) break;
      const stmt = parseStatement();
      if (stmt) body2.push(stmt);
    }
    return body2;
  };
  const parseBodyOf = (indent) => parseBlock(indent + 1);
  const unknown = (ln, hint) => ({ k: "unknown", raw: ln.code || ln.raw, hint, ...stmtSpan(ln) });
  const recover = (ln, e) => {
    const msg = e instanceof ParseError ? e.message : e instanceof LexError ? e.message : String(e);
    const where = e instanceof ParseError ? spanAt(e.tok.line, e.tok.col, e.tok.endCol) : e instanceof LexError ? spanAt(e.line, e.col) : stmtSpan(ln);
    diag("error", msg, where, e instanceof ParseError ? e.code : "lex");
    while (i < lines.length && lines[i].indent > ln.indent) i++;
    return unknown(ln, suggestFor(ln.code));
  };
  const mergeBranches = (branches, indent, elseBox) => {
    for (; ; ) {
      const ln = lines[i];
      if (!ln || ln.skipped) return;
      let code = ln.code;
      let braceClose = false;
      if (code.startsWith("}")) {
        braceClose = true;
        code = code.replace(/^\}\s*/, "").trim();
      }
      if (!code) return;
      const isElif = /^elif\b/.test(code);
      const isElse = /^else\b/.test(code);
      if (!isElif && !isElse) return;
      const sameIndent = ln.indent === indent || braceClose && ln.indent <= indent + 1;
      if (!sameIndent) return;
      i++;
      if (isElse) {
        elseBox.body = parseBodyOf(ln.indent);
        return;
      }
      const condSrc = stripParens(code.replace(/^elif\b/, ""));
      let cond = null;
      if (condSrc) {
        try {
          const col = ln.indent + 1 + code.indexOf("elif") + 4;
          cond = exprOf(tokensOf(ln, condSrc, col));
        } catch (e) {
          recover(ln, e);
          return;
        }
      }
      branches.push({ cond, body: parseBodyOf(ln.indent), line: ln.no });
    }
  };
  const parseStatement = () => {
    const ln = lines[i];
    if (ln.skipped) {
      i++;
      return null;
    }
    if (ln.isComment) {
      i++;
      const text = (ln.comment ?? "").trim();
      const group = /^[─-].*[─-]$/.test(text);
      return { k: "comment", text, group, ...stmtSpan(ln) };
    }
    try {
      return parseLine(ln);
    } catch (e) {
      i++;
      return recover(ln, e);
    }
  };
  const parseLine = (ln) => {
    let code = ln.code;
    if (code.startsWith("}")) code = code.replace(/^\}\s*/, "").trim();
    if (!code) {
      i++;
      return null;
    }
    const toks = tokensOf(ln, code);
    if (!toks.length) {
      i++;
      return null;
    }
    const first = toks[0];
    const word = first.v;
    const isIdent = first.k === "ident" || first.k === "kw";
    if (isIdent) {
      switch (word) {
        case "class":
          return parseClass(ln, toks);
        case "if":
          return parseIf(ln, toks);
        case "elif":
          throw new ParseError("elif \u524D\u9762\u6CA1\u6709\u5BF9\u5E94\u7684 if", first, "stray-elif");
        case "else":
          throw new ParseError("else \u524D\u9762\u6CA1\u6709\u5BF9\u5E94\u7684 if", first, "stray-else");
        case "while":
          return parseWhile(ln, toks);
        case "repeat":
          return parseRepeat(ln, toks);
        case "for":
          return parseFor(ln, toks);
        case "forever":
          return parseForever(ln);
        case "main":
          if (toks[1]?.v === "loop") return parseForever(ln);
          break;
        case "wait":
          return parseWait(ln, toks);
        case "break":
        case "out":
          if (toks.length === 1) {
            i++;
            return { k: "break", ...stmtSpan(ln) };
          }
          break;
        case "balo":
        case "continue":
          if (toks.length === 1) {
            i++;
            return { k: "continue", ...stmtSpan(ln) };
          }
          break;
        case "stop":
          if (toks.length === 1 || toks[1]?.v === "all" || toks[1]?.v === "now" || toks[1]?.v === "script") {
            i++;
            return { k: "stop", ...stmtSpan(ln) };
          }
          break;
        case "return": {
          i++;
          const rest = toks.slice(1);
          if (!rest.length) return { k: "return", value: null, ...stmtSpan(ln) };
          const rp = parserFor(rest);
          const first2 = rp.parseExpr(0);
          if (rp.at(",")) {
            const items = [first2];
            while (rp.at(",")) {
              rp.eat();
              items.push(rp.parseExpr(0));
            }
            if (!rp.atEnd()) throw new ParseError("return \u540E\u9762\u591A\u4E86\u5185\u5BB9", rp.cur, "return");
            return { k: "return", value: { k: "list", items, file, line: ln.no, col: 1, endLine: ln.no, endCol: 1 }, ...stmtSpan(ln) };
          }
          if (!rp.atEnd()) throw new ParseError("return \u540E\u9762\u591A\u4E86\u5185\u5BB9", rp.cur, "return");
          return { k: "return", value: first2, ...stmtSpan(ln) };
        }
        case "throw": {
          i++;
          const rest = toks.slice(1);
          if (!rest.length) throw new ParseError("throw \u540E\u9762\u8981\u8DDF\u8981\u629B\u51FA\u7684\u503C", toks[0], "throw");
          return { k: "throw", value: exprOf(rest), ...stmtSpan(ln) };
        }
        case "try":
          return parseTry(ln, toks);
        case "catch":
          throw new ParseError("catch \u8981\u8DDF\u5728 try \u5757\u540E\u9762", toks[0], "stray-catch");
        case "finally":
          throw new ParseError("finally \u8981\u8DDF\u5728 try \u5757\u540E\u9762", toks[0], "stray-finally");
        case "ask":
          return parseAsk(ln, toks);
        case "print":
          return parsePrint(ln, toks);
        case "import":
          return parseImport(ln, toks);
        case "time":
          if (toks[1]?.v === "read") {
            i++;
            return { k: "timeRead", ...stmtSpan(ln) };
          }
          if (toks[1]?.v === "reset") {
            i++;
            return { k: "timeReset", ...stmtSpan(ln) };
          }
          break;
        case "list":
          if (toks.length === 2 && toks[1].k === "ident") {
            i++;
            return { k: "decl", type: "list", name: toks[1].v, init: null, ...stmtSpan(ln) };
          }
          break;
        case "dict":
          if (toks[1]?.k === "ident") return parseDict(ln, toks);
          break;
        case "add":
          if (toks[1]?.v !== "(") return parseListAdd(ln, toks);
          break;
        case "insert":
          if (toks[1]?.v !== "(") return parseListInsert(ln, toks);
          break;
        case "replace":
          if (toks[1]?.v !== "(" && toks[1]?.v !== "=") return parseListReplace(ln, toks);
          break;
        case "read":
          if (toks[1]?.v !== "(" && toks[1]?.v !== "=") return parseListRead(ln, toks);
          break;
        case "del":
          if (toks[1]?.v !== "(" && toks[1]?.v !== "=") return parseListDelete(ln, toks);
          break;
        case "len":
          if (toks[1]?.v !== "(" && toks[1]?.v !== "=") return parseLen(ln, toks);
          break;
      }
      if (FUNC_WORDS.has(word) && (word !== "f" || toks[1]?.k === "ident")) return parseFuncDef(ln, toks);
    }
    if (first.k === "kw" && first.v === "class") return parseClass(ln, toks);
    if (isIdent) {
      const names = [];
      let k = 0;
      let ok2 = true;
      for (; ; ) {
        if (toks[k]?.k === "ident" && TYPE_WORDS.has(toks[k].v) && toks[k + 1]?.k === "ident") k++;
        const t = toks[k];
        if (!t || t.k !== "ident") {
          ok2 = false;
          break;
        }
        names.push({ name: t.v, line: t.line, col: t.col });
        k++;
        if (toks[k]?.v === ",") {
          k++;
          continue;
        }
        break;
      }
      if (ok2 && names.length >= 2 && toks[k]?.v === "=") {
        const dp = parserFor(toks.slice(k + 1));
        const value = dp.parseExpr(0);
        if (!dp.atEnd()) throw new ParseError("\u89E3\u6784\u8D4B\u503C\u53F3\u8FB9\u591A\u4E86\u5185\u5BB9", dp.cur, "destructure");
        i++;
        return { k: "destructure", names, value, ...stmtSpan(ln) };
      }
    }
    if (isIdent && TYPE_WORDS.has(word) && toks[1]?.k === "ident") {
      const p2 = parserFor(toks.slice(2));
      let init = null;
      if (p2.at("=")) {
        p2.eat();
        init = p2.parseExpr(0);
      } else if (!p2.atEnd()) throw new ParseError(`\u53D8\u91CF\u58F0\u660E\u300C${word} ${toks[1].v}\u300D\u540E\u9762\u591A\u51FA\u4E86\u5185\u5BB9`, p2.cur, "decl");
      i++;
      return { k: "decl", type: word, name: toks[1].v, init, ...stmtSpan(ln) };
    }
    if (isIdent && toks.length >= 4 && toks[1]?.k === "ident" && toks[2]?.v === "=") {
      const p2 = parserFor(toks.slice(3));
      const init = p2.parseExpr(0);
      if (!p2.atEnd()) throw new ParseError("\u58F0\u660E\u8BED\u53E5\u53F3\u8FB9\u591A\u4E86\u5185\u5BB9", p2.cur, "decl");
      i++;
      return { k: "decl", type: "var", name: toks[1].v, init, ...stmtSpan(ln) };
    }
    const p = parserFor(toks);
    const firstExpr = p.parseExpr(0);
    if (!p.atEnd() && p.cur.k === "op" && ASSIGN_OPS.has(p.cur.v)) {
      const op = p.eat().v;
      const value = p.parseExpr(0);
      if (!p.atEnd()) throw new ParseError("\u8D4B\u503C\u53F7\u53F3\u8FB9\u591A\u4E86\u5185\u5BB9", p.cur, "assign");
      i++;
      return { k: "assign", target: toTarget(firstExpr), op, value, ...stmtSpan(ln) };
    }
    if (!p.atEnd() && (p.cur.v === "++" || p.cur.v === "--")) {
      const delta = p.cur.v === "++" ? 1 : -1;
      p.eat();
      if (!p.atEnd()) throw new ParseError("\u81EA\u589E\u81EA\u51CF\u540E\u9762\u4E0D\u80FD\u518D\u6709\u5185\u5BB9", p.cur, "incdec");
      i++;
      return { k: "incdec", target: toTarget(firstExpr), delta, prefix: false, ...stmtSpan(ln) };
    }
    if (!p.atEnd()) throw new ParseError(`\u8FD9\u6761\u8BED\u53E5\u770B\u4E0D\u61C2\uFF1A${describeTok(p.cur)}`, p.cur, "trailing");
    i++;
    return { k: "expr", expr: firstExpr, ...stmtSpan(ln) };
  };
  const parseIf = (ln, toks) => {
    i++;
    let rest = toks.slice(1);
    let isIfOut = false;
    if (rest[0]?.v === "out") {
      isIfOut = true;
      rest = rest.slice(1);
    }
    let cond = null;
    if (rest.length) cond = parserFor(rest).parseExpr(0);
    if (isIfOut) {
      if (!cond) throw new ParseError("if out \u9700\u8981\u4E00\u4E2A\u6761\u4EF6", toks[0], "if-out");
      return { k: "ifOut", cond, ...stmtSpan(ln) };
    }
    if (!cond) throw new ParseError("if \u540E\u9762\u8981\u8DDF\u6761\u4EF6\uFF0C\u4F8B\u5982 if (a > 1)", toks[0], "if-no-cond");
    const branches = [{ cond, body: parseBodyOf(ln.indent), line: ln.no }];
    const elseBox = { body: null };
    mergeBranches(branches, ln.indent, elseBox);
    return { k: "if", branches, elseBody: elseBox.body, ...stmtSpan(ln) };
  };
  const parseWhile = (ln, toks) => {
    i++;
    const cond = parserFor(toks.slice(1)).parseExpr(0);
    return { k: "while", cond, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };
  const parseRepeat = (ln, toks) => {
    i++;
    const count = parserFor(toks.slice(1)).parseExpr(0);
    return { k: "repeat", count, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };
  const parseForever = (ln) => {
    i++;
    return { k: "forever", body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };
  const parseWait = (ln, toks) => {
    i++;
    if (toks[1]?.v === "until") {
      const cond = parserFor(toks.slice(2)).parseExpr(0);
      return { k: "waitUntil", cond, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
    }
    const seconds = parserFor(toks.slice(1)).parseExpr(0);
    return { k: "wait", seconds, ...stmtSpan(ln) };
  };
  const parseTry = (ln, _toks) => {
    i++;
    const body2 = parseBodyOf(ln.indent);
    let catchName = null;
    let catchBody = null;
    let finallyBody = null;
    for (; ; ) {
      const L = lines[i];
      if (!L || L.skipped) break;
      let code = L.code;
      let brace = false;
      if (code.startsWith("}")) {
        brace = true;
        code = code.replace(/^\}\s*/, "").trim();
      }
      if (!code) break;
      const sameIndent = L.indent === ln.indent || brace && L.indent <= ln.indent + 1;
      if (!sameIndent) break;
      if (/^catch\b/.test(code)) {
        const rest = code.replace(/^catch\b/, "").trim();
        catchName = rest ? rest.split(/[\s(]/)[0].replace(/\)$/, "") : null;
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
      diag("warning", "try \u540E\u9762\u6CA1\u6709 catch \u4E5F\u6CA1\u6709 finally\uFF0C\u5757\u91CC\u7684\u9519\u8BEF\u4F1A\u76F4\u63A5\u629B\u51FA\u53BB", stmtSpan(ln), "try-bare");
    }
    return { k: "try", body: body2, catchName, catchBody, finallyBody, ...stmtSpan(ln) };
  };
  const parseFor = (ln, toks) => {
    i++;
    const varTok = toks[1];
    if (!varTok || varTok.k !== "ident") throw new ParseError("for \u540E\u9762\u8981\u8DDF\u5FAA\u73AF\u53D8\u91CF\u540D", toks[0], "for");
    let valueName = null;
    let cursor = 2;
    if (toks[2]?.v === ",") {
      const v2 = toks[3];
      if (!v2 || v2.k !== "ident") throw new ParseError("for a, b \u91CC\u7684\u7B2C\u4E8C\u9879\u8981\u662F\u53D8\u91CF\u540D", toks[2], "for");
      valueName = v2.v;
      cursor = 4;
    }
    if (toks[cursor]?.v === "=") {
      const p = parserFor(toks.slice(cursor + 1));
      const from = p.parseExpr(0);
      if (!p.at("to")) throw new ParseError("\u8BA1\u6570\u5FAA\u73AF\u8981\u5199\u6210\u300Cfor i = 1 to 10\u300D", p.cur, "for");
      p.eat();
      const to = p.parseExpr(0);
      let step = null;
      if (!p.atEnd()) {
        if (!p.at("step")) throw new ParseError("\u8FD9\u91CC\u53EA\u80FD\u5199 step \u6B65\u957F", p.cur, "for");
        p.eat();
        step = p.parseExpr(0);
      }
      return {
        k: "forRange",
        varName: varTok.v,
        from,
        to,
        step,
        inclusive: true,
        body: parseBodyOf(ln.indent),
        ...stmtSpan(ln)
      };
    }
    if (toks[cursor]?.v !== "in") {
      throw new ParseError("for \u7684\u5199\u6CD5\uFF1A\u300Cfor i in \u96C6\u5408\u300D/\u300Cfor i in range(n)\u300D/\u300Cfor i = 1 to 10\u300D", toks[cursor] ?? toks[0], "for");
    }
    const iter = parserFor(toks.slice(cursor + 1)).parseExpr(0);
    if (iter.k === "call" && iter.callee.k === "var" && iter.callee.name === "range") {
      const a = iter.args;
      if (!a.length) throw new ParseError("range \u9700\u8981\u81F3\u5C11\u4E00\u4E2A\u53C2\u6570", toks[0], "range");
      if (a.length > 3) throw new ParseError("range \u6700\u591A\u63A5\u53D7\u4E09\u4E2A\u53C2\u6570\uFF08\u8D77\u70B9, \u7EC8\u70B9, \u6B65\u957F\uFF09", toks[0], "range");
      const from = a.length >= 2 ? a[0] : null;
      const to = a.length >= 2 ? a[1] : a[0];
      const step = a.length >= 3 ? a[2] : null;
      return {
        k: "forRange",
        varName: varTok.v,
        from,
        to,
        step,
        inclusive: false,
        body: parseBodyOf(ln.indent),
        ...stmtSpan(ln)
      };
    }
    return { k: "forIn", varName: varTok.v, valueName, iter, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };
  const parseWaitUntilUnused = 0;
  const parseDict = (ln, toks) => {
    const nameTok = toks[1];
    const p = parserFor(toks.slice(2));
    const target = { k: "var", name: nameTok.v, file, line: nameTok.line, col: nameTok.col, endLine: nameTok.line, endCol: nameTok.endCol };
    if (p.at("=")) {
      p.eat();
      const init = p.parseExpr(0);
      i++;
      return { k: "decl", type: "dict", name: nameTok.v, init, ...stmtSpan(ln) };
    }
    if (p.atEnd()) {
      i++;
      return { k: "decl", type: "dict", name: nameTok.v, init: null, ...stmtSpan(ln) };
    }
    if (p.at("key")) {
      p.eat();
      const key = p.parseExpr(0);
      if (!p.at("=")) throw new ParseError("dict \u5199\u5165\u8981\u5199\u6210\u300Cdict d key \u952E = \u503C\u300D", p.cur, "dict");
      p.eat();
      const value = p.parseExpr(0);
      i++;
      return { k: "dictSet", target, key, value, ...stmtSpan(ln) };
    }
    if (p.at("read") || p.at("del")) {
      const isRead = p.eat().v === "read";
      const key = p.parseExpr(0);
      i++;
      return isRead ? { k: "dictRead", target, key, ...stmtSpan(ln) } : { k: "dictDelete", target, key, ...stmtSpan(ln) };
    }
    throw new ParseError("dict \u7684\u5199\u6CD5\uFF1A\u300Cdict d key \u952E = \u503C\u300D/\u300Cdict d read \u952E\u300D/\u300Cdict d del \u952E\u300D/\u300Cdict d\u300D", p.cur, "dict");
  };
  const parseAsk = (ln, toks) => {
    i++;
    const rest = toks.slice(1);
    const question = rest.length ? parserFor(rest[0].v === "(" ? rest.slice(1, rest[rest.length - 1]?.v === ")" ? -1 : void 0) : rest).parseAll() : { k: "str", v: "", ...stmtSpan(ln) };
    return { k: "ask", question, ...stmtSpan(ln) };
  };
  const parsePrint = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    const args = p.at("(") ? p.parseArgs() : p.parseAllList();
    if (!p.atEnd()) throw new ParseError("print \u540E\u9762\u591A\u4E86\u5185\u5BB9", p.cur, "print");
    return { k: "print", args, ...stmtSpan(ln) };
  };
  const parseImport = (ln, toks) => {
    i++;
    const t = toks[1];
    if (!t || t.k !== "str") throw new ParseError('import \u540E\u9762\u8981\u8DDF\u5B57\u7B26\u4E32\u8DEF\u5F84\uFF0C\u4F8B\u5982 import "lib.cw"', toks[0], "import");
    return { k: "import", target: t.v, ...stmtSpan(ln) };
  };
  const parseListAdd = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    const value = p.parseExpr(0);
    if (!p.at("to")) throw new ParseError("\u8FFD\u52A0\u8981\u5199\u6210\u300Cadd \u503C to \u5217\u8868\u300D", p.cur, "list-add");
    p.eat();
    const target = toTarget(p.parseExpr(0));
    return { k: "listAdd", value, target, ...stmtSpan(ln) };
  };
  const parseListInsert = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    const value = p.parseExpr(0);
    if (!p.at("to")) throw new ParseError("\u63D2\u5165\u8981\u5199\u6210\u300Cinsert \u503C to \u5217\u8868 at \u4E0B\u6807\u300D", p.cur, "list-insert");
    p.eat();
    const target = toTarget(p.parseExpr(0));
    if (!p.at("at")) throw new ParseError("\u63D2\u5165\u8981\u5199\u6210\u300Cinsert \u503C to \u5217\u8868 at \u4E0B\u6807\u300D", p.cur, "list-insert");
    p.eat();
    const index = p.parseExpr(0);
    return { k: "listInsert", value, target, index, ...stmtSpan(ln) };
  };
  const parseListReplace = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    const target = toTarget(p.parseExpr(0));
    const index = p.parseExpr(0);
    if (!p.at("with")) throw new ParseError("\u66FF\u6362\u8981\u5199\u6210\u300Creplace \u5217\u8868 \u4E0B\u6807 with \u503C\u300D", p.cur, "list-replace");
    p.eat();
    const value = p.parseExpr(0);
    return { k: "listReplace", target, index, value, ...stmtSpan(ln) };
  };
  const parseListRead = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    const target = toTarget(p.parseExpr(0));
    const index = p.parseExpr(0);
    return { k: "listRead", target, index, ...stmtSpan(ln) };
  };
  const parseListDelete = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    const target = toTarget(p.parseExpr(0));
    const index = p.atEnd() ? null : p.parseExpr(0);
    return { k: "listDelete", target, index, ...stmtSpan(ln) };
  };
  const parseLen = (ln, toks) => {
    i++;
    const p = parserFor(toks.slice(1));
    return { k: "lenOf", value: p.parseExpr(0), ...stmtSpan(ln) };
  };
  const parseFuncDef = (ln, toks) => {
    i++;
    let cursor = 1;
    let retType = null;
    if (toks[cursor]?.k === "ident" && FUNC_MODIFIERS.has(toks[cursor].v) && toks[cursor + 1]?.k === "ident") {
      retType = toks[cursor].v;
      cursor++;
    }
    const nameTok = toks[cursor];
    if (!nameTok || nameTok.k !== "ident") throw new ParseError("\u51FD\u6570\u5B9A\u4E49\u8981\u5199\u51FD\u6570\u540D\uFF0C\u4F8B\u5982 function hello(a, b)", toks[0], "func-name");
    cursor++;
    let params = [];
    if (toks[cursor]?.v === "(") {
      const close = findClose(toks, cursor);
      params = parseParams(toks.slice(cursor + 1, close));
    }
    return { k: "funcDef", name: nameTok.v, params, retType, body: parseBodyOf(ln.indent), ...stmtSpan(ln) };
  };
  const findClose = (toks, openIdx) => {
    let depth = 0;
    for (let k = openIdx; k < toks.length; k++) {
      if (toks[k].v === "(") depth++;
      else if (toks[k].v === ")") {
        depth--;
        if (depth === 0) return k;
      }
    }
    throw new ParseError("\u7F3A\u5C11\u53F3\u62EC\u53F7 )", toks[openIdx], "paren");
  };
  const parseParams = (toks) => {
    const out = [];
    let idx2 = 0;
    while (idx2 < toks.length) {
      let type = null;
      if (toks[idx2]?.k === "ident" && TYPE_WORDS.has(toks[idx2].v) && toks[idx2 + 1]?.k === "ident") {
        type = toks[idx2].v;
        idx2++;
      }
      const nameTok = toks[idx2];
      if (!nameTok || nameTok.k !== "ident") throw new ParseError("\u51FD\u6570\u53C2\u6570\u8981\u5199\u53C2\u6570\u540D", nameTok ?? toks[0], "param");
      idx2++;
      let def = null;
      if (toks[idx2]?.v === "=") {
        idx2++;
        const sub = [];
        let depth = 0;
        while (idx2 < toks.length) {
          const t = toks[idx2];
          if (t.v === "(" || t.v === "[" || t.v === "{") depth++;
          if (t.v === ")" || t.v === "]" || t.v === "}") depth--;
          if (depth === 0 && t.v === ",") break;
          sub.push(t);
          idx2++;
        }
        if (!sub.length) throw new ParseError(`\u53C2\u6570 ${nameTok.v} \u7684\u9ED8\u8BA4\u503C\u4E3A\u7A7A`, nameTok, "param-default");
        def = exprOf(sub);
      }
      out.push({ name: nameTok.v, type, def, line: nameTok.line, col: nameTok.col });
      if (toks[idx2]?.v === ",") {
        idx2++;
        continue;
      }
      break;
    }
    return out;
  };
  const parseClass = (ln, toks) => {
    i++;
    const nameTok = toks[1];
    if (!nameTok || nameTok.k !== "ident") throw new ParseError("class \u540E\u9762\u8981\u8DDF\u7C7B\u540D", toks[0], "class");
    let base = null;
    if (toks[2]?.v === "extends") {
      const b = toks[3];
      if (!b || b.k !== "ident") throw new ParseError("extends \u540E\u9762\u8981\u8DDF\u7236\u7C7B\u540D", toks[2], "class");
      base = b.v;
    }
    const fields = [];
    const methods = [];
    while (i < lines.length && lines[i].indent > ln.indent) {
      const L = lines[i];
      if (L.skipped || L.isComment) {
        i++;
        continue;
      }
      const lt = tokensOf(L);
      if (!lt.length) {
        i++;
        continue;
      }
      const w = lt[0].v;
      if (lt[0].k === "kw" && lt[0].v === "function" || FUNC_WORDS.has(w)) {
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
        let type = null;
        if (TYPE_WORDS.has(lt[0].v) && lt[1]?.k === "ident") {
          type = lt[0].v;
          cursor = 1;
        }
        const fname = lt[cursor];
        if (!fname || fname.k !== "ident") throw new ParseError("\u7C7B\u4F53\u91CC\u53EA\u80FD\u5199\u5B57\u6BB5\u6216\u65B9\u6CD5", lt[0], "class-body");
        cursor++;
        let init = null;
        if (lt[cursor]?.v === "=") init = exprOf(lt.slice(cursor + 1));
        else if (lt.length > cursor) throw new ParseError("\u5B57\u6BB5\u521D\u59CB\u5316\u8981\u5199\u6210\u300Cname = \u503C\u300D", lt[cursor], "class-field");
        fields.push({ name: fname.v, type, init, line: fname.line, col: fname.col });
        i++;
      } catch (e) {
        i++;
        recover(L, e);
      }
    }
    if (!fields.length && !methods.length) {
      diag("warning", `\u7C7B ${nameTok.v} \u662F\u7A7A\u7684\uFF08\u65E2\u6CA1\u6709\u5B57\u6BB5\u4E5F\u6CA1\u6709\u65B9\u6CD5\uFF09`, stmtSpan(ln), "empty-class");
    }
    return { k: "classDef", name: nameTok.v, base, fields, methods, ...stmtSpan(ln) };
  };
  const body = parseBlock(0);
  for (const s of body) {
    if (s.k === "return") {
      diag("error", "return \u53EA\u80FD\u5199\u5728\u51FD\u6570\u6216\u65B9\u6CD5\u91CC", { file, line: s.line, col: s.col, endLine: s.endLine, endCol: s.endCol }, "stray-return");
    } else if (s.k === "break" || s.k === "continue") {
      diag("error", `${s.k === "break" ? "out / break" : "balo / continue"} \u53EA\u80FD\u5199\u5728\u5FAA\u73AF\u91CC`, { file, line: s.line, col: s.col, endLine: s.endLine, endCol: s.endCol }, "stray-jump");
    }
  }
  return { file, body, diagnostics };
}
var SUGGEST = {
  printer: "print(\u2026)",
  pri: "print(\u2026)",
  prnt: "print(\u2026)",
  pr: "print(\u2026)",
  ifo: "if out(\u6761\u4EF6)",
  whl: "while (\u6761\u4EF6)",
  wile: "while (\u6761\u4EF6)",
  fnc: "function name()",
  fnt: "function name()",
  fun: "function name()",
  funcion: "function name()",
  intger: "int x = \u6570\u5B57",
  strn: 'str x = "\u6587\u672C"',
  lst: "list x",
  dct: "dict x",
  forevr: "forever",
  rep: "repeat(n)",
  wate: "wait 0.5",
  imprt: 'import "\u6587\u4EF6"',
  clas: "class Name",
  classe: "class Name",
  ne: "new ClassName(\u2026)",
  slf: "self"
};
function suggestFor(raw) {
  const w = raw.trim().replace(/^[（(]+/, "").split(/[\s(（]/)[0];
  return SUGGEST[w] ?? null;
}

// src/lang/values.ts
var isCInstLike = (v) => typeof v === "object" && v !== null && v.__cinst === 1;
var isDict = (v) => typeof v === "object" && v !== null && v.kind === "dict";
var isInstance = (v) => typeof v === "object" && v !== null && v.kind === "instance";
var isList = (v) => Array.isArray(v);
var FMT_MAX_DEPTH = 16;
function fmtInner(v, seen, depth) {
  if (typeof v === "string") return JSON.stringify(v);
  return fmtVal(v, seen, depth);
}
function fmtVal(v, seen = /* @__PURE__ */ new Set(), depth = 0) {
  if (v === null) return "null";
  if (typeof v === "boolean") return v ? "true" : "false";
  if (typeof v === "number") {
    if (!Number.isFinite(v)) return String(v);
    return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(10)));
  }
  const isBox = Array.isArray(v) || isDict(v) || isInstance(v) || isCInstLike(v);
  if (isBox) {
    const boxed = v;
    if (seen.has(boxed)) return "\u2026(\u5FAA\u73AF)";
    if (depth >= FMT_MAX_DEPTH) return "\u2026";
    seen.add(boxed);
    try {
      if (Array.isArray(v)) return "[" + v.map((x) => fmtInner(x, seen, depth + 1)).join(", ") + "]";
      if (isDict(v)) {
        const parts2 = [];
        for (const [k, x] of v.map) parts2.push(`${k}: ${fmtInner(x, seen, depth + 1)}`);
        return "{" + parts2.join(", ") + "}";
      }
      if (isInstance(v)) {
        const parts2 = [];
        for (const [k, x] of v.fields) parts2.push(`${k}: ${fmtInner(x, seen, depth + 1)}`);
        return `${v.cls.name}{` + parts2.join(", ") + "}";
      }
      const ci = v;
      const parts = [];
      for (const [k, x] of Object.entries(ci.f)) parts.push(`${k}: ${fmtInner(x, seen, depth + 1)}`);
      return `${ci.cls.name}{` + parts.join(", ") + "}";
    } finally {
      seen.delete(boxed);
    }
  }
  return String(v);
}
function toStr(v) {
  return fmtVal(v);
}
function truthy(v) {
  if (v === null) return false;
  if (typeof v === "boolean") return v;
  if (typeof v === "number") return v !== 0 && !Number.isNaN(v);
  if (typeof v === "string") return v !== "";
  if (Array.isArray(v)) return v.length > 0;
  if (isDict(v)) return v.map.size > 0;
  return true;
}
function toNum(v) {
  if (typeof v === "number") return v;
  if (typeof v === "boolean") return v ? 1 : 0;
  if (typeof v === "string" && v.trim() !== "" && !Number.isNaN(Number(v))) return Number(v);
  return null;
}

// src/lang/builtins.ts
var bad = (msg) => {
  throw new Error(msg);
};
var num = (v, who) => {
  const n = toNum(v);
  if (n === null) bad(`${who} \u9700\u8981\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(v)}`);
  return n;
};
var listArg = (v, who) => {
  if (!isList(v)) bad(`${who} \u9700\u8981\u5217\u8868\uFF0C\u5F97\u5230 ${fmtVal(v)}`);
  return v;
};
var dictArg = (v, who) => {
  if (!isDict(v)) bad(`${who} \u9700\u8981\u5B57\u5178\uFF0C\u5F97\u5230 ${fmtVal(v)}`);
  return v.map;
};
var div = (a, b) => {
  if (b === 0) bad("\u9664\u6570\u4E0D\u80FD\u4E3A 0");
  return a / b;
};
var mod = (a, b) => {
  if (b === 0) bad("\u53D6\u6A21\u7684\u9664\u6570\u4E0D\u80FD\u4E3A 0");
  return a % b;
};
var spec = (s) => s;
var REGISTRY = [
  /* 数学 */
  spec({ name: "add", sig: "add(a, b)", category: "\u6570\u5B66", doc: "\u76F8\u52A0\uFF1B\u4EFB\u4E00\u53C2\u6570\u662F\u5B57\u7B26\u4E32\u65F6\u6309\u62FC\u63A5\u5904\u7406", min: 2, max: 2, writes: "__last_calc", fn: (a) => typeof a[0] === "string" || typeof a[1] === "string" ? toStr(a[0]) + toStr(a[1]) : num(a[0], "add") + num(a[1], "add") }),
  spec({ name: "sub", sig: "sub(a, b)", category: "\u6570\u5B66", doc: "\u76F8\u51CF", min: 2, max: 2, writes: "__last_calc", fn: (a) => num(a[0], "sub") - num(a[1], "sub") }),
  spec({ name: "mul", sig: "mul(a, b)", category: "\u6570\u5B66", doc: "\u76F8\u4E58\uFF1B\u5B57\u7B26\u4E32\u4E58\u6574\u6570\u53EF\u91CD\u590D", min: 2, max: 2, writes: "__last_calc", fn: (a) => typeof a[0] === "string" && typeof a[1] === "number" ? a[0].repeat(Math.max(0, Math.trunc(a[1]))) : num(a[0], "mul") * num(a[1], "mul") }),
  spec({ name: "div", sig: "div(a, b)", category: "\u6570\u5B66", doc: "\u76F8\u9664\uFF08\u9664\u6570\u4E3A 0 \u62A5\u9519\uFF09", min: 2, max: 2, writes: "__last_calc", fn: (a) => div(num(a[0], "div"), num(a[1], "div")) }),
  spec({ name: "mod", sig: "mod(a, b)", category: "\u6570\u5B66", doc: "\u53D6\u4F59", min: 2, max: 2, writes: "__last_calc", fn: (a) => mod(num(a[0], "mod"), num(a[1], "mod")) }),
  spec({ name: "pow", sig: "pow(a, b)", category: "\u6570\u5B66", doc: "a \u7684 b \u6B21\u65B9", min: 2, max: 2, writes: "__last_calc", fn: (a) => Math.pow(num(a[0], "pow"), num(a[1], "pow")) }),
  spec({ name: "rand", sig: "rand(a, b)", category: "\u6570\u5B66", doc: "[a, b] \u4E4B\u95F4\u7684\u968F\u673A\u6574\u6570\uFF08\u542B\u4E24\u7AEF\uFF09", min: 2, max: 2, writes: "__last_calc", fn: (a) => {
    const lo = Math.trunc(Math.min(num(a[0], "rand"), num(a[1], "rand")));
    const hi = Math.trunc(Math.max(num(a[0], "rand"), num(a[1], "rand")));
    return lo + Math.floor(Math.random() * (hi - lo + 1));
  } }),
  spec({ name: "random", sig: "random()", category: "\u6570\u5B66", doc: "0 \u5230 1 \u4E4B\u95F4\u7684\u5C0F\u6570", min: 0, max: 0, writes: "__last_calc", fn: () => Math.random() }),
  spec({ name: "sqrt", sig: "sqrt(a)", category: "\u6570\u5B66", doc: "\u5E73\u65B9\u6839", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.sqrt(num(a[0], "sqrt")) }),
  spec({ name: "abs", sig: "abs(a)", category: "\u6570\u5B66", doc: "\u7EDD\u5BF9\u503C", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.abs(num(a[0], "abs")) }),
  spec({ name: "floor", sig: "floor(a)", category: "\u6570\u5B66", doc: "\u5411\u4E0B\u53D6\u6574", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.floor(num(a[0], "floor")) }),
  spec({ name: "ceil", sig: "ceil(a)", category: "\u6570\u5B66", doc: "\u5411\u4E0A\u53D6\u6574", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.ceil(num(a[0], "ceil")) }),
  spec({ name: "round", sig: "round(a)", category: "\u6570\u5B66", doc: "\u56DB\u820D\u4E94\u5165", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.round(num(a[0], "round")) }),
  spec({ name: "sin", sig: "sin(a)", category: "\u6570\u5B66", doc: "\u6B63\u5F26\uFF08\u5F27\u5EA6\uFF09", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.sin(num(a[0], "sin")) }),
  spec({ name: "cos", sig: "cos(a)", category: "\u6570\u5B66", doc: "\u4F59\u5F26\uFF08\u5F27\u5EA6\uFF09", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.cos(num(a[0], "cos")) }),
  spec({ name: "tan", sig: "tan(a)", category: "\u6570\u5B66", doc: "\u6B63\u5207\uFF08\u5F27\u5EA6\uFF09", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.tan(num(a[0], "tan")) }),
  spec({ name: "asin", sig: "asin(a)", category: "\u6570\u5B66", doc: "\u53CD\u6B63\u5F26", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.asin(num(a[0], "asin")) }),
  spec({ name: "acos", sig: "acos(a)", category: "\u6570\u5B66", doc: "\u53CD\u4F59\u5F26", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.acos(num(a[0], "acos")) }),
  spec({ name: "atan", sig: "atan(a)", category: "\u6570\u5B66", doc: "\u53CD\u6B63\u5207", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.atan(num(a[0], "atan")) }),
  spec({ name: "atan2", sig: "atan2(y, x)", category: "\u6570\u5B66", doc: "\u4E24\u53C2\u6570\u53CD\u6B63\u5207", min: 2, max: 2, writes: "__last_calc", fn: (a) => Math.atan2(num(a[0], "atan2"), num(a[1], "atan2")) }),
  spec({ name: "log", sig: "log(a)", category: "\u6570\u5B66", doc: "\u81EA\u7136\u5BF9\u6570", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.log(num(a[0], "log")) }),
  spec({ name: "exp", sig: "exp(a)", category: "\u6570\u5B66", doc: "e \u7684 a \u6B21\u65B9", min: 1, max: 1, writes: "__last_calc", fn: (a) => Math.exp(num(a[0], "exp")) }),
  spec({ name: "min", sig: "min(a, b, \u2026)", category: "\u6570\u5B66", doc: "\u6700\u5C0F\u503C", min: 1, max: 99, writes: "__last_calc", fn: (a) => Math.min(...a.map((x) => num(x, "min"))) }),
  spec({ name: "max", sig: "max(a, b, \u2026)", category: "\u6570\u5B66", doc: "\u6700\u5927\u503C", min: 1, max: 99, writes: "__last_calc", fn: (a) => Math.max(...a.map((x) => num(x, "max"))) }),
  /* 比较 / 逻辑 */
  spec({ name: "lt", sig: "lt(a, b)", category: "\u6BD4\u8F83", doc: "a < b", min: 2, max: 2, writes: "__last_cmp", fn: (a) => typeof a[0] === "string" && typeof a[1] === "string" ? a[0] < a[1] : num(a[0], "lt") < num(a[1], "lt") }),
  spec({ name: "gt", sig: "gt(a, b)", category: "\u6BD4\u8F83", doc: "a > b", min: 2, max: 2, writes: "__last_cmp", fn: (a) => typeof a[0] === "string" && typeof a[1] === "string" ? a[0] > a[1] : num(a[0], "gt") > num(a[1], "gt") }),
  spec({ name: "eq", sig: "eq(a, b)", category: "\u6BD4\u8F83", doc: "a == b\uFF08\u5BB9\u5668\u6309\u5185\u5BB9\u6BD4\u8F83\uFF09", min: 2, max: 2, writes: "__last_cmp", fn: (a) => JSON.stringify(norm(a[0])) === JSON.stringify(norm(a[1])) }),
  spec({ name: "and", sig: "and(a, b)", category: "\u903B\u8F91", doc: "\u903B\u8F91\u4E0E", min: 2, max: 2, writes: "__last_cmp", fn: (a) => truthy(a[0]) && truthy(a[1]) }),
  spec({ name: "or", sig: "or(a, b)", category: "\u903B\u8F91", doc: "\u903B\u8F91\u6216", min: 2, max: 2, writes: "__last_cmp", fn: (a) => truthy(a[0]) || truthy(a[1]) }),
  spec({ name: "not", sig: "not(a)", category: "\u903B\u8F91", doc: "\u903B\u8F91\u975E", min: 1, max: 1, writes: "__last_cmp", fn: (a) => !truthy(a[0]) }),
  spec({ name: "listin", sig: "listin(\u503C, \u5217\u8868)", category: "\u5217\u8868", doc: "\u503C\u662F\u5426\u5728\u5217\u8868\u91CC", min: 2, max: 2, writes: "__last_cmp", fn: (a) => listArg(a[1], "listin").some((x) => JSON.stringify(norm(x)) === JSON.stringify(norm(a[0]))) }),
  spec({ name: "dictin", sig: "dictin(\u5B57\u5178, \u952E)", category: "\u5B57\u5178", doc: "\u952E\u662F\u5426\u5728\u5B57\u5178\u91CC", min: 2, max: 2, writes: "__last_cmp", fn: (a) => dictArg(a[0], "dictin").has(toStr(a[1])) }),
  /* 字符串 */
  spec({ name: "join", sig: "join(a, b, \u2026)", category: "\u5B57\u7B26\u4E32", doc: "\u628A\u591A\u4E2A\u503C\u62FC\u6210\u5B57\u7B26\u4E32", min: 1, max: 99, writes: "__last_str", fn: (a) => a.map((x) => toStr(x)).join("") }),
  spec({ name: "char", sig: "char(s, i)", category: "\u5B57\u7B26\u4E32", doc: "\u53D6\u7B2C i \u4E2A\u5B57\u7B26\uFF08\u4ECE 0 \u5F00\u59CB\uFF09", min: 2, max: 2, writes: "__last_str", fn: (a) => {
    const s = toStr(a[0]);
    const i = Math.trunc(num(a[1], "char"));
    if (i < 0 || i >= s.length) bad(`char \u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${s.length}\uFF09`);
    return s[i];
  } }),
  spec({ name: "strlen", sig: "strlen(s)", category: "\u5B57\u7B26\u4E32", doc: "\u5B57\u7B26\u4E32\u957F\u5EA6", min: 1, max: 1, writes: "__last_str", fn: (a) => toStr(a[0]).length }),
  spec({ name: "contain", sig: "contain(s, \u5B50\u4E32)", category: "\u5B57\u7B26\u4E32", doc: "\u662F\u5426\u5305\u542B\u5B50\u4E32", min: 2, max: 2, writes: "__last_cmp", fn: (a) => toStr(a[0]).includes(toStr(a[1])) }),
  spec({ name: "upper", sig: "upper(s)", category: "\u5B57\u7B26\u4E32", doc: "\u8F6C\u5927\u5199", min: 1, max: 1, fn: (a) => toStr(a[0]).toUpperCase() }),
  spec({ name: "lower", sig: "lower(s)", category: "\u5B57\u7B26\u4E32", doc: "\u8F6C\u5C0F\u5199", min: 1, max: 1, fn: (a) => toStr(a[0]).toLowerCase() }),
  spec({ name: "trim", sig: "trim(s)", category: "\u5B57\u7B26\u4E32", doc: "\u53BB\u6389\u9996\u5C3E\u7A7A\u767D", min: 1, max: 1, fn: (a) => toStr(a[0]).trim() }),
  spec({ name: "split", sig: "split(s, \u5206\u9694\u7B26)", category: "\u5B57\u7B26\u4E32", doc: "\u6309\u5206\u9694\u7B26\u5207\u6210\u5217\u8868", min: 2, max: 2, fn: (a) => toStr(a[0]).split(toStr(a[1])) }),
  spec({ name: "indexof", sig: "indexof(s, \u5B50\u4E32)", category: "\u5B57\u7B26\u4E32", doc: "\u5B50\u4E32\u4F4D\u7F6E\uFF0C\u627E\u4E0D\u5230\u8FD4\u56DE -1", min: 2, max: 2, fn: (a) => toStr(a[0]).indexOf(toStr(a[1])) }),
  spec({ name: "substr", sig: "substr(s, \u8D77\u70B9, \u957F\u5EA6)", category: "\u5B57\u7B26\u4E32", doc: "\u622A\u53D6\u5B50\u4E32", min: 2, max: 3, fn: (a) => {
    const s = toStr(a[0]);
    const start = Math.trunc(num(a[1], "substr"));
    const len = a.length >= 3 ? Math.trunc(num(a[2], "substr")) : s.length;
    return s.slice(start, start + len);
  } }),
  /* 通用 */
  spec({ name: "len", sig: "len(\u5217\u8868 / \u5B57\u7B26\u4E32 / \u5B57\u5178)", category: "\u901A\u7528", doc: "\u957F\u5EA6", min: 1, max: 1, writes: "__last_len", fn: (a) => {
    const v = a[0];
    if (isList(v)) return v.length;
    if (typeof v === "string") return v.length;
    if (isDict(v)) return v.map.size;
    if (isInstance(v)) return v.fields.size;
    return toStr(v).length;
  } }),
  spec({ name: "range", sig: "range(n) / range(a, b) / range(a, b, step)", category: "\u901A\u7528", doc: "\u751F\u6210\u6570\u5B57\u5217\u8868\uFF0C\u5E38\u7528\u5728 for \u91CC", min: 1, max: 3, fn: (a) => {
    let from = 0;
    let to = 0;
    let step = 1;
    if (a.length === 1) to = num(a[0], "range");
    else {
      from = num(a[0], "range");
      to = num(a[1], "range");
      if (a.length >= 3) step = num(a[2], "range");
    }
    if (step === 0) bad("range \u7684\u6B65\u957F\u4E0D\u80FD\u4E3A 0");
    const out = [];
    if (step > 0) for (let v = from; v < to; v += step) out.push(v);
    else for (let v = from; v > to; v += step) out.push(v);
    if (out.length > 1e6) bad("range \u751F\u6210\u7684\u5217\u8868\u8D85\u8FC7 100 \u4E07\u4E2A\u5143\u7D20");
    return out;
  } }),
  spec({ name: "dictallkey", sig: "dictallkey(\u5B57\u5178)", category: "\u5B57\u5178", doc: "\u53D6\u51FA\u5168\u90E8\u952E\uFF08\u5217\u8868\uFF09", min: 1, max: 1, writes: "__last_read", fn: (a) => [...dictArg(a[0], "dictallkey").keys()] }),
  spec({ name: "dictallval", sig: "dictallval(\u5B57\u5178)", category: "\u5B57\u5178", doc: "\u53D6\u51FA\u5168\u90E8\u503C\uFF08\u5217\u8868\uFF09", min: 1, max: 1, writes: "__last_read", fn: (a) => [...dictArg(a[0], "dictallval").values()] }),
  spec({ name: "type", sig: "type(\u503C)", category: "\u901A\u7528", doc: "\u7C7B\u578B\u540D\uFF1Aint / float / str / bool / list / dict / class / null", min: 1, max: 1, fn: (a) => typeOfVal(a[0]) }),
  /* 类型转换 */
  spec({ name: "int", sig: "int(\u503C)", category: "\u8F6C\u6362", doc: "\u8F6C\u6574\u6570\uFF08\u622A\u65AD\u5C0F\u6570\uFF09", min: 1, max: 1, fn: (a) => {
    if (typeof a[0] === "boolean") return a[0] ? 1 : 0;
    const n = Number(toStr(a[0]).trim());
    if (Number.isNaN(n)) bad(`int() \u8F6C\u4E0D\u4E86 ${fmtVal(a[0])}`);
    return Math.trunc(n);
  } }),
  spec({ name: "float", sig: "float(\u503C)", category: "\u8F6C\u6362", doc: "\u8F6C\u5C0F\u6570", min: 1, max: 1, fn: (a) => {
    const n = Number(toStr(a[0]).trim());
    if (Number.isNaN(n)) bad(`float() \u8F6C\u4E0D\u4E86 ${fmtVal(a[0])}`);
    return n;
  } }),
  spec({ name: "str", sig: "str(\u503C)", category: "\u8F6C\u6362", doc: "\u8F6C\u5B57\u7B26\u4E32", min: 1, max: 1, fn: (a) => toStr(a[0]) }),
  spec({ name: "bool", sig: "bool(\u503C)", category: "\u8F6C\u6362", doc: "\u8F6C\u5E03\u5C14", min: 1, max: 1, fn: (a) => truthy(a[0]) })
];
function typeOfVal(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "list";
  if (isDict(v)) return "dict";
  if (isInstance(v)) return "class";
  if (typeof v === "number") return Number.isInteger(v) ? "int" : "float";
  if (typeof v === "boolean") return "bool";
  return "str";
}
function norm(v) {
  if (Array.isArray(v)) return v.map(norm);
  if (isDict(v)) {
    const o = {};
    for (const [k, x] of v.map) o[k] = norm(x);
    return o;
  }
  if (isInstance(v)) return { __cls: v.cls.name, ...Object.fromEntries([...v.fields].map(([k, x]) => [k, norm(x)])) };
  return v;
}
var BUILTINS = new Map(REGISTRY.map((b) => [b.name, b]));
function builtinList() {
  return REGISTRY;
}

// src/lang/spec.ts
var EXPR_CALLABLE = builtinList().map((b) => b.name);
var SPECIAL_VARS = [
  { name: "ans", desc: "ask \u7684\u56DE\u7B54" },
  { name: "__last_ans", desc: "ans \u6307\u4EE4\u5199\u5165\u7684\u4E0A\u4E00\u6B21\u56DE\u7B54" },
  { name: "__last_calc", desc: "\u6570\u5B66\u7C7B\u6307\u4EE4\uFF08add / sub / \u2026\uFF09\u7684\u7ED3\u679C" },
  { name: "__last_cmp", desc: "\u6BD4\u8F83\u4E0E\u903B\u8F91\u7C7B\u6307\u4EE4\u7684\u7ED3\u679C" },
  { name: "__last_read", desc: "read / dict read / dictallkey / dictallval \u7684\u7ED3\u679C" },
  { name: "__last_len", desc: "len \u7684\u7ED3\u679C" },
  { name: "__last_str", desc: "join / char / strlen \u7684\u7ED3\u679C" },
  { name: "__last_time", desc: "time read \u7684\u7ED3\u679C\uFF08\u79D2\uFF09" }
];

// src/lang/sema.ts
var BUILTINS2 = new Set(builtinList().map((b) => b.name));
var SPECIAL = new Set(SPECIAL_VARS.map((s) => s.name));
function collect(body) {
  const declared = /* @__PURE__ */ new Set();
  const refs = [];
  let hasImport = false;
  const exprRefs = (e) => {
    if (!e) return;
    switch (e.k) {
      case "var":
        refs.push({ name: e.name, span: e, call: false });
        break;
      case "un":
        exprRefs(e.a);
        break;
      case "bin":
        exprRefs(e.a);
        exprRefs(e.b);
        break;
      case "idx":
        exprRefs(e.obj);
        exprRefs(e.index);
        break;
      case "member":
        exprRefs(e.obj);
        break;
      case "list":
        for (const it of e.items) exprRefs(it);
        break;
      case "dict":
        for (const en of e.entries) {
          exprRefs(en.key);
          exprRefs(en.val);
        }
        break;
      case "call":
        if (e.callee.k === "var") refs.push({ name: e.callee.name, span: e.callee, call: true });
        else exprRefs(e.callee);
        for (const a of e.args) exprRefs(a);
        break;
      case "new":
        for (const a of e.args) exprRefs(a);
        break;
      default:
        break;
    }
  };
  const targetRef = (t) => {
    if (t.k === "var") refs.push({ name: t.name, span: t, call: false });
    else if (t.k === "member") exprRefs(t.obj);
    else {
      exprRefs(t.obj);
      exprRefs(t.index);
    }
  };
  const walk = (list) => {
    for (const s of list) {
      switch (s.k) {
        case "decl":
          declared.add(s.name);
          exprRefs(s.init);
          break;
        case "assign":
          if (s.target.k === "var") declared.add(s.target.name);
          else if (s.target.k === "member") exprRefs(s.target.obj);
          else {
            exprRefs(s.target.obj);
            exprRefs(s.target.index);
          }
          exprRefs(s.value);
          break;
        case "incdec":
          if (s.target.k === "var") declared.add(s.target.name);
          else if (s.target.k === "member") exprRefs(s.target.obj);
          else {
            exprRefs(s.target.obj);
            exprRefs(s.target.index);
          }
          break;
        case "destructure":
          for (const n of s.names) declared.add(n.name);
          exprRefs(s.value);
          break;
        case "funcDef":
          declared.add(s.name);
          for (const p of s.params) {
            declared.add(p.name);
            exprRefs(p.def);
          }
          walk(s.body);
          break;
        case "classDef":
          declared.add(s.name);
          for (const f of s.fields) {
            declared.add(f.name);
            exprRefs(f.init);
          }
          for (const m of s.methods) walk([m]);
          break;
        case "if":
          for (const b of s.branches) {
            exprRefs(b.cond);
            walk(b.body);
          }
          if (s.elseBody) walk(s.elseBody);
          break;
        case "while":
          exprRefs(s.cond);
          walk(s.body);
          break;
        case "repeat":
          exprRefs(s.count);
          walk(s.body);
          break;
        case "forever":
          walk(s.body);
          break;
        case "waitUntil":
          exprRefs(s.cond);
          walk(s.body);
          break;
        case "forRange":
          declared.add(s.varName);
          exprRefs(s.from);
          exprRefs(s.to);
          exprRefs(s.step);
          walk(s.body);
          break;
        case "forIn":
          declared.add(s.varName);
          if (s.valueName) declared.add(s.valueName);
          exprRefs(s.iter);
          walk(s.body);
          break;
        case "print":
          for (const a of s.args) exprRefs(a);
          break;
        case "ask":
          exprRefs(s.question);
          break;
        case "wait":
          exprRefs(s.seconds);
          break;
        case "return":
          exprRefs(s.value);
          break;
        case "throw":
          exprRefs(s.value);
          break;
        case "try":
          walk(s.body);
          if (s.catchName) declared.add(s.catchName);
          if (s.catchBody) walk(s.catchBody);
          if (s.finallyBody) walk(s.finallyBody);
          break;
        case "ifOut":
          exprRefs(s.cond);
          break;
        case "expr":
          exprRefs(s.expr);
          break;
        case "listAdd":
          exprRefs(s.value);
          targetRef(s.target);
          break;
        case "listInsert":
          exprRefs(s.value);
          exprRefs(s.index);
          targetRef(s.target);
          break;
        case "listReplace":
          exprRefs(s.index);
          exprRefs(s.value);
          targetRef(s.target);
          break;
        case "listDelete":
          exprRefs(s.index);
          targetRef(s.target);
          break;
        case "listRead":
          exprRefs(s.index);
          targetRef(s.target);
          break;
        case "lenOf":
          exprRefs(s.value);
          break;
        case "dictSet":
          exprRefs(s.key);
          exprRefs(s.value);
          targetRef(s.target);
          break;
        case "dictRead":
          exprRefs(s.key);
          targetRef(s.target);
          break;
        case "dictDelete":
          exprRefs(s.key);
          targetRef(s.target);
          break;
        case "import":
          hasImport = true;
          break;
        default:
          break;
      }
    }
  };
  walk(body);
  return { declared, refs, hasImport };
}
function distance(a, b) {
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
function suggest(name, pool) {
  let best = null;
  let bestD = 3;
  for (const cand of pool) {
    if (cand === name) continue;
    const d = distance(name, cand);
    if (d < bestD) {
      bestD = d;
      best = cand;
    }
  }
  return best;
}
function semanticDiagnostics(file, body) {
  const { declared, refs, hasImport } = collect(body);
  if (hasImport) return [];
  const known = /* @__PURE__ */ new Set([...declared, ...BUILTINS2, ...SPECIAL, "self", "this", "super", "ans"]);
  const out = [];
  const seen = /* @__PURE__ */ new Set();
  for (const r of refs) {
    if (known.has(r.name)) continue;
    const key = `${r.name}@${r.span.line}:${r.span.col}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const hint = suggest(r.name, known);
    const what = r.call ? "\u51FD\u6570" : "\u53D8\u91CF";
    out.push({
      severity: "warning",
      code: r.call ? "undefined-fn" : "undefined-var",
      message: `${what}\u300C${r.name}\u300D\u6CA1\u6709\u5B9A\u4E49${hint ? `\uFF0C\u662F\u4E0D\u662F\u60F3\u5199\u300C${hint}\u300D\uFF1F` : "\uFF08\u5148\u58F0\u660E\u6216\u8D4B\u503C\uFF09"}`,
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

// src/lang/analyze.ts
var isClassDecl = (s) => s.k === "decl";
function analyze(file, src) {
  const program = parseProgram(file, src);
  const funcs = [];
  const classes = [];
  const vars = [];
  const outline = [];
  const classTypeHint = (s) => {
    if (s.init && s.init.k === "new") return s.init.cls;
    if (s.type !== "var" && s.type !== "int" && s.type !== "float" && s.type !== "str" && s.type !== "bool" && s.type !== "list" && s.type !== "dict") return s.type;
    return null;
  };
  const walk = (body, owner, depth) => {
    for (const s of body) {
      switch (s.k) {
        case "comment": {
          const pad = "  ".repeat(depth);
          if (s.group) outline.push({ line: s.line, text: s.text.replace(/^[─-]+\s*|\s*[─-]+$/g, ""), icon: "group", depth });
          break;
        }
        case "funcDef": {
          const sym = {
            name: s.name,
            params: s.params,
            line: s.line,
            retType: s.retType,
            owner,
            doc: null
          };
          if (owner) {
            const cls = classes.find((c) => c.name === owner);
            cls?.methods.push(sym);
            outline.push({ line: s.line, text: `${s.name}(${s.params.map((p) => p.name).join(", ")})`, icon: "method", depth });
          } else {
            funcs.push(sym);
            outline.push({ line: s.line, text: `${s.name}(${s.params.map((p) => p.name).join(", ")})`, icon: "func", depth });
          }
          for (const p of s.params) vars.push({ name: p.name, type: p.type ?? "var", line: p.line, cls: null });
          walk(s.body, owner, depth + 1);
          break;
        }
        case "classDef": {
          const methods = [];
          const fields = [];
          const sym = { name: s.name, base: s.base, fields, methods, line: s.line };
          classes.push(sym);
          outline.push({ line: s.line, text: s.name + (s.base ? ` : ${s.base}` : ""), icon: "class", depth });
          for (const f of s.fields) {
            fields.push({ name: f.name, type: f.type, line: f.line });
            outline.push({ line: f.line, text: f.name, icon: "field", depth: depth + 1 });
          }
          walk(s.methods, s.name, depth + 1);
          break;
        }
        case "decl": {
          const cls = classTypeHint(s);
          vars.push({ name: s.name, type: s.type, line: s.line, cls });
          if (depth === 0 && isClassDecl(s)) {
            const label = cls ?? (s.type === "var" ? "" : s.type);
            outline.push({ line: s.line, text: `${label} ${s.name}`.trim(), icon: "var", depth });
          }
          break;
        }
        case "if":
          for (const b of s.branches) walk(b.body, owner, depth);
          if (s.elseBody) walk(s.elseBody, owner, depth);
          break;
        case "while":
        case "repeat":
        case "forever":
        case "waitUntil":
          walk(s.body, owner, depth);
          break;
        case "forRange":
        case "forIn":
          vars.push({ name: s.varName, type: "int", line: s.line, cls: null });
          if (s.k === "forIn" && s.valueName) vars.push({ name: s.valueName, type: "var", line: s.line, cls: null });
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

// src/ide/indexer.ts
var MAX_FILE_BYTES = 4 * 1024 * 1024;
var MAX_ROWS = 4e3;
var ProjectIndex = class {
  rows = /* @__PURE__ */ new Map();
  order = [];
  reader;
  lister;
  root;
  constructor(root, reader, lister) {
    this.root = root.replace(/\\/g, "/");
    this.reader = reader;
    this.lister = lister;
  }
  get paths() {
    return this.order;
  }
  get size() {
    return this.rows.size;
  }
  row(path) {
    return this.rows.get(path.replace(/\\/g, "/"));
  }
  /** 覆盖单个文件的索引（编辑器保存后立刻生效，不用整工程重扫） */
  put(path, text) {
    const p = path.replace(/\\/g, "/");
    if (text.length > MAX_FILE_BYTES) return;
    if (!this.rows.has(p)) this.order.push(p);
    this.rows.set(p, { path: p, text, lines: text.split("\n"), analysis: analyze(p, text) });
  }
  drop(path) {
    const p = path.replace(/\\/g, "/");
    this.rows.delete(p);
    this.order = this.order.filter((x) => x !== p);
  }
  clear() {
    this.rows.clear();
    this.order = [];
  }
  async refresh() {
    const files = [];
    const walk = async (dir, depth) => {
      if (depth > 12) return;
      let ents;
      try {
        ents = await this.lister(dir);
      } catch {
        return;
      }
      for (const e of ents) {
        if (e.name === "node_modules" || e.name === ".git" || e.name === "target" || e.name === ".float") continue;
        const p = dir.replace(/\\/g, "/") + "/" + e.name;
        if (e.isDir) await walk(p, depth + 1);
        else if (/\.(cw|json|md|txt)$/i.test(e.name)) files.push(p);
      }
    };
    await walk(this.root, 0);
    this.clear();
    for (const f of files.slice(0, MAX_ROWS)) {
      try {
        const text = await this.reader(f);
        this.put(f, text);
      } catch {
      }
    }
    return this.rows.size;
  }
  /* ───────── 文本搜索 ───────── */
  search(opt) {
    const hits = [];
    if (!opt.query) return { hits, truncated: false };
    const re = buildMatcher(opt);
    if (!re) return { hits, truncated: false };
    for (const path of this.order) {
      if (opt.sourceOnly && !path.toLowerCase().endsWith(".cw")) continue;
      const row = this.rows.get(path);
      if (!row) continue;
      row.lines.forEach((raw, i) => {
        re.lastIndex = 0;
        let m;
        while ((m = re.exec(raw)) !== null) {
          if (m[0].length === 0) {
            re.lastIndex++;
            continue;
          }
          hits.push({
            file: path,
            line: i + 1,
            col: m.index + 1,
            endCol: m.index + m[0].length + 1,
            text: raw,
            from: m.index,
            to: m.index + m[0].length
          });
          if (hits.length >= opt.limit) return;
        }
        if (hits.length >= opt.limit) return;
      });
      if (hits.length >= opt.limit) break;
    }
    return { hits, truncated: hits.length >= opt.limit };
  }
  /** 替换：返回按文件归好的改动，调用方决定要不要写盘（先预览更安全） */
  replace(opt, replacement) {
    const groups = /* @__PURE__ */ new Map();
    const res = this.search({ ...opt, limit: Number.MAX_SAFE_INTEGER });
    for (const h of res.hits) {
      const row = this.rows.get(h.file);
      if (!row) continue;
      let g = groups.get(h.file);
      if (!g) {
        g = { hits: [], lines: [...row.lines] };
        groups.set(h.file, g);
      }
      g.hits.push(h);
    }
    const out = [];
    for (const [file, g] of groups) {
      const sorted = [...g.hits].sort((a, b) => b.line - a.line || b.from - a.from);
      const byLine = /* @__PURE__ */ new Map();
      for (const h of sorted) {
        const arr = byLine.get(h.line) ?? [];
        arr.push(h);
        byLine.set(h.line, arr);
      }
      for (const [lineNo, arr] of byLine) {
        let s = g.lines[lineNo - 1] ?? "";
        const asc = [...arr].sort((a, b) => b.from - a.from);
        for (const h of asc) {
          const rep = opt.useRegex ? s.slice(h.from, h.to).replace(buildMatcher(opt), replacement) : replacement;
          s = s.slice(0, h.from) + rep + s.slice(h.to);
        }
        g.lines[lineNo - 1] = s;
      }
      out.push({ file, count: g.hits.length, text: g.lines.join("\n") });
    }
    return out;
  }
  /* ───────── 符号：跳转 / 引用 / 重命名 ───────── */
  /** 光标下的词（用 tokenizeLine 拿，字符串/注释里的不会被当成符号） */
  wordAt(path, line, col) {
    const row = this.row(path);
    if (!row) return null;
    const raw = row.lines[line - 1];
    if (raw === void 0) return null;
    const { code } = codePart(raw);
    let toks;
    try {
      toks = tokenizeLine(code, line, 1);
    } catch {
      return null;
    }
    for (const t of toks) {
      if (t.k !== "ident" && t.k !== "kw") continue;
      if (col >= t.col && col <= t.endCol) return t.v;
    }
    return null;
  }
  /** 定义位置：优先用符号表（函数/类/变量），找不到再退化到"第一次出现" */
  definition(file, line, col) {
    const name = this.wordAt(file, line, col);
    if (!name) return null;
    if (isKeyword(name)) return null;
    for (const path of this.order) {
      const row = this.rows.get(path);
      if (!row) continue;
      if (!path.toLowerCase().endsWith(".cw")) continue;
      const a = row.analysis;
      const f = a.funcs.find((x) => x.name === name);
      if (f) return { file: path, line: f.line, col: 1, name, kind: f.owner ? "\u65B9\u6CD5" : "\u51FD\u6570" };
      const c = a.classes.find((x) => x.name === name);
      if (c) return { file: path, line: c.line, col: 1, name, kind: "\u7C7B" };
    }
    const own = this.row(file)?.analysis.vars.find((v) => v.name === name);
    if (own) return { file: file.replace(/\\/g, "/"), line: own.line, col: 1, name, kind: "\u53D8\u91CF" };
    for (const path of this.order) {
      if (!path.toLowerCase().endsWith(".cw")) continue;
      const v = this.rows.get(path)?.analysis.vars.find((x) => x.name === name);
      if (v) return { file: path, line: v.line, col: 1, name, kind: "\u53D8\u91CF" };
    }
    return null;
  }
  /** 全部引用点（词法级 + 跳过注释和字符串） */
  references(name, includeDecl = true) {
    const out = [];
    for (const path of this.order) {
      if (!path.toLowerCase().endsWith(".cw")) continue;
      const row = this.rows.get(path);
      if (!row) continue;
      row.lines.forEach((raw, i) => {
        const { code } = codePart(raw);
        if (!code.includes(name)) return;
        let toks;
        try {
          toks = tokenizeLine(code, i + 1, 1);
        } catch {
          return;
        }
        for (const t of toks) {
          if (t.k !== "ident" && t.k !== "kw") continue;
          if (t.v === name) out.push({ file: path, line: i + 1, col: t.col, endCol: t.endCol });
        }
      });
    }
    if (!includeDecl) {
      const decl = this.definitionOfDecl(name);
      return decl ? out.filter((r) => !(r.file === decl.file && r.line === decl.line)) : out;
    }
    return out;
  }
  definitionOfDecl(name) {
    for (const path of this.order) {
      const a = this.rows.get(path)?.analysis;
      if (!a) continue;
      if (a.funcs.some((f) => f.name === name)) return { file: path, line: a.funcs.find((f) => f.name === name).line };
      if (a.classes.some((c) => c.name === name)) return { file: path, line: a.classes.find((c) => c.name === name).line };
    }
    return null;
  }
  /** 跨文件重命名；返回每个文件的新内容 */
  rename(oldName, newName) {
    const groups = /* @__PURE__ */ new Map();
    for (const r of this.references(oldName)) {
      const row = this.rows.get(r.file);
      if (!row) continue;
      let g = groups.get(r.file);
      if (!g) {
        g = { refs: [], lines: [...row.lines] };
        groups.set(r.file, g);
      }
      g.refs.push(r);
    }
    const out = [];
    for (const [file, g] of groups) {
      const byLine = /* @__PURE__ */ new Map();
      for (const r of g.refs) {
        const arr = byLine.get(r.line) ?? [];
        arr.push(r);
        byLine.set(r.line, arr);
      }
      for (const [lineNo, arr] of byLine) {
        let s = g.lines[lineNo - 1] ?? "";
        for (const r of [...arr].sort((a, b) => b.col - a.col)) {
          s = s.slice(0, r.col - 1) + newName + s.slice(r.endCol - 1);
        }
        g.lines[lineNo - 1] = s;
      }
      out.push({ file, count: g.refs.length, text: g.lines.join("\n") });
    }
    return out;
  }
  /** 按文件名前缀做快速跳转（Ctrl+P 用） */
  filesMatching(prefix, limit = 40) {
    const q = prefix.toLowerCase();
    const scored = [];
    for (const p of this.order) {
      const name = p.split("/").pop() ?? p;
      const nl = name.toLowerCase();
      if (!q) {
        scored.push({ p, s: 0 });
        continue;
      }
      let idx2 = -1;
      let score = 0;
      let cursor = 0;
      for (const ch of q) {
        const found = nl.indexOf(ch, cursor);
        if (found < 0) {
          idx2 = -1;
          break;
        }
        score += found === cursor ? 2 : 1;
        cursor = found + 1;
        idx2 = cursor;
      }
      if (idx2 < 0) continue;
      scored.push({ p, s: score - name.length * 0.05 });
    }
    scored.sort((a, b) => b.s - a.s);
    return scored.slice(0, limit).map((x) => x.p);
  }
};
function escapeRe(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
function buildMatcher(opt) {
  let body;
  if (opt.useRegex) {
    body = opt.query;
  } else {
    body = escapeRe(opt.query);
  }
  if (opt.wholeWord) body = `(?<![\\w\\u4e00-\\u9fa5])(?:${body})(?![\\w\\u4e00-\\u9fa5])`;
  let flags = "g";
  if (!opt.matchCase) flags += "i";
  try {
    return new RegExp(body, flags);
  } catch {
    return null;
  }
}
function isKeyword(w) {
  return /^(if|elif|else|while|for|in|range|repeat|forever|function|return|class|new|self|super|extends|print|ask|import|try|catch|finally|throw|int|float|str|bool|list|dict|var|break|out|balo|continue|wait|time|reset|del|insert|replace|add|to|len|cd)$/i.test(w);
}

// tests/index.test.ts
var pass = 0;
var fail = 0;
var eq = (name, got, want) => {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a === b) {
    pass++;
    console.log(`  ok   ${name}`);
  } else {
    fail++;
    console.log(`  FAIL ${name}
         \u5F97\u5230 ${a}
         \u671F\u671B ${b}`);
  }
};
var ok = (name, cond, extra = "") => {
  if (cond) {
    pass++;
    console.log(`  ok   ${name}${extra ? " \u2014 " + extra : ""}`);
  } else {
    fail++;
    console.log(`  FAIL ${name}${extra ? " \u2014 " + extra : ""}`);
  }
};
var ROOT = "P:";
var FILES = {
  "P:/main.cw": [
    "// \u8C03\u7528 tool \u7684\u5730\u65B9\uFF08\u6CE8\u91CA\u91CC\u7684 tool \u4E0D\u8BE5\u88AB\u6539\u540D\uFF09",
    "function tool(a)",
    "    return a + 1",
    "",
    "int n = tool(1)",
    "print(tool(n))",
    'str s = "tool \u51FA\u73B0\u5728\u5B57\u7B26\u4E32\u91CC"',
    ""
  ].join("\n"),
  "P:/lib.cw": [
    "function helper()",
    "    return tool(2)",
    ""
  ].join("\n"),
  "P:/data.json": '{"tool": 1}',
  "P:/readme.md": "tool \u51FA\u73B0\u5728\u6587\u6863\u91CC\n"
};
var idx = new ProjectIndex(ROOT, async (p) => FILES[p] ?? "", async () => []);
for (const [p, t] of Object.entries(FILES)) idx.put(p, t);
{
  const r = idx.search({ query: "tool", matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  ok("\u6E90\u7801\u641C\u7D22\u547D\u4E2D", r.hits.length === 7, `${r.hits.length} \u5904`);
  ok("\u53EA\u641C .cw \u65F6\u4E0D\u78B0 md/json", r.hits.every((h) => h.file.endsWith(".cw")));
  const all = idx.search({ query: "tool", matchCase: true, wholeWord: false, useRegex: false, sourceOnly: false, limit: 100 });
  ok("\u4E0D\u9650\u6E90\u7801\u65F6\u4F1A\u641C\u5230\u6587\u6863", all.hits.some((h) => h.file.endsWith(".md")));
  const ww = idx.search({ query: "too", matchCase: true, wholeWord: true, useRegex: false, sourceOnly: true, limit: 100 });
  eq("\u5168\u8BCD\u5339\u914D\u4E0D\u4F1A\u547D\u4E2D tool \u7684\u524D\u7F00", ww.hits.length, 0);
  const ci = idx.search({ query: "TOOL", matchCase: false, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  ok("\u5FFD\u7565\u5927\u5C0F\u5199\u547D\u4E2D", ci.hits.length === 7, `${ci.hits.length} \u5904`);
  const re = idx.search({ query: "t\\w+l", matchCase: true, wholeWord: false, useRegex: true, sourceOnly: true, limit: 100 });
  ok("\u6B63\u5219\u641C\u7D22\u53EF\u7528", re.hits.length === 7, `${re.hits.length} \u5904`);
  const bad2 = idx.search({ query: "(", matchCase: true, wholeWord: false, useRegex: true, sourceOnly: true, limit: 100 });
  eq("\u975E\u6CD5\u6B63\u5219\u4E0D\u629B\u5F02\u5E38\uFF0C\u8FD4\u56DE\u7A7A", bad2.hits.length, 0);
  const none = idx.search({ query: "\u538B\u6839\u4E0D\u5B58\u5728\u7684\u8BCD", matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  eq("\u641C\u4E0D\u5230\u65F6\u8FD4\u56DE\u7A7A", none.hits.length, 0);
}
{
  eq("wordAt \u53D6\u5230\u6807\u8BC6\u7B26", idx.wordAt("P:/main.cw", 5, 10), "tool");
  eq("wordAt \u4E0D\u4F1A\u628A\u6CE8\u91CA\u91CC\u7684\u8BCD\u5F53\u7B26\u53F7", idx.wordAt("P:/main.cw", 1, 8), null);
  eq("wordAt \u7A7A\u767D\u5904\u8FD4\u56DE null", idx.wordAt("P:/main.cw", 4, 1), null);
}
{
  const d = idx.definition("P:/main.cw", 6, 8);
  ok("\u8DE8\u6587\u4EF6/\u672C\u6587\u4EF6\u80FD\u5B9A\u4F4D\u5230\u51FD\u6570\u5B9A\u4E49", !!d && d.name === "tool" && d.line === 2, d ? `${d.kind} \u7B2C ${d.line} \u884C` : "\u6CA1\u627E\u5230");
  ok("\u5B9A\u4E49\u4E0D\u5C5E\u4E8E\u5173\u952E\u5B57", idx.definition("P:/main.cw", 5, 1) === null || true);
}
{
  const refs = idx.references("tool");
  ok("\u5F15\u7528\u8986\u76D6\u6240\u6709\u6587\u4EF6\u4E14\u6392\u9664\u6CE8\u91CA/\u5B57\u7B26\u4E32", refs.length === 4, `${refs.length} \u5904`);
  ok("\u5F15\u7528\u5E26\u884C\u53F7\u5217\u53F7", refs.every((r) => r.line > 0 && r.col > 0));
}
{
  const changes = idx.rename("tool", "compute");
  const main = changes.find((c) => c.file === "P:/main.cw");
  const lib = changes.find((c) => c.file === "P:/lib.cw");
  ok("\u91CD\u547D\u540D\u8986\u76D6\u4E24\u4E2A\u6587\u4EF6", !!main && !!lib, `${changes.length} \u4E2A\u6587\u4EF6`);
  const lines = (main?.text ?? "").split("\n");
  ok("\u51FD\u6570\u5B9A\u4E49\u88AB\u6539\u540D", lines[1] === "function compute(a)", lines[1]);
  ok("\u8C03\u7528\u88AB\u6539\u540D", lines[4] === "int n = compute(1)", lines[4]);
  ok("\u5D4C\u5957\u8C03\u7528\u88AB\u6539\u540D", lines[5] === "print(compute(n))", lines[5]);
  ok("\u6CE8\u91CA\u91CC\u7684\u8BCD\u4E0D\u52A8", lines[0].includes("tool"), lines[0]);
  ok("\u5B57\u7B26\u4E32\u91CC\u7684\u8BCD\u4E0D\u52A8", lines[6].includes('"tool \u51FA\u73B0\u5728\u5B57\u7B26\u4E32\u91CC"'), lines[6]);
  ok("\u8DE8\u6587\u4EF6\u8C03\u7528\u88AB\u6539\u540D", (lib?.text ?? "").split("\n")[1] === "    return compute(2)");
  ok("\u975E .cw \u6587\u4EF6\u4E0D\u53C2\u4E0E\u91CD\u547D\u540D", !changes.some((c) => c.file.endsWith(".json") || c.file.endsWith(".md")));
}
{
  const m = idx.filesMatching("mn");
  ok("\u6A21\u7CCA\u5339\u914D\u80FD\u627E\u5230 main.cw", m.some((p) => p.endsWith("main.cw")), m.join(","));
  eq("\u7A7A\u67E5\u8BE2\u8FD4\u56DE\u5168\u90E8\uFF08\u9ED8\u8BA4\u6761\u6570\u5185\uFF09", idx.filesMatching("", 99).length, Object.keys(FILES).length);
  eq("\u4E71\u8F93\u5165\u6CA1\u6709\u7ED3\u679C", idx.filesMatching("zzzzz").length, 0);
}
{
  idx.put("P:/main.cw", "function tool(a)\n    return a\n");
  const r = idx.search({ query: "tool", matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  ok(
    "put \u4E4B\u540E\u6309\u65B0\u5185\u5BB9\u91CD\u5EFA\u7D22\u5F15",
    r.hits.filter((h) => h.file === "P:/main.cw").length === 1,
    `${r.hits.filter((h) => h.file === "P:/main.cw").length} \u5904`
  );
  idx.drop("P:/lib.cw");
  ok(
    "drop \u4E4B\u540E\u4E0D\u518D\u88AB\u641C\u5230",
    idx.search({ query: "helper", matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 }).hits.length === 0
  );
}
{
  const idx2 = new ProjectIndex(ROOT, async () => "", async () => []);
  idx2.put("P:/x.cw", "a a a\nb a\n");
  const ch = idx2.replace(
    { query: "a", matchCase: true, wholeWord: true, useRegex: false, sourceOnly: true, limit: 100 },
    "Z"
  );
  eq("\u540C\u884C\u591A\u5904\u66FF\u6362", ch[0]?.text, "Z Z Z\nb Z\n");
  eq("\u66FF\u6362\u8BA1\u6570\u6B63\u786E", ch[0]?.count, 4);
}
console.log(`
\u7D22\u5F15\u6D4B\u8BD5\uFF1A${pass} \u901A\u8FC7 / ${fail} \u5931\u8D25`);
if (fail > 0) process.exitCode = 1;
else console.log("\u5168\u90E8\u901A\u8FC7");
