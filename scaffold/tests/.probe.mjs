var __defProp = Object.defineProperty;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __esm = (fn, res) => function __init() {
  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
};
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};

// src/core/tauri.ts
var tauri_exports = {};
__export(tauri_exports, {
  api: () => api,
  listen: () => listen
});
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
var api;
var init_tauri = __esm({
  "src/core/tauri.ts"() {
    "use strict";
    api = {
      /* 文件 */
      fsList: (dir) => invoke("fs_list", { dir }),
      fsRead: (path) => invoke("fs_read", { path }),
      fsWrite: (path, contents) => invoke("fs_write", { path, contents }),
      fsMkdir: (path) => invoke("fs_mkdir", { path }),
      fsRename: (from, to) => invoke("fs_rename", { from, to }),
      fsDelete: (path) => invoke("fs_delete", { path }),
      /* 工程 */
      projectNew: (dir, name, files) => invoke("project_new", { args: { dir, name, files } }),
      projectExportZip: (projectDir, outPath) => invoke("project_export_zip", { projectDir, outPath }),
      projectImportZip: (zipPath, destDir) => invoke("project_import_zip", { zipPath, destDir }),
      pickFolder: (title) => invoke("pick_folder", { title }),
      pickSaveFile: (title, defaultName, ext) => invoke("pick_save_file", { title, defaultName, ext }),
      openPath: (path) => invoke("open_path", { path }),
      pickImage: (title) => invoke("pick_image", { title }),
      detectEnv: () => invoke("detect_env"),
      /* 最近工程 */
      recentGet: () => invoke("recent_get"),
      recentPush: (path, name) => invoke("recent_push", { path, name }),
      recentRemove: (path) => invoke("recent_remove", { path }),
      /* 终端（ConPTY） */
      ptyCreate: (id, cwd, rows, cols) => invoke("pty_create", { id, cwd, rows, cols }),
      ptyWrite: (id, data) => invoke("pty_write", { id, data }),
      ptyResize: (id, cols, rows) => invoke("pty_resize", { id, cols, rows }),
      ptyClose: (id) => invoke("pty_close", { id }),
      /* Git */
      gitAvailable: () => invoke("git_available"),
      gitRun: (dir, args) => invoke("git_run", { dir, args })
    };
  }
});

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
  const push = (k, v, col, endCol, num3) => {
    out.push({ k, v, line, col, endCol, num: num3 });
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
      while (j < src.length && src[j] !== quote) {
        if (src[j] === "\\" && j + 1 < src.length) {
          const e = src[j + 1];
          text += e === "n" ? "\n" : e === "t" ? "	" : e === "r" ? "\r" : e === "0" ? "\0" : e === "\\" ? "\\" : e === '"' ? '"' : e === "'" ? "'" : e;
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
    if (!text.includes("{") && !text.includes("\\{")) return { k: "str", v: text, ...this.span(t) };
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
      if (c === "\\" && text[i + 1] === "{") {
        buf += "{";
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
  text.split("\n").forEach((rawLine, idx) => {
    const expanded = rawLine.replace(/\t/g, "    ");
    const indent = expanded.length - expanded.trimStart().length;
    const trimmed = expanded.trim();
    if (trimmed === "") return;
    if (trimmed.startsWith("--")) {
      out.push({ no: idx + 1, indent, code: "", raw: trimmed, comment: null, isComment: false, skipped: true });
      return;
    }
    const { code, comment } = codePart(trimmed);
    const isComment = trimmed.startsWith("//");
    const cleaned = isComment ? "" : code.trim().replace(/\{\s*$/, "").trim();
    out.push({ no: idx + 1, indent, code: cleaned, raw: trimmed, comment, isComment, skipped: false });
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
          if (toks[1]?.v === "(") return parsePrint(ln, toks);
          break;
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
      let ok = true;
      for (; ; ) {
        if (toks[k]?.k === "ident" && TYPE_WORDS.has(toks[k].v) && toks[k + 1]?.k === "ident") k++;
        const t = toks[k];
        if (!t || t.k !== "ident") {
          ok = false;
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
      if (ok && names.length >= 2 && toks[k]?.v === "=") {
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
    const args = p.parseArgs();
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
    let idx = 0;
    while (idx < toks.length) {
      let type = null;
      if (toks[idx]?.k === "ident" && TYPE_WORDS.has(toks[idx].v) && toks[idx + 1]?.k === "ident") {
        type = toks[idx].v;
        idx++;
      }
      const nameTok = toks[idx];
      if (!nameTok || nameTok.k !== "ident") throw new ParseError("\u51FD\u6570\u53C2\u6570\u8981\u5199\u53C2\u6570\u540D", nameTok ?? toks[0], "param");
      idx++;
      let def = null;
      if (toks[idx]?.v === "=") {
        idx++;
        const sub = [];
        let depth = 0;
        while (idx < toks.length) {
          const t = toks[idx];
          if (t.v === "(" || t.v === "[" || t.v === "{") depth++;
          if (t.v === ")" || t.v === "]" || t.v === "}") depth--;
          if (depth === 0 && t.v === ",") break;
          sub.push(t);
          idx++;
        }
        if (!sub.length) throw new ParseError(`\u53C2\u6570 ${nameTok.v} \u7684\u9ED8\u8BA4\u503C\u4E3A\u7A7A`, nameTok, "param-default");
        def = exprOf(sub);
      }
      out.push({ name: nameTok.v, type, def, line: nameTok.line, col: nameTok.col });
      if (toks[idx]?.v === ",") {
        idx++;
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
var dict = (entries) => ({
  kind: "dict",
  map: new Map(entries ?? [])
});
var instance = (cls) => ({ kind: "instance", cls, fields: /* @__PURE__ */ new Map() });
var isDict = (v) => typeof v === "object" && v !== null && v.kind === "dict";
var isInstance = (v) => typeof v === "object" && v !== null && v.kind === "instance";
var isList = (v) => Array.isArray(v);
function typeName(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "list";
  if (isDict(v)) return "dict";
  if (isInstance(v) || isCInstLike(v)) return "class";
  if (typeof v === "number") return Number.isInteger(v) ? "int" : "float";
  if (typeof v === "boolean") return "bool";
  return "str";
}
function fmtInner(v) {
  if (typeof v === "string") return JSON.stringify(v);
  return fmtVal(v);
}
function fmtVal(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "[" + v.map(fmtInner).join(", ") + "]";
  if (isDict(v)) {
    const parts = [];
    for (const [k, x] of v.map) parts.push(`${k}: ${fmtInner(x)}`);
    return "{" + parts.join(", ") + "}";
  }
  if (isInstance(v)) {
    const parts = [];
    for (const [k, x] of v.fields) parts.push(`${k}: ${fmtInner(x)}`);
    return `${v.cls.name}{` + parts.join(", ") + "}";
  }
  if (isCInstLike(v)) {
    const parts = [];
    for (const [k, x] of Object.entries(v.f)) parts.push(`${k}: ${fmtInner(x)}`);
    return `${v.cls.name}{` + parts.join(", ") + "}";
  }
  if (typeof v === "boolean") return v ? "true" : "false";
  if (typeof v === "number") {
    if (!Number.isFinite(v)) return String(v);
    return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(10)));
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
function deepEq(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((x, i) => deepEq(x, b[i]));
  }
  if (isDict(a) && isDict(b)) {
    if (a.map.size !== b.map.size) return false;
    for (const [k, v] of a.map) {
      if (!b.map.has(k) || !deepEq(v, b.map.get(k))) return false;
    }
    return true;
  }
  if (isInstance(a) && isInstance(b)) return a === b;
  return false;
}
function toNum(v) {
  if (typeof v === "number") return v;
  if (typeof v === "boolean") return v ? 1 : 0;
  if (typeof v === "string" && v.trim() !== "" && !Number.isNaN(Number(v))) return Number(v);
  return null;
}
function cloneVal(v) {
  if (Array.isArray(v)) return v.map(cloneVal);
  if (isDict(v)) {
    const d = dict();
    for (const [k, x] of v.map) d.map.set(k, cloneVal(x));
    return d;
  }
  return v;
}
function findMethod(cls, name) {
  let c = cls;
  while (c) {
    const fn = c.methods.get(name);
    if (fn) return { fn, owner: c };
    c = c.base;
  }
  return null;
}
function collectFields(cls) {
  const chain = [];
  let c = cls;
  while (c) {
    chain.unshift(c);
    c = c.base;
  }
  const out = /* @__PURE__ */ new Map();
  for (const cd of chain) for (const [k, v] of cd.fields) out.set(k, v);
  return [...out.entries()].map(([name, f]) => ({ name, init: f.init, type: f.type }));
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
function getBuiltin(name) {
  return BUILTINS.get(name);
}
function builtinList() {
  return REGISTRY;
}
var emptyDict = () => dict();

// src/lang/methods.ts
var NOT = { handled: false };
var MethodError = class extends Error {
};
var bad2 = (msg) => {
  throw new MethodError(msg);
};
var num2 = (v, who) => {
  const n = toNum(v);
  if (n === null) bad2(`${who} \u9700\u8981\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(v)}`);
  return n;
};
var str = (v) => toStr(v);
function callBuiltinMethod(obj, name, args) {
  if (typeof obj === "string") return strMethod(obj, name, args);
  if (isList(obj)) return listMethod(obj, name, args);
  if (isDict(obj)) return dictMethod(obj, name, args);
  return NOT;
}
function strMethod(s, name, a) {
  switch (name) {
    case "upper":
      return { handled: true, value: s.toUpperCase() };
    case "lower":
      return { handled: true, value: s.toLowerCase() };
    case "trim":
      return { handled: true, value: s.trim() };
    case "split":
      return { handled: true, value: s.split(a.length ? str(a[0]) : "") };
    case "replace": {
      const from = str(a[0] ?? "");
      const to = str(a[1] ?? "");
      return { handled: true, value: s.split(from).join(to) };
    }
    case "find":
    case "index_of":
      return { handled: true, value: s.indexOf(str(a[0] ?? "")) };
    case "contains":
    case "includes":
      return { handled: true, value: s.includes(str(a[0] ?? "")) };
    case "starts_with":
      return { handled: true, value: s.startsWith(str(a[0] ?? "")) };
    case "ends_with":
      return { handled: true, value: s.endsWith(str(a[0] ?? "")) };
    case "substr":
    case "substring": {
      const start = Math.trunc(num2(a[0] ?? 0, "substr"));
      const len = a.length >= 2 ? Math.trunc(num2(a[1], "substr")) : s.length;
      return { handled: true, value: s.slice(start, start + len) };
    }
    case "repeat":
      return { handled: true, value: s.repeat(Math.max(0, Math.trunc(num2(a[0] ?? 1, "repeat")))) };
    case "chars":
      return { handled: true, value: s.split("") };
    case "to_int":
      return { handled: true, value: Math.trunc(num2(s.trim() === "" ? 0 : s, "to_int")) };
    case "to_float":
      return { handled: true, value: num2(s.trim() === "" ? 0 : s, "to_float") };
    case "len":
      return { handled: true, value: s.length };
    default:
      return NOT;
  }
}
function listMethod(xs, name, a) {
  switch (name) {
    case "push":
    case "add":
      xs.push(a[0] ?? null);
      return { handled: true, value: xs.length };
    case "pop":
      return { handled: true, value: xs.length ? xs.pop() : null };
    case "insert": {
      const i = Math.trunc(num2(a[0] ?? 0, "insert"));
      const at = Math.max(0, Math.min(xs.length, i));
      xs.splice(at, 0, a[1] ?? null);
      return { handled: true, value: xs.length };
    }
    case "remove": {
      const i = Math.trunc(num2(a[0] ?? 0, "remove"));
      if (i < 0 || i >= xs.length) bad2(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${xs.length}\uFF09`);
      return { handled: true, value: xs.splice(i, 1)[0] ?? null };
    }
    case "index_of":
      return { handled: true, value: xs.findIndex((x) => JSON.stringify(x) === JSON.stringify(a[0] ?? null)) };
    case "contains":
      return { handled: true, value: xs.some((x) => JSON.stringify(x) === JSON.stringify(a[0] ?? null)) };
    case "join":
      return { handled: true, value: xs.map((x) => str(x)).join(a.length ? str(a[0]) : "") };
    case "reverse":
      xs.reverse();
      return { handled: true, value: xs };
    case "sort": {
      const nums = xs.every((x) => typeof x === "number");
      xs.sort((p, q) => nums ? p - q : str(p).localeCompare(str(q), "zh-Hans-CN"));
      return { handled: true, value: xs };
    }
    case "slice": {
      const from = Math.trunc(num2(a[0] ?? 0, "slice"));
      const to = a.length >= 2 ? Math.trunc(num2(a[1], "slice")) : xs.length;
      return { handled: true, value: xs.slice(from, to) };
    }
    case "sum":
      return { handled: true, value: xs.reduce((n, x) => n + num2(x, "sum"), 0) };
    case "clear":
      xs.length = 0;
      return { handled: true, value: 0 };
    case "len":
      return { handled: true, value: xs.length };
    default:
      return NOT;
  }
}
function dictMethod(d, name, a) {
  switch (name) {
    case "keys":
      return { handled: true, value: [...d.map.keys()] };
    case "values":
      return { handled: true, value: [...d.map.values()] };
    case "items":
      return { handled: true, value: [...d.map].map(([k, v]) => [k, v]) };
    case "has":
      return { handled: true, value: d.map.has(str(a[0] ?? "")) };
    case "remove": {
      const k = str(a[0] ?? "");
      const had = d.map.get(k) ?? null;
      d.map.delete(k);
      return { handled: true, value: had };
    }
    case "get":
      return { handled: true, value: d.map.has(str(a[0] ?? "")) ? d.map.get(str(a[0] ?? "")) : a[1] ?? null };
    case "set": {
      d.map.set(str(a[0] ?? ""), a[1] ?? null);
      return { handled: true, value: dict() };
    }
    case "len":
      return { handled: true, value: d.map.size };
    default:
      return NOT;
  }
}

// src/lang/codegen.ts
var isCInst = (v) => typeof v === "object" && v !== null && v.__cinst === 1;
var RuntimeError = class extends Error {
  constructor(message, line, frames = []) {
    super(message);
    this.line = line;
    this.frames = frames;
    this.name = "RuntimeError";
  }
};
var StopSignal = class extends Error {
  constructor() {
    super("stop");
    this.name = "StopSignal";
  }
};
var CompiledRuntime = class {
  v = {};
  /** 顶层函数 */
  f = {};
  /** 方法（按 类_方法 命名，避免和函数重名） */
  mf = {};
  classes = {};
  self = null;
  ln = 0;
  file = "";
  steps = 0;
  t0 = typeof performance !== "undefined" ? performance.now() : 0;
  out = () => {
  };
  stack = [];
  push(name) {
    this.stack.push(name);
  }
  pop() {
    this.stack.pop();
  }
  frames() {
    return [...this.stack].reverse();
  }
  /** 变量读取：未定义就报错，而不是静默 undefined */
  g(name) {
    if (!(name in this.v)) throw new RuntimeError(`\u53D8\u91CF\u300C${name}\u300D\u6CA1\u6709\u5B9A\u4E49\uFF08\u5148\u58F0\u660E\u6216\u8D4B\u503C\uFF09`, this.ln);
    return this.v[name];
  }
  tick() {
    if (++this.steps > 2e7) {
      throw new RuntimeError("\u6267\u884C\u6B65\u6570\u8D85\u8FC7 2000 \u4E07\uFF0C\u811A\u672C\u53EF\u80FD\u9677\u5165\u6B7B\u5FAA\u73AF\uFF08\u7528 out / break / stop \u7ED3\u675F\uFF09", this.ln);
    }
  }
  answerOf(r) {
    if (r && typeof r === "object" && "answer" in r) {
      const a = r.answer;
      return a == null ? "" : String(a);
    }
    return "";
  }
  print(parts) {
    this.out(parts.map((p) => toStr(p)).join(" "));
  }
  timeRead() {
    return (performance.now() - this.t0) / 1e3;
  }
  timeReset() {
    this.t0 = performance.now();
  }
  stop() {
    throw new StopSignal();
  }
  callFn(name, args) {
    const fn = this.f[name];
    if (!fn) throw new RuntimeError(`\u51FD\u6570\u300C${name}\u300D\u6CA1\u6709\u5B9A\u4E49`, this.ln);
    return fn(this, null, ...args);
  }
  /** catch 里拿到的错误对象：e.message / e.line / e.file */
  errObj(e) {
    const d = dict();
    if (e instanceof RuntimeError) {
      d.map.set("message", e.message);
      d.map.set("line", e.line || this.ln);
      d.map.set("file", this.file);
    } else {
      d.map.set("message", e instanceof Error ? e.message : String(e));
      d.map.set("line", this.ln);
      d.map.set("file", this.file);
    }
    return d;
  }
  throwVal(v) {
    const msg = isDict(v) && v.map.has("message") ? toStr(v.map.get("message")) : toStr(v);
    throw new RuntimeError(msg, this.ln);
  }
  badBreak(what) {
    throw new RuntimeError(`${what} \u53EA\u80FD\u7528\u5728\u5FAA\u73AF\u91CC`, this.ln);
  }
  /* ── 类 ── */
  makClass(name, baseName, fields, methods) {
    const base = baseName ? this.classes[baseName] ?? null : null;
    if (baseName && !base) throw new RuntimeError(`\u7236\u7C7B\u300C${baseName}\u300D\u8FD8\u6CA1\u6709\u5B9A\u4E49\uFF08\u5148\u5199\u7236\u7C7B\uFF0C\u518D\u5199\u5B50\u7C7B\uFF09`, this.ln);
    const m = /* @__PURE__ */ new Map();
    for (const [k, v] of Object.entries(methods)) m.set(k, v);
    return { name, base, fields, methods: m };
  }
  newInst(name, args) {
    const cls = this.classes[name];
    if (!cls) throw new RuntimeError(`\u7C7B\u300C${name}\u300D\u6CA1\u6709\u5B9A\u4E49`, this.ln);
    const inst = { __cinst: 1, cls, f: {} };
    const chain = [];
    let c = cls;
    while (c) {
      chain.unshift(c);
      c = c.base;
    }
    for (const cd of chain) {
      for (const fd of cd.fields) {
        inst.f[fd.name] = fd.init ? fd.init(this) : defaultFor(fd.type);
      }
    }
    const init = this.findMethod(cls, "init") ?? this.findMethod(cls, "__init__");
    if (init) init(this, inst, ...args);
    else if (args.length) throw new RuntimeError(`\u7C7B ${name} \u6CA1\u6709 init \u65B9\u6CD5\uFF0C\u4E0D\u80FD\u63A5\u6536\u53C2\u6570`, this.ln);
    return inst;
  }
  findMethod(cls, name) {
    let c = cls;
    while (c) {
      const fn = c.methods.get(name);
      if (fn) return fn;
      c = c.base;
    }
    return null;
  }
  /** super(args)：调用父类 init */
  callSuper(className, args) {
    const cls = this.classes[className];
    if (!cls || !cls.base) throw new RuntimeError("super(\u2026) \u53EA\u80FD\u7528\u5728\u6709\u7236\u7C7B\u7684\u7C7B\u91CC", this.ln);
    const init = this.findMethod(cls.base, "init") ?? this.findMethod(cls.base, "__init__");
    if (!init) throw new RuntimeError(`\u7236\u7C7B ${cls.base.name} \u6CA1\u6709 init\uFF0C\u65E0\u9700 super(\u2026)`, this.ln);
    return init(this, this.self, ...args);
  }
};
function defaultFor(type) {
  switch (type) {
    case "int":
    case "float":
      return 0;
    case "str":
      return "";
    case "bool":
      return false;
    case "list":
      return [];
    case "dict":
      return dict();
    default:
      return null;
  }
}
var O = {
  t: (v) => truthy(v),
  num(v, who = "\u8FD0\u7B97") {
    const n = toNum(v);
    if (n === null) throw new RuntimeError(`${who}\u9700\u8981\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(v)}`, 0);
    return n;
  },
  add(a, b) {
    if (typeof a === "number" && typeof b === "number") return a + b;
    if (typeof a === "string" && typeof b === "string") return a + b;
    if (typeof a === "string" || typeof b === "string") return toStr(a) + toStr(b);
    if (isList(a) && isList(b)) return [...a, ...b];
    return O.num(a, "\u300C+\u300D") + O.num(b, "\u300C+\u300D");
  },
  sub(a, b) {
    if (typeof a === "number" && typeof b === "number") return a - b;
    return O.num(a, "\u300C-\u300D") - O.num(b, "\u300C-\u300D");
  },
  mul(a, b) {
    if (typeof a === "number" && typeof b === "number") return a * b;
    if (typeof a === "string" && typeof b === "number") return a.repeat(Math.max(0, Math.trunc(b)));
    return O.num(a, "\u300C*\u300D") * O.num(b, "\u300C*\u300D");
  },
  div(a, b) {
    if (typeof a === "number" && typeof b === "number") {
      if (b === 0) throw new RuntimeError("\u9664\u6570\u4E0D\u80FD\u4E3A 0", 0);
      return a / b;
    }
    const d = O.num(b, "\u300C/\u300D");
    if (d === 0) throw new RuntimeError("\u9664\u6570\u4E0D\u80FD\u4E3A 0", 0);
    return O.num(a, "\u300C/\u300D") / d;
  },
  mod(a, b) {
    if (typeof a === "number" && typeof b === "number") {
      if (b === 0) throw new RuntimeError("\u53D6\u6A21\u7684\u9664\u6570\u4E0D\u80FD\u4E3A 0", 0);
      return a % b;
    }
    const d = O.num(b, "\u300C%\u300D");
    if (d === 0) throw new RuntimeError("\u53D6\u6A21\u7684\u9664\u6570\u4E0D\u80FD\u4E3A 0", 0);
    return O.num(a, "\u300C%\u300D") % d;
  },
  pow(a, b) {
    if (typeof a === "number" && typeof b === "number") return Math.pow(a, b);
    return Math.pow(O.num(a, "\u300C**\u300D"), O.num(b, "\u300C**\u300D"));
  },
  neg(a) {
    return typeof a === "number" ? -a : -O.num(a, "\u4E00\u5143 -");
  },
  bitnot(a) {
    return ~Math.trunc(O.num(a, "~"));
  },
  band(a, b) {
    return Math.trunc(O.num(a, "&")) & Math.trunc(O.num(b, "&"));
  },
  bor(a, b) {
    return Math.trunc(O.num(a, "|")) | Math.trunc(O.num(b, "|"));
  },
  bxor(a, b) {
    return Math.trunc(O.num(a, "^")) ^ Math.trunc(O.num(b, "^"));
  },
  shl(a, b) {
    return Math.trunc(O.num(a, "<<")) << Math.trunc(O.num(b, "<<"));
  },
  shr(a, b) {
    return Math.trunc(O.num(a, ">>")) >> Math.trunc(O.num(b, ">>"));
  },
  inc(v, delta) {
    const n = toNum(v);
    if (n === null) throw new RuntimeError(`\u300C${fmtVal(v)}\u300D\u4E0D\u662F\u6570\u5B57\uFF0C\u4E0D\u80FD ${delta > 0 ? "++" : "--"}`, 0);
    return n + delta;
  },
  lt(a, b) {
    if (typeof a === "number" && typeof b === "number") return a < b;
    if (typeof a === "string" && typeof b === "string") return a < b;
    const x = toNum(a);
    const y = toNum(b);
    if (x === null || y === null) throw new RuntimeError(`\u6BD4\u8F83\u5927\u5C0F\u9700\u8981\u4E24\u4E2A\u6570\u5B57\u6216\u4E24\u4E2A\u5B57\u7B26\u4E32\uFF0C\u5F97\u5230 ${fmtVal(a)} \u4E0E ${fmtVal(b)}`, 0);
    return x < y;
  },
  eq(a, b) {
    if (typeof a === "number" && typeof b === "number") return a === b;
    if (typeof a === "string" && typeof b === "string") return a === b;
    if (typeof a === "boolean" && typeof b === "boolean") return a === b;
    return deepEq(a, b);
  },
  eq3(a, b) {
    return typeof a === typeof b && deepEq(a, b);
  },
  comp(op, a, b) {
    switch (op) {
      case "+=":
        return O.add(a, b);
      case "-=":
        return O.sub(a, b);
      case "*=":
        return O.mul(a, b);
      case "/=":
        return O.div(a, b);
      case "%=":
        return O.mod(a, b);
      case "**=":
        return O.pow(a, b);
      default:
        return b;
    }
  },
  get(obj, idx) {
    if (Array.isArray(obj)) {
      const n = toNum(idx);
      if (n === null) throw new RuntimeError(`\u5217\u8868\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(idx)}`, 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new RuntimeError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${obj.length}\uFF09`, 0);
      return obj[real];
    }
    if (typeof obj === "string") {
      const n = toNum(idx);
      if (n === null) throw new RuntimeError(`\u5B57\u7B26\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(idx)}`, 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new RuntimeError(`\u5B57\u7B26\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${obj.length}\uFF09`, 0);
      return obj[real];
    }
    if (isDict(obj)) {
      const k = toStr(idx);
      if (!obj.map.has(k)) throw new RuntimeError(`\u5B57\u5178\u91CC\u6CA1\u6709\u952E\u300C${k}\u300D`, 0);
      return obj.map.get(k);
    }
    if (isCInst(obj)) {
      const k = toStr(idx);
      if (k in obj.f) return obj.f[k];
      throw new RuntimeError(`${obj.cls.name} \u6CA1\u6709\u5B57\u6BB5\u300C${k}\u300D`, 0);
    }
    throw new RuntimeError(`\u8FD9\u4E2A\u503C\u4E0D\u80FD\u7528\u4E0B\u6807\u8BBF\u95EE\uFF1A${fmtVal(obj)}`, 0);
  },
  mem(obj, name) {
    if (isCInst(obj)) {
      if (name in obj.f) return obj.f[name];
      throw new RuntimeError(`${obj.cls.name} \u6CA1\u6709\u5B57\u6BB5\u300C${name}\u300D`, 0);
    }
    if (isDict(obj)) {
      if (!obj.map.has(name)) throw new RuntimeError(`\u5B57\u5178\u91CC\u6CA1\u6709\u952E\u300C${name}\u300D`, 0);
      return obj.map.get(name);
    }
    if (name === "length" && (isList(obj) || typeof obj === "string")) return obj.length;
    if (isInstance(obj)) {
      if (obj.fields.has(name)) return obj.fields.get(name);
      throw new RuntimeError(`${obj.cls.name} \u6CA1\u6709\u5B57\u6BB5\u300C${name}\u300D`, 0);
    }
    throw new RuntimeError(`\u8FD9\u4E2A\u503C\u6CA1\u6709\u6210\u5458\u300C${name}\u300D\uFF1A${fmtVal(obj)}`, 0);
  },
  smem(obj, name, val) {
    if (isCInst(obj)) {
      obj.f[name] = val;
      return;
    }
    if (isDict(obj)) {
      obj.map.set(name, val);
      return;
    }
    if (isInstance(obj)) {
      obj.fields.set(name, val);
      return;
    }
    throw new RuntimeError(`\u4E0D\u80FD\u7ED9 ${fmtVal(obj)} \u7684\u6210\u5458\u8D4B\u503C`, 0);
  },
  sidx(obj, idx, val) {
    if (Array.isArray(obj)) {
      const n = toNum(idx);
      if (n === null) throw new RuntimeError("\u5217\u8868\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", 0);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new RuntimeError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${obj.length}\uFF09`, 0);
      obj[real] = val;
      return;
    }
    if (isDict(obj)) {
      obj.map.set(toStr(idx), val);
      return;
    }
    if (isCInst(obj)) {
      obj.f[toStr(idx)] = val;
      return;
    }
    throw new RuntimeError(`\u8FD9\u4E2A\u503C\u4E0D\u80FD\u6309\u4E0B\u6807\u8D4B\u503C\uFF1A${fmtVal(obj)}`, 0);
  },
  listAdd(target, val) {
    if (!isList(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5217\u8868`, 0);
    target.push(cloneVal(val));
  },
  listInsert(target, at, val) {
    if (!isList(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5217\u8868`, 0);
    const n = toNum(at);
    if (n === null) throw new RuntimeError("\u63D2\u5165\u4F4D\u7F6E\u5FC5\u987B\u662F\u6570\u5B57", 0);
    const i = Math.trunc(n);
    if (i < 0 || i > target.length) throw new RuntimeError(`\u63D2\u5165\u4F4D\u7F6E ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, 0);
    target.splice(i, 0, cloneVal(val));
  },
  listReplace(target, at, val) {
    if (!isList(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5217\u8868`, 0);
    const n = toNum(at);
    if (n === null) throw new RuntimeError("\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", 0);
    const i = Math.trunc(n);
    if (i < 0 || i >= target.length) throw new RuntimeError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, 0);
    target[i] = cloneVal(val);
  },
  listDelete(target, at) {
    if (!isList(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5217\u8868`, 0);
    if (at === null) {
      target.length = 0;
      return;
    }
    const n = toNum(at);
    if (n === null) throw new RuntimeError("\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", 0);
    const i = Math.trunc(n);
    if (i < 0 || i >= target.length) throw new RuntimeError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, 0);
    target.splice(i, 1);
  },
  listRead(target, at) {
    const n = toNum(at);
    if (n === null) throw new RuntimeError("\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", 0);
    const i = Math.trunc(n);
    if (Array.isArray(target)) {
      const real = i < 0 ? target.length + i : i;
      if (real < 0 || real >= target.length) throw new RuntimeError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, 0);
      return target[real];
    }
    if (typeof target === "string") {
      const real = i < 0 ? target.length + i : i;
      if (real < 0 || real >= target.length) throw new RuntimeError(`\u5B57\u7B26\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, 0);
      return target[real];
    }
    throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5217\u8868`, 0);
  },
  lenOf(v) {
    if (isList(v)) return v.length;
    if (typeof v === "string") return v.length;
    if (isDict(v)) return v.map.size;
    if (isCInst(v)) return Object.keys(v.f).length;
    throw new RuntimeError(`len \u53EA\u63A5\u53D7\u5217\u8868 / \u5B57\u7B26\u4E32 / \u5B57\u5178\uFF0C\u5F97\u5230 ${fmtVal(v)}`, 0);
  },
  dictSet(target, key, val) {
    if (!isDict(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5B57\u5178`, 0);
    target.map.set(toStr(key), cloneVal(val));
  },
  dictRead(target, key) {
    if (!isDict(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5B57\u5178`, 0);
    const k = toStr(key);
    if (!target.map.has(k)) throw new RuntimeError(`\u5B57\u5178\u91CC\u6CA1\u6709\u952E\u300C${k}\u300D`, 0);
    return target.map.get(k);
  },
  dictDelete(target, key) {
    if (!isDict(target)) throw new RuntimeError(`\u300C${fmtVal(target)}\u300D\u4E0D\u662F\u5B57\u5178`, 0);
    target.map.delete(toStr(key));
  },
  /** 单变量 for：列表/字符串给元素，字典给键 */
  each(v) {
    if (isList(v)) return v;
    if (typeof v === "string") return v.split("");
    if (isDict(v)) return [...v.map.keys()];
    throw new RuntimeError(`for \u2026 in \u53EA\u80FD\u904D\u5386\u5217\u8868\u3001\u5B57\u7B26\u4E32\u6216\u5B57\u5178\uFF0C\u5F97\u5230 ${fmtVal(v)}`, 0);
  },
  /** 双变量 for：字典给 [键, 值] */
  pairs(v) {
    if (isDict(v)) return [...v.map];
    throw new RuntimeError("for a, b in \u2026 \u53EA\u80FD\u7528\u5728\u5B57\u5178\u4E0A", 0);
  },
  mcall(obj, name, args, rt) {
    try {
      const m = callBuiltinMethod(obj, name, args);
      if (m.handled) return m.value;
    } catch (e) {
      if (e instanceof MethodError) throw new RuntimeError(e.message, rt.ln);
      throw e;
    }
    if (isCInst(obj)) {
      const fn = rt.findMethod(obj.cls, name);
      if (!fn) throw new RuntimeError(`\u7C7B ${obj.cls.name} \u6CA1\u6709\u65B9\u6CD5\u300C${name}\u300D`, rt.ln);
      return fn(rt, obj, ...args);
    }
    if (isInstance(obj)) {
      throw new RuntimeError(`\u7C7B ${obj.cls.name} \u7684\u65B9\u6CD5\u300C${name}\u300D\u9700\u8981\u5728\u89E3\u91CA\u6A21\u5F0F\u4E0B\u8FD0\u884C`, rt.ln);
    }
    throw new RuntimeError(`\u8FD9\u4E2A\u503C\u6CA1\u6709\u65B9\u6CD5\u300C${name}\u300D\uFF1A${fmtVal(obj)}`, rt.ln);
  }
};
var CompileError = class extends Error {
};
var BUILTINS_REF = {};
for (const b of builtinList()) BUILTINS_REF[b.name] = (...args) => b.fn(args);
var VALUES_REF = { toStr, dict };
function compileProgram(program) {
  const fnNames = /* @__PURE__ */ new Set();
  const classInfo = /* @__PURE__ */ new Map();
  const scan = (body, inClass = false) => {
    for (const s of body) {
      if (s.k === "funcDef") {
        if (!inClass) fnNames.add(s.name);
        scan(s.body, inClass);
      } else if (s.k === "classDef") {
        const fields = /* @__PURE__ */ new Set();
        for (const f of s.fields) fields.add(f.name);
        const methods = /* @__PURE__ */ new Map();
        for (const m of s.methods) {
          if (m.k === "funcDef") methods.set(m.name, { params: m.params, body: m.body });
        }
        classInfo.set(s.name, { fields, methods, base: s.base });
        scan(s.methods, true);
      } else if (s.k === "import") {
        throw new CompileError("\u6709 import\uFF0C\u4EA4\u56DE\u89E3\u91CA\u6A21\u5F0F");
      } else if (s.k === "unknown") {
        throw new CompileError("\u6709\u65E0\u6CD5\u8BC6\u522B\u7684\u8BED\u53E5\uFF0C\u4EA4\u56DE\u89E3\u91CA\u6A21\u5F0F");
      } else {
        scan(nestedBlocks(s));
      }
    }
  };
  const fieldsOf = (name) => {
    const out = /* @__PURE__ */ new Set();
    let cur = name;
    const guard = /* @__PURE__ */ new Set();
    while (cur && !guard.has(cur)) {
      guard.add(cur);
      const info = classInfo.get(cur);
      if (!info) break;
      for (const f of info.fields) out.add(f);
      cur = info.base;
    }
    return out;
  };
  const methodsOf = (name) => {
    const out = /* @__PURE__ */ new Set();
    let cur = name;
    const guard = /* @__PURE__ */ new Set();
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
      locals: /* @__PURE__ */ new Set(),
      top: true,
      fields: /* @__PURE__ */ new Set(),
      className: null,
      inLoop: false
    });
    const src = `"use strict";
return function* __floatMain($) {
${body}
};`;
    const factory = new Function("B", "O", "V", src);
    const main = factory(BUILTINS_REF, O, VALUES_REF);
    return { main, functions: fnNames.size, src };
  } catch (e) {
    if (!(e instanceof CompileError)) {
      console.warn("[Float \u7F16\u8BD1\u5668] \u9000\u56DE\u89E3\u91CA\u6A21\u5F0F\uFF1A" + (e instanceof Error ? e.message : String(e)));
    }
    return null;
  }
  function genProgramBody(stmts, ctx) {
    let out = "";
    for (const s of stmts) {
      if (s.k === "funcDef") out += genFuncDef(s, ctx);
      else if (s.k === "classDef") out += genClassDef(s);
    }
    for (const s of stmts) {
      if (s.k === "funcDef" || s.k === "classDef") continue;
      out += genStmt(s, ctx);
    }
    return out;
  }
  function genBlock(stmts, ctx) {
    let out = "";
    for (const s of stmts) out += genStmt(s, ctx);
    return out;
  }
  function localDecls(stmts, seed) {
    const out = new Set(seed);
    const walk = (list) => {
      for (const s of list) {
        switch (s.k) {
          case "decl":
            out.add(s.name);
            break;
          case "funcDef":
            out.add(s.name);
            break;
          case "destructure":
            for (const n of s.names) out.add(n.name);
            break;
          case "if":
            for (const b of s.branches) walk(b.body);
            if (s.elseBody) walk(s.elseBody);
            break;
          case "while":
          case "repeat":
          case "forever":
          case "waitUntil":
            walk(s.body);
            break;
          case "forRange":
            out.add(s.varName);
            walk(s.body);
            break;
          case "forIn":
            out.add(s.varName);
            if (s.valueName) out.add(s.valueName);
            walk(s.body);
            break;
          default:
            break;
        }
      }
    };
    walk(stmts);
    return out;
  }
  function genFuncDef(s, outer) {
    const locals = localDecls(s.body, new Set(s.params.map((p) => p.name)));
    const ctx = { locals, top: false, fields: /* @__PURE__ */ new Set(), className: outer.className, inLoop: false };
    const extra = [...locals].filter((n) => !s.params.some((p) => p.name === n));
    let out = `$.f[${JSON.stringify(s.name)}] = function($, ${s.params.map((p) => p.name).join(", ")}) {
`;
    out += `$.push(${JSON.stringify(s.name)});
`;
    out += `try {
`;
    if (extra.length) out += `let ${extra.join(", ")};
`;
    for (const p of s.params) {
      if (p.def) out += `if (${p.name} === undefined) ${p.name} = ${genExpr(p.def, ctx)};
`;
    }
    out += genBlock(s.body, ctx);
    out += `
return null;
} finally { $.pop(); }
};
`;
    return out;
  }
  function genClassDef(s) {
    const fctx = { locals: /* @__PURE__ */ new Set(), top: false, fields: /* @__PURE__ */ new Set(), className: s.name, inLoop: false };
    const fields = s.fields.map((f) => {
      const init = f.init ? `function($) { return ${genExpr(f.init, fctx)}; }` : "null";
      return `{ name: ${JSON.stringify(f.name)}, type: ${JSON.stringify(f.type)}, init: ${init} }`;
    });
    let out = "";
    const methodEntries = [];
    for (const m of s.methods) {
      if (m.k !== "funcDef") continue;
      const locals = localDecls(m.body, new Set(m.params.map((p) => p.name)));
      const ctx = { locals, top: false, fields: fieldsOf(s.name), className: s.name, inLoop: false };
      const extra = [...locals].filter((n) => !m.params.some((p) => p.name === n));
      const key = `__m_${s.name}_${m.name}`;
      out += `$.mf[${JSON.stringify(key)}] = function($, self, ${m.params.map((p) => p.name).join(", ")}) {
`;
      out += `const __keepSelf = $.self, __keepLn = $.ln; $.self = self; $.push(${JSON.stringify(m.name)});
`;
      out += `try {
`;
      if (extra.length) out += `let ${extra.join(", ")};
`;
      for (const p of m.params) {
        if (p.def) out += `if (${p.name} === undefined) ${p.name} = ${genExpr(p.def, ctx)};
`;
      }
      out += genBlock(m.body, ctx);
      out += `
return null;
} finally { $.self = __keepSelf; $.ln = __keepLn; $.pop(); }
};
`;
      methodEntries.push(`${JSON.stringify(m.name)}: $.mf[${JSON.stringify(key)}]`);
    }
    out += `$.classes[${JSON.stringify(s.name)}] = $.makClass(${JSON.stringify(s.name)}, ${JSON.stringify(s.base)}, [${fields.join(", ")}], {${methodEntries.join(", ")}});
`;
    return out;
  }
  function genStmt(s, ctx) {
    const ln = `$.ln = ${s.line};
`;
    switch (s.k) {
      case "comment":
        return "";
      case "unknown":
        throw new CompileError("\u65E0\u6CD5\u8BC6\u522B\u7684\u8BED\u53E5");
      case "decl": {
        const val = s.init ? genExpr(s.init, ctx) : defaultLiteral(s.type);
        if (ctx.top || !ctx.locals.has(s.name)) return `${ln}$.v[${JSON.stringify(s.name)}] = ${val};
`;
        return `${ln}${s.name} = ${val};
`;
      }
      case "assign":
        return `${ln}${genAssign(s.target, s.op, genExpr(s.value, ctx), ctx)};
`;
      case "incdec": {
        const cur = genRead(s.target, ctx);
        const val = `O.inc(${cur}, ${s.delta})`;
        return `${ln}${genAssign(s.target, "=", val, ctx)};
`;
      }
      case "expr": {
        const e = s.expr;
        if (e.k === "call" && e.callee.k === "var") {
          const b = getBuiltin(e.callee.name);
          if (b) {
            const call = `B.${e.callee.name}(${e.args.map((a) => genExpr(a, ctx)).join(", ")})`;
            return b.writes ? `${ln}$.v[${JSON.stringify(b.writes)}] = ${call};
` : `${ln}${call};
`;
          }
        }
        if (e.k === "var" && e.name === "ans") return `${ln}$.v.__last_ans = $.v.ans;
`;
        return `${ln}${genExpr(e, ctx)};
`;
      }
      case "print":
        return `${ln}$.print([${s.args.map((a) => genExpr(a, ctx)).join(", ")}]);
`;
      case "ask": {
        const q = genExpr(s.question, ctx);
        return `${ln}$.v.ans = $.answerOf(yield { kind: 'ask', q: V.toStr(${q}) }); $.v.__last_ans = $.v.ans;
`;
      }
      case "wait":
        return `${ln}yield { kind: 'wait', seconds: O.num(${genExpr(s.seconds, ctx)}, 'wait') };
`;
      case "if": {
        let out = `${ln}`;
        let first = true;
        for (const br of s.branches) {
          const test = br.cond ? `O.t(${genExpr(br.cond, ctx)})` : "true";
          out += `${first ? "if" : "else if"} (${test}) {
${genBlock(br.body, ctx)}} `;
          first = false;
        }
        if (s.elseBody) out += `else {
${genBlock(s.elseBody, ctx)}}`;
        return out + "\n";
      }
      case "while": {
        const c2 = { ...ctx, inLoop: true };
        return `${ln}while (O.t(${genExpr(s.cond, ctx)})) {
$.tick();
${genBlock(s.body, c2)}}
`;
      }
      case "forever": {
        const c2 = { ...ctx, inLoop: true };
        return `${ln}for (;;) {
$.tick();
${genBlock(s.body, c2)}}
`;
      }
      case "repeat": {
        const c2 = { ...ctx, inLoop: true };
        return `${ln}for (let __r = 0, __rn = Math.trunc(O.num(${genExpr(s.count, ctx)}, 'repeat')); __r < __rn; __r++) {
$.tick();
${genBlock(s.body, c2)}}
`;
      }
      case "forRange": {
        const c2 = { ...ctx, inLoop: true, locals: /* @__PURE__ */ new Set([...ctx.locals, s.varName]) };
        const from = s.from ? genExpr(s.from, ctx) : "0";
        const to = genExpr(s.to, ctx);
        const step = s.step ? genExpr(s.step, ctx) : "1";
        const cmp = s.inclusive ? "(__st > 0 ? __i < __end + 1e-9 : __i > __end - 1e-9)" : "(__st > 0 ? __i < __end : __i > __end)";
        return `${ln}{
  const __st = O.num(${step}, 'for \u6B65\u957F') || 1;
  const __end = O.num(${to}, 'for \u7EC8\u70B9');
  for (let __i = O.num(${from}, 'for \u8D77\u70B9'); ${cmp}; __i += __st) {
    $.tick();
    let ${s.varName} = __i;
${genBlock(s.body, c2)}  }
}
`;
      }
      case "forIn": {
        const names = s.valueName ? [s.varName, s.valueName] : [s.varName];
        const c2 = { ...ctx, inLoop: true, locals: /* @__PURE__ */ new Set([...ctx.locals, ...names]) };
        const iter = genExpr(s.iter, ctx);
        if (s.valueName) {
          return `${ln}for (const __p of O.pairs(${iter})) {
$.tick();
let ${s.varName} = __p[0], ${s.valueName} = __p[1];
${genBlock(s.body, c2)}}
`;
        }
        return `${ln}for (const __x of O.each(${iter})) {
$.tick();
let ${s.varName} = __x;
${genBlock(s.body, c2)}}
`;
      }
      case "waitUntil": {
        const c2 = { ...ctx, inLoop: true };
        return `${ln}while (!O.t(${genExpr(s.cond, ctx)})) {
$.tick();
${genBlock(s.body, c2)}}
`;
      }
      case "break":
        return ctx.inLoop ? `${ln}break;
` : `${ln}$.badBreak('break / out');
`;
      case "continue":
        return ctx.inLoop ? `${ln}continue;
` : `${ln}$.badBreak('balo');
`;
      case "stop":
        return `${ln}$.stop();
`;
      case "ifOut":
        return ctx.inLoop ? `${ln}if (O.t(${genExpr(s.cond, ctx)})) break;
` : `${ln}$.badBreak('if out');
`;
      case "return":
        return `${ln}return ${s.value ? genExpr(s.value, ctx) : "null"};
`;
      case "throw":
        return `${ln}$.throwVal(${genExpr(s.value, ctx)});
`;
      case "try": {
        const c2 = { ...ctx };
        const body = genBlock(s.body, c2);
        const fin = s.finallyBody ? ` finally {
${genBlock(s.finallyBody, ctx)}}` : "";
        if (!s.catchBody && !s.finallyBody) return `${ln}${body}`;
        if (!s.catchBody) return `${ln}try {
${body}}${fin}
`;
        const bind = s.catchName ? `let ${s.catchName} = $.errObj(__e);
` : "";
        const cctx = s.catchName ? { ...ctx, locals: /* @__PURE__ */ new Set([...ctx.locals, s.catchName]) } : c2;
        return `${ln}try {
${body}} catch (__e) {
${bind}${genBlock(s.catchBody, cctx)}}${fin}
`;
      }
      case "destructure": {
        const tmp = `__d${s.line}_${s.col}`;
        let out = `${ln}{ const ${tmp} = ${genExpr(s.value, ctx)};
`;
        s.names.forEach((n, idx) => {
          out += `${genAssign({ k: "var", name: n.name, file: s.file, line: n.line, col: n.col, endLine: n.line, endCol: n.col }, "=", `${tmp}[${idx}]`, ctx)};
`;
        });
        return out + "}\n";
      }
      case "funcDef":
        return genFuncDef(s, ctx);
      case "classDef":
        return genClassDef(s);
      case "import":
        throw new CompileError("import \u4EA4\u56DE\u89E3\u91CA\u6A21\u5F0F");
      case "listAdd":
        return `${ln}O.listAdd(${genRead(s.target, ctx)}, ${genExpr(s.value, ctx)});
`;
      case "listInsert":
        return `${ln}O.listInsert(${genRead(s.target, ctx)}, ${genExpr(s.index, ctx)}, ${genExpr(s.value, ctx)});
`;
      case "listReplace":
        return `${ln}O.listReplace(${genRead(s.target, ctx)}, ${genExpr(s.index, ctx)}, ${genExpr(s.value, ctx)});
`;
      case "listDelete":
        return `${ln}O.listDelete(${genRead(s.target, ctx)}, ${s.index ? genExpr(s.index, ctx) : "null"});
`;
      case "listRead":
        return `${ln}$.v.__last_read = O.listRead(${genRead(s.target, ctx)}, ${genExpr(s.index, ctx)});
`;
      case "lenOf":
        return `${ln}$.v.__last_len = O.lenOf(${genExpr(s.value, ctx)});
`;
      case "dictSet":
        return `${ln}O.dictSet(${genRead(s.target, ctx)}, ${genExpr(s.key, ctx)}, ${genExpr(s.value, ctx)});
`;
      case "dictRead":
        return `${ln}$.v.__last_read = O.dictRead(${genRead(s.target, ctx)}, ${genExpr(s.key, ctx)});
`;
      case "dictDelete":
        return `${ln}O.dictDelete(${genRead(s.target, ctx)}, ${genExpr(s.key, ctx)});
`;
      case "timeRead":
        return `${ln}$.v.__last_time = $.timeRead();
`;
      case "timeReset":
        return `${ln}$.timeReset();
`;
    }
    throw new CompileError("\u672A\u77E5\u8BED\u53E5");
  }
  function genRead(t, ctx) {
    if (t.k === "var") return readVar(t.name, ctx);
    if (t.k === "member") return `O.mem(${genExpr(t.obj, ctx)}, ${JSON.stringify(t.name)})`;
    return `O.get(${genExpr(t.obj, ctx)}, ${genExpr(t.index, ctx)})`;
  }
  function readVar(name, ctx) {
    if (ctx.locals.has(name)) return name;
    if (ctx.fields.has(name)) return `O.mem($.self, ${JSON.stringify(name)})`;
    return `$.g(${JSON.stringify(name)})`;
  }
  function genAssign(t, op, valCode, ctx) {
    if (t.k === "var") {
      const cur = op === "=" ? null : readVar(t.name, ctx);
      const value = cur ? `O.comp(${JSON.stringify(op)}, ${cur}, ${valCode})` : valCode;
      if (ctx.locals.has(t.name)) return `${t.name} = ${value}`;
      if (ctx.fields.has(t.name)) return `O.smem($.self, ${JSON.stringify(t.name)}, ${value})`;
      return `($.v[${JSON.stringify(t.name)}] = ${value})`;
    }
    if (t.k === "member") {
      const obj2 = genExpr(t.obj, ctx);
      if (op === "=") return `O.smem(${obj2}, ${JSON.stringify(t.name)}, ${valCode})`;
      return `(() => { const __o = ${obj2}; O.smem(__o, ${JSON.stringify(t.name)}, O.comp(${JSON.stringify(op)}, O.mem(__o, ${JSON.stringify(t.name)}), ${valCode})); return null; })()`;
    }
    const obj = genExpr(t.obj, ctx);
    const idx = genExpr(t.index, ctx);
    if (op === "=") return `O.sidx(${obj}, ${idx}, ${valCode})`;
    return `(() => { const __o = ${obj}, __i = ${idx}; O.sidx(__o, __i, O.comp(${JSON.stringify(op)}, O.get(__o, __i), ${valCode})); return null; })()`;
  }
  function genExpr(e, ctx) {
    switch (e.k) {
      case "num":
        return JSON.stringify(e.v);
      case "str":
        return JSON.stringify(e.v);
      case "bool":
        return e.v ? "true" : "false";
      case "null":
        return "null";
      case "list":
        return `[${e.items.map((i) => genExpr(i, ctx)).join(", ")}]`;
      case "dict":
        return `V.dict([${e.entries.map((en) => `[V.toStr(${genExpr(en.key, ctx)}), ${genExpr(en.val, ctx)}]`).join(", ")}])`;
      case "var":
        return readVar(e.name, ctx);
      case "self":
        return "$.self";
      case "un":
        if (e.op === "!") return `(!O.t(${genExpr(e.a, ctx)}))`;
        if (e.op === "-") return `O.neg(${genExpr(e.a, ctx)})`;
        if (e.op === "+") return `O.num(${genExpr(e.a, ctx)}, '\u4E00\u5143 +')`;
        return `O.bitnot(${genExpr(e.a, ctx)})`;
      case "bin": {
        const a = genExpr(e.a, ctx);
        const b = genExpr(e.b, ctx);
        switch (e.op) {
          case "&&":
            return `(O.t(${a}) ? O.t(${b}) : false)`;
          case "||":
            return `(O.t(${a}) ? true : O.t(${b}))`;
          case "+":
            return `O.add(${a}, ${b})`;
          case "-":
            return `O.sub(${a}, ${b})`;
          case "*":
            return `O.mul(${a}, ${b})`;
          case "/":
            return `O.div(${a}, ${b})`;
          case "%":
            return `O.mod(${a}, ${b})`;
          case "**":
            return `O.pow(${a}, ${b})`;
          case "&":
            return `O.band(${a}, ${b})`;
          case "|":
            return `O.bor(${a}, ${b})`;
          case "^":
            return `O.bxor(${a}, ${b})`;
          case "<<":
            return `O.shl(${a}, ${b})`;
          case ">>":
            return `O.shr(${a}, ${b})`;
          case "==":
            return `O.eq(${a}, ${b})`;
          case "!=":
            return `(!O.eq(${a}, ${b}))`;
          case "===":
            return `O.eq3(${a}, ${b})`;
          case "!==":
            return `(!O.eq3(${a}, ${b}))`;
          case "<":
            return `O.lt(${a}, ${b})`;
          case ">":
            return `O.lt(${b}, ${a})`;
          case "<=":
            return `(!O.lt(${b}, ${a}))`;
          case ">=":
            return `(!O.lt(${a}, ${b}))`;
          default:
            throw new CompileError(`\u4E0D\u652F\u6301\u7684\u8FD0\u7B97\u7B26 ${e.op}`);
        }
      }
      case "idx":
        return `O.get(${genExpr(e.obj, ctx)}, ${genExpr(e.index, ctx)})`;
      case "member":
        return `O.mem(${genExpr(e.obj, ctx)}, ${JSON.stringify(e.name)})`;
      case "call":
        return genCall(e, ctx);
      case "new": {
        const args = e.args.map((a) => genExpr(a, ctx)).join(", ");
        return `$.newInst(${JSON.stringify(e.cls)}, [${args}])`;
      }
    }
  }
  function genCall(e, ctx) {
    const args = e.args.map((a) => genExpr(a, ctx)).join(", ");
    if (e.callee.k === "var") {
      const name = e.callee.name;
      if (name === "super") {
        if (!ctx.className) throw new CompileError("super \u53EA\u80FD\u5728\u7C7B\u91CC\u7528");
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
    if (e.callee.k === "member") {
      return `O.mcall(${genExpr(e.callee.obj, ctx)}, ${JSON.stringify(e.callee.name)}, [${args}], $)`;
    }
    throw new CompileError("\u8FD9\u4E2A\u8868\u8FBE\u5F0F\u4E0D\u80FD\u88AB\u8C03\u7528");
  }
}
function defaultLiteral(type) {
  switch (type) {
    case "int":
    case "float":
      return "0";
    case "str":
      return '""';
    case "bool":
      return "false";
    case "list":
      return "[]";
    case "dict":
      return "V.dict([])";
    default:
      return "null";
  }
}
function nestedBlocks(s) {
  switch (s.k) {
    case "if":
      return [...s.branches.flatMap((b) => b.body), ...s.elseBody ?? []];
    case "while":
    case "repeat":
    case "forever":
    case "waitUntil":
      return s.body;
    case "forRange":
    case "forIn":
      return s.body;
    default:
      return [];
  }
}

// src/lang/interp.ts
var FloatError = class extends Error {
  constructor(message, span, frames = []) {
    super(message);
    this.span = span;
    this.frames = frames;
    this.name = "FloatError";
  }
};
var StopSignal2 = class extends Error {
  constructor() {
    super("\u811A\u672C\u5DF2\u505C\u6B62");
    this.name = "StopSignal";
  }
};
var Env = class {
  constructor(parent, label = "local") {
    this.parent = parent;
    this.label = label;
    if (parent) this.selfObj = parent.selfObj;
  }
  vars = /* @__PURE__ */ new Map();
  selfObj = null;
  lookup(name) {
    let e = this;
    while (e) {
      if (e.vars.has(name)) return e.vars.get(name);
      e = e.parent;
    }
    if (this.selfObj && this.selfObj.fields.has(name)) return this.selfObj.fields.get(name);
    return void 0;
  }
  has(name) {
    let e = this;
    while (e) {
      if (e.vars.has(name)) return true;
      e = e.parent;
    }
    return !!(this.selfObj && this.selfObj.fields.has(name));
  }
  define(name, v) {
    this.vars.set(name, v);
  }
  assign(name, v) {
    let e = this;
    while (e) {
      if (e.vars.has(name)) {
        e.vars.set(name, v);
        return;
      }
      e = e.parent;
    }
    if (this.selfObj && this.selfObj.fields.has(name)) {
      this.selfObj.fields.set(name, v);
      return;
    }
    this.vars.set(name, v);
  }
  /** 快照：当前帧 → 全局 */
  chain() {
    const out = [];
    let e = this;
    while (e) {
      out.push({ env: e, label: e.parent ? "\u5C40\u90E8" : "\u5168\u5C40" });
      e = e.parent;
    }
    return out;
  }
};
var SPECIAL_DEFAULTS = {
  ans: "",
  __last_calc: 0,
  __last_cmp: false,
  __last_read: null,
  __last_len: 0,
  __last_str: "",
  __last_time: 0,
  __last_ans: ""
};
function defaultValue(type) {
  switch (type) {
    case "int":
    case "float":
      return 0;
    case "str":
      return "";
    case "bool":
      return false;
    case "list":
      return [];
    case "dict":
      return emptyDict();
    default:
      return null;
  }
}
var Runner = class {
  constructor(entryFile, hook, importRoot) {
    this.entryFile = entryFile;
    this.hook = hook;
    this.importRoot = importRoot;
    this.env = this.globals;
  }
  state = "idle";
  pauseReason = "debug";
  gen = null;
  mode = "run";
  stepMode = "none";
  stepFromDepth = 0;
  pendingAsk = null;
  waitSeconds = 0;
  globals = new Env(null, "global");
  env;
  depth = 0;
  steps = 0;
  time0 = 0;
  retValue = null;
  frames = [];
  funcs = /* @__PURE__ */ new Map();
  classes = /* @__PURE__ */ new Map();
  /** 编译通道的运行时（运行模式用；调试模式为 null） */
  crt = null;
  /** 当前正在执行的方法所属的类，供 super(…) 找父类实现 */
  ownerStack = [];
  bps = /* @__PURE__ */ new Set();
  programs = /* @__PURE__ */ new Map();
  imported = /* @__PURE__ */ new Set();
  lastNode = null;
  /* ── 对外接口 ── */
  setBreakpoints(file, lines) {
    const norm2 = file.replace(/\\/g, "/");
    for (const k of [...this.bps]) if (k.startsWith(norm2 + ":")) this.bps.delete(k);
    for (const l of lines) this.bps.add(`${norm2}:${l}`);
  }
  hasBreakpoints() {
    return this.bps.size > 0;
  }
  get askQuestion() {
    return this.pendingAsk;
  }
  varsSnapshot() {
    if (this.crt) {
      const rt = this.crt;
      const out2 = [];
      for (const [name, v] of Object.entries(rt.v)) {
        out2.push({ name, type: typeName(v), value: fmtVal(v) });
      }
      if (rt.self) {
        for (const [name, v] of Object.entries(rt.self.f)) {
          if (name in rt.v) continue;
          out2.push({ name: `self.${name}`, type: typeName(v), value: fmtVal(v) });
        }
      }
      out2.sort((a, b) => Number(a.name.startsWith("__last")) - Number(b.name.startsWith("__last")));
      return out2;
    }
    const seen = /* @__PURE__ */ new Set();
    const out = [];
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
    out.sort((a, b) => Number(a.name.startsWith("__last")) - Number(b.name.startsWith("__last")));
    return out;
  }
  stop() {
    if (this.state === "idle" || this.state === "done") return;
    this.hook.output({ kind: "info", text: "\u5DF2\u624B\u52A8\u505C\u6B62" });
    this.gen = null;
    this.finish(true);
  }
  resume(answer) {
    if (!this.gen || this.state !== "paused") return;
    this.setState("running");
    if (this.pendingAsk !== null) {
      this.pendingAsk = null;
      void this.pump(answer ?? "");
      return;
    }
    if (this.waitSeconds > 0) {
      const s = this.waitSeconds;
      this.waitSeconds = 0;
      window.setTimeout(() => void this.pump(), Math.round(s * 1e3));
      return;
    }
    void this.pump();
  }
  continueRun() {
    if (this.state !== "paused") return;
    if (this.pendingAsk !== null) return;
    this.stepMode = "none";
    this.resume();
  }
  setStep(mode) {
    if (this.state !== "paused") return;
    if (this.pendingAsk !== null || this.waitSeconds > 0) return;
    this.stepMode = mode;
    this.stepFromDepth = this.depth;
    this.resume();
  }
  /* ── 启动 ── */
  start(mode) {
    this.mode = mode;
    this.stepMode = mode === "debug" ? "into" : "none";
    this.globals = new Env(null, "global");
    this.env = this.globals;
    this.funcs.clear();
    this.classes.clear();
    this.programs.clear();
    this.imported.clear();
    this.steps = 0;
    this.depth = 0;
    this.retValue = null;
    this.frames = [];
    for (const [k, v] of Object.entries(SPECIAL_DEFAULTS)) this.globals.define(k, v);
    const sources = this.sources();
    const entry = this.entryFile.replace(/\\/g, "/");
    const src = sources.get(entry) ?? "";
    if (!src.trim()) {
      this.fail(new FloatError("\u5165\u53E3\u6587\u4EF6\u6CA1\u6709\u5185\u5BB9\uFF0C\u65E0\u6CD5\u8FD0\u884C"));
      return;
    }
    this.time0 = performance.now();
    let program;
    try {
      program = parseProgram(entry, src);
      this.programs.set(entry, program);
    } catch (e) {
      this.fail(e);
      return;
    }
    const errors = program.diagnostics.filter((d) => d.severity === "error");
    if (errors.length) {
      this.setState("running");
      this.hook.output({ kind: "err", text: `\u53D1\u73B0 ${errors.length} \u5904\u8BED\u6CD5\u9519\u8BEF\uFF0C\u5148\u6539\u597D\u518D\u8FD0\u884C\uFF1A` });
      for (const d of errors.slice(0, 12)) {
        this.hook.output({
          kind: "err",
          text: `  ${this.shortFile(d.span.file)} \u7B2C ${d.span.line} \u884C\u7B2C ${d.span.col} \u5217\uFF1A${d.message}`
        });
      }
      if (errors.length > 12) this.hook.output({ kind: "err", text: `  \u2026\u2026\u8FD8\u6709 ${errors.length - 12} \u5904` });
      this.finish(true);
      return;
    }
    for (const d of program.diagnostics) {
      if (d.severity === "warning") {
        this.hook.output({ kind: "info", text: `\u63D0\u793A\uFF1A${this.shortFile(d.span.file)} \u7B2C ${d.span.line} \u884C ${d.message}` });
      }
    }
    try {
      this.preregister(program.body);
    } catch (e) {
      this.fail(e);
      return;
    }
    this.preregister(program.body);
    this.setState("running");
    this.crt = null;
    this.gen = null;
    if (mode === "run") {
      const unit = compileProgram(program);
      if (unit) {
        const rt = new CompiledRuntime();
        for (const [k, v] of Object.entries(SPECIAL_DEFAULTS)) rt.v[k] = v;
        rt.file = entry;
        rt.out = (line) => this.hook.output({ kind: "out", text: line });
        this.crt = rt;
        this.gen = unit.main(rt);
        this.hook.output({ kind: "info", text: "\u8FD0\u884C\u5F00\u59CB\uFF08\u7F16\u8BD1\u6267\u884C\uFF09" });
      }
    }
    if (!this.gen) {
      this.hook.output({
        kind: "info",
        text: mode === "debug" ? "\u8C03\u8BD5\u5F00\u59CB\uFF08\u89E3\u91CA\u6267\u884C \xB7 F8 \u7EE7\u7EED \xB7 F10 \u8DF3\u8FC7 \xB7 F11 \u8FDB\u5165 \xB7 Shift+F11 \u8DF3\u51FA\uFF09" : "\u8FD0\u884C\u5F00\u59CB\uFF08\u89E3\u91CA\u6267\u884C\uFF09"
      });
      this.gen = this.execBlock(program.body, this.globals, 0);
    }
    this.lastNode = null;
    void this.pump();
  }
  /* ── 推进生成器 ── */
  async pump(answer) {
    if (!this.gen) return;
    for (; ; ) {
      let r;
      try {
        r = await this.gen.next(answer === void 0 ? {} : { answer });
      } catch (e) {
        if (e instanceof StopSignal2 || e instanceof StopSignal) {
          this.hook.output({ kind: "info", text: "\u811A\u672C\u5DF2\u6309 stop \u6307\u4EE4\u505C\u6B62" });
          this.gen = null;
          this.finish(true);
          return;
        }
        this.fail(e);
        return;
      }
      answer = void 0;
      if (r.done) {
        this.finish();
        return;
      }
      const y = r.value;
      if (y.kind === "ask") {
        this.pauseReason = "ask";
        this.pendingAsk = y.q ?? y.question ?? "";
        this.setState("paused");
        this.hook.onPaused(this.lastNode, this.varsSnapshot());
        return;
      }
      if (y.kind === "wait") {
        this.pauseReason = "wait";
        this.waitSeconds = Math.max(0, y.seconds);
        this.setState("paused");
        this.hook.onPaused(null, this.varsSnapshot());
        return;
      }
      this.steps++;
      if (this.steps > 2e6) {
        this.fail(new FloatError("\u6267\u884C\u6B65\u6570\u8D85\u8FC7 200 \u4E07\uFF0C\u811A\u672C\u53EF\u80FD\u9677\u5165\u6B7B\u5FAA\u73AF\uFF08\u7528 out / break / stop \u7ED3\u675F\uFF09"));
        return;
      }
      this.lastNode = y.node;
      this.depth = y.depth;
      if (this.mode === "debug" && this.shouldPause(y)) {
        this.pauseReason = "debug";
        this.setState("paused");
        this.hook.onPaused(y.node, this.varsSnapshot());
        return;
      }
    }
  }
  shouldPause(y) {
    const key = `${y.node.file.replace(/\\/g, "/")}:${y.node.line}`;
    if (this.bps.has(key)) return true;
    if (this.stepMode === "into") return true;
    if (this.stepMode === "none") return false;
    if (this.stepMode === "over") return y.depth <= this.stepFromDepth;
    if (this.stepMode === "out") return y.depth < this.stepFromDepth;
    return false;
  }
  setState(s) {
    this.state = s;
    this.hook.onState(s);
  }
  fail(e) {
    if (e instanceof RuntimeError) {
      e = new FloatError(e.message, {
        file: this.crt?.file ?? this.entryFile,
        line: e.line || 1,
        col: 1,
        endLine: e.line || 1,
        endCol: 1
      }, e.frames);
    }
    const msg = e instanceof FloatError ? this.formatError(e) : e instanceof Error ? e.message : String(e);
    this.hook.output({ kind: "err", text: msg });
    this.gen = null;
    this.finish(true);
  }
  formatError(e) {
    const where = e.span ? `\uFF08${this.shortFile(e.span.file)} \u7B2C ${e.span.line} \u884C\uFF09` : "";
    const lines = [`\u51FA\u9519${where}\uFF1A${e.message}`];
    for (const f of e.frames) lines.push(`    ${f}`);
    return lines.join("\n");
  }
  finish(aborted = false) {
    if (this.crt) this.steps = this.crt.steps;
    if (!aborted) {
      const secs = (performance.now() - this.time0) / 1e3;
      this.hook.output({ kind: "info", text: `\u7ED3\u675F\uFF1A${this.steps} \u6B65 \xB7 ${secs.toFixed(2)} \u79D2` });
    }
    this.setState("done");
    this.hook.onPaused(null, this.varsSnapshot());
    this.hook.onDone();
  }
  shortFile(file) {
    const norm2 = file.replace(/\\/g, "/");
    return norm2.split("/").pop() ?? norm2;
  }
  sources() {
    const m = window.__floatSources;
    if (!m) {
      const empty = /* @__PURE__ */ new Map();
      window.__floatSources = empty;
      return empty;
    }
    return m;
  }
  /* ── 预注册：顶层函数与类先就位，允许"先用后定义" ── */
  preregister(body) {
    for (const s of body) {
      if (s.k === "funcDef") this.funcs.set(s.name, s);
      else if (s.k === "classDef") this.registerClass(s);
    }
  }
  registerClass(s) {
    const existing = this.classes.get(s.name);
    if (existing) return existing;
    const base = s.base ? this.classes.get(s.base) ?? null : null;
    if (s.base && !base) {
      throw new FloatError(`\u7236\u7C7B\u300C${s.base}\u300D\u8FD8\u6CA1\u6709\u5B9A\u4E49\uFF08\u5148\u5199\u7236\u7C7B\uFF0C\u518D\u5199\u5B50\u7C7B\uFF09`, s);
    }
    const fields = /* @__PURE__ */ new Map();
    for (const f of s.fields) fields.set(f.name, { init: f.init, type: f.type });
    const methods = /* @__PURE__ */ new Map();
    for (const m of s.methods) {
      if (m.k === "funcDef") methods.set(m.name, m);
    }
    const def = { name: s.name, base, fields, methods, file: s.file, line: s.line };
    this.classes.set(s.name, def);
    return def;
  }
  /* ── 语句执行 ── */
  async *execBlock(body, env, depth) {
    for (const s of body) {
      const sig = yield* this.execStmt(s, env, depth);
      if (sig !== "normal") return sig;
    }
    return "normal";
  }
  async *execStmt(s, env, depth) {
    this.env = env;
    switch (s.k) {
      case "comment":
        return "normal";
      case "unknown": {
        const hint = s.hint ? `\uFF08\u662F\u4E0D\u662F\u60F3\u5199 ${s.hint}\uFF1F\uFF09` : "";
        throw new FloatError(`\u65E0\u6CD5\u8BC6\u522B\u7684\u8BED\u53E5\u300C${s.raw}\u300D${hint}`, s);
      }
      case "funcDef":
        this.funcs.set(s.name, s);
        return "normal";
      case "classDef":
        this.registerClass(s);
        return "normal";
      case "import": {
        const program = await this.loadImport(s.target, s.file, s);
        yield { kind: "stmt", node: s, depth };
        if (!program) return "normal";
        this.preregister(program.body);
        const sig = yield* this.execBlock(program.body, this.globals, depth);
        if (sig === "stop") return sig;
        if (sig === "return") return "normal";
        return "normal";
      }
      case "break":
        yield { kind: "stmt", node: s, depth };
        return "break";
      case "continue":
        yield { kind: "stmt", node: s, depth };
        return "continue";
      case "stop":
        yield { kind: "stmt", node: s, depth };
        return "stop";
      case "return": {
        yield { kind: "stmt", node: s, depth };
        this.retValue = s.value ? yield* this.evalExpr(s.value, env) : null;
        return "return";
      }
      case "decl": {
        yield { kind: "stmt", node: s, depth };
        const v = s.init ? yield* this.evalExpr(s.init, env) : defaultValue(s.type);
        env.define(s.name, v);
        return "normal";
      }
      case "assign": {
        yield { kind: "stmt", node: s, depth };
        const value = yield* this.evalExpr(s.value, env);
        yield* this.assignTo(s.target, s.op, value, env, s);
        return "normal";
      }
      case "incdec": {
        yield { kind: "stmt", node: s, depth };
        const cur = yield* this.readTarget(s.target, env, s);
        const n = toNum(cur);
        if (n === null) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u6570\u5B57\uFF0C\u4E0D\u80FD ${s.delta > 0 ? "++" : "--"}`, s);
        yield* this.assignTo(s.target, "=", n + s.delta, env, s);
        return "normal";
      }
      case "expr": {
        yield { kind: "stmt", node: s, depth };
        yield* this.execExprStmt(s.expr, env, depth, s);
        return "normal";
      }
      case "print": {
        yield { kind: "stmt", node: s, depth };
        const parts = [];
        for (const a of s.args) parts.push(toStr(yield* this.evalExpr(a, env)));
        this.hook.output({ kind: "out", text: parts.join(" ") });
        return "normal";
      }
      case "ask": {
        yield { kind: "stmt", node: s, depth };
        const q = toStr(yield* this.evalExpr(s.question, env));
        const answer = yield { kind: "ask", question: q };
        const ans = answer?.answer ?? "";
        env.assign("ans", ans);
        this.globals.assign("ans", ans);
        this.globals.assign("__last_ans", ans);
        return "normal";
      }
      case "if": {
        yield { kind: "stmt", node: s, depth };
        for (const br of s.branches) {
          const ok = br.cond === null ? true : truthy(yield* this.evalExpr(br.cond, env));
          if (ok) return yield* this.execBlock(br.body, env, depth);
        }
        if (s.elseBody) return yield* this.execBlock(s.elseBody, env, depth);
        return "normal";
      }
      case "while": {
        yield { kind: "stmt", node: s, depth };
        return yield* this.loopWhile(s, env, depth);
      }
      case "waitUntil": {
        yield { kind: "stmt", node: s, depth };
        let guard = 0;
        for (; ; ) {
          if (truthy(yield* this.evalExpr(s.cond, env))) return "normal";
          if (++guard > 1e6) throw new FloatError("wait until \u5FAA\u73AF\u8D85\u8FC7 100 \u4E07\u6B21\uFF0C\u53EF\u80FD\u6B7B\u5FAA\u73AF", s);
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig === "break") return "normal";
          if (sig === "continue") continue;
          if (sig !== "normal") return sig;
        }
      }
      case "repeat": {
        yield { kind: "stmt", node: s, depth };
        const n = toNum(yield* this.evalExpr(s.count, env));
        if (n === null) throw new FloatError("repeat \u7684\u6B21\u6570\u5FC5\u987B\u662F\u6570\u5B57", s);
        const times = Math.trunc(n);
        if (times > 1e6) throw new FloatError("repeat \u6B21\u6570\u8D85\u8FC7 100 \u4E07\uFF0C\u53EF\u80FD\u6B7B\u5FAA\u73AF", s);
        for (let k = 0; k < times; k++) {
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig === "break") break;
          if (sig === "continue") continue;
          if (sig !== "normal") return sig;
        }
        return "normal";
      }
      case "forever": {
        yield { kind: "stmt", node: s, depth };
        let guard = 0;
        for (; ; ) {
          if (++guard > 1e6) throw new FloatError("forever \u5FAA\u73AF\u8D85\u8FC7 100 \u4E07\u6B21\uFF0C\u53EF\u80FD\u6B7B\u5FAA\u73AF", s);
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig === "break") break;
          if (sig === "continue") continue;
          if (sig !== "normal") return sig;
        }
        return "normal";
      }
      case "forRange": {
        yield { kind: "stmt", node: s, depth };
        const from = s.from ? toNum(yield* this.evalExpr(s.from, env)) : 0;
        if (from === null) throw new FloatError("for \u7684\u8D77\u70B9\u5FC5\u987B\u662F\u6570\u5B57", s);
        const to = toNum(yield* this.evalExpr(s.to, env));
        if (to === null) throw new FloatError("for \u7684\u7EC8\u70B9\u5FC5\u987B\u662F\u6570\u5B57", s);
        const step = s.step ? toNum(yield* this.evalExpr(s.step, env)) : 1;
        if (step === null || step === 0) throw new FloatError("for \u7684\u6B65\u957F\u5FC5\u987B\u662F\u975E 0 \u6570\u5B57", s);
        const loopEnv = new Env(env);
        const end = s.inclusive ? to + (step > 0 ? 1e-9 : -1e-9) : to;
        let guard = 0;
        for (let v = from; step > 0 ? v < end : v > end; v += step) {
          if (++guard > 1e6) throw new FloatError("for \u5FAA\u73AF\u8D85\u8FC7 100 \u4E07\u6B21\uFF0C\u53EF\u80FD\u6B7B\u5FAA\u73AF", s);
          loopEnv.define(s.varName, v);
          this.env = loopEnv;
          const sig = yield* this.execBlock(s.body, loopEnv, depth);
          if (sig === "break") break;
          if (sig === "continue") continue;
          if (sig !== "normal") return sig;
        }
        return "normal";
      }
      case "forIn": {
        yield { kind: "stmt", node: s, depth };
        const iter = yield* this.evalExpr(s.iter, env);
        const pairs = [];
        if (isList(iter)) {
          if (s.valueName) throw new FloatError("for a, b in \u2026 \u53EA\u80FD\u7528\u5728\u5B57\u5178\u4E0A", s);
          for (let k = 0; k < iter.length; k++) pairs.push([k, iter[k]]);
        } else if (typeof iter === "string") {
          if (s.valueName) throw new FloatError("for a, b in \u2026 \u53EA\u80FD\u7528\u5728\u5B57\u5178\u4E0A", s);
          for (let k = 0; k < iter.length; k++) pairs.push([k, iter[k]]);
        } else if (isDict(iter)) {
          for (const [k, v] of iter.map) pairs.push([k, v]);
        } else {
          throw new FloatError(`for \u2026 in \u53EA\u80FD\u904D\u5386\u5217\u8868\u3001\u5B57\u7B26\u4E32\u6216\u5B57\u5178\uFF0C\u5F97\u5230 ${fmtVal(iter)}`, s);
        }
        if (pairs.length > 1e6) throw new FloatError("\u8981\u904D\u5386\u7684\u96C6\u5408\u8D85\u8FC7 100 \u4E07\u4E2A\u5143\u7D20", s);
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
          if (sig === "break") break;
          if (sig === "continue") continue;
          if (sig !== "normal") return sig;
        }
        return "normal";
      }
      case "throw": {
        yield { kind: "stmt", node: s, depth };
        const v = yield* this.evalExpr(s.value, env);
        throw new FloatError(this.errMessageOf(v), s);
      }
      case "try": {
        yield { kind: "stmt", node: s, depth };
        try {
          const sig = yield* this.execBlock(s.body, env, depth);
          if (sig !== "normal") {
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
          return sig === "normal" ? "normal" : sig;
        }
        if (s.finallyBody) yield* this.execBlock(s.finallyBody, env, depth);
        return "normal";
      }
      case "destructure": {
        yield { kind: "stmt", node: s, depth };
        const v = yield* this.evalExpr(s.value, env);
        const arr = isList(v) ? v : typeof v === "string" ? v.split("") : null;
        if (!arr) throw new FloatError("\u89E3\u6784\u8D4B\u503C\u53F3\u8FB9\u8981\u662F\u5217\u8868", s);
        if (arr.length < s.names.length) {
          throw new FloatError(`\u53F3\u8FB9\u53EA\u6709 ${arr.length} \u4E2A\u503C\uFF0C\u5DE6\u8FB9\u8981 ${s.names.length} \u4E2A`, s);
        }
        s.names.forEach((n, idx) => env.assign(n.name, arr[idx]));
        return "normal";
      }
      case "wait": {
        const secs = toNum(yield* this.evalExpr(s.seconds, env));
        if (secs === null) throw new FloatError("wait \u9700\u8981\u79D2\u6570", s);
        yield { kind: "stmt", node: s, depth };
        yield { kind: "wait", seconds: secs };
        return "normal";
      }
      case "ifOut": {
        yield { kind: "stmt", node: s, depth };
        return truthy(yield* this.evalExpr(s.cond, env)) ? "break" : "normal";
      }
      case "listAdd": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5217\u8868`, s);
        target.push(cloneVal(yield* this.evalExpr(s.value, env)));
        return "normal";
      }
      case "listInsert": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5217\u8868`, s);
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError("\u63D2\u5165\u4F4D\u7F6E\u5FC5\u987B\u662F\u6570\u5B57", s);
        const i = Math.trunc(at);
        if (i < 0 || i > target.length) throw new FloatError(`\u63D2\u5165\u4F4D\u7F6E ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, s);
        target.splice(i, 0, cloneVal(yield* this.evalExpr(s.value, env)));
        return "normal";
      }
      case "listReplace": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5217\u8868`, s);
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError("\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", s);
        const i = Math.trunc(at);
        if (i < 0 || i >= target.length) throw new FloatError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, s);
        target[i] = cloneVal(yield* this.evalExpr(s.value, env));
        return "normal";
      }
      case "listDelete": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isList(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5217\u8868`, s);
        if (!s.index) {
          target.length = 0;
          return "normal";
        }
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError("\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", s);
        const i = Math.trunc(at);
        if (i < 0 || i >= target.length) throw new FloatError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, s);
        target.splice(i, 1);
        return "normal";
      }
      case "listRead": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        const at = toNum(yield* this.evalExpr(s.index, env));
        if (at === null) throw new FloatError("\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", s);
        const i = Math.trunc(at);
        if (isList(target)) {
          const idx = i < 0 ? target.length + i : i;
          if (idx < 0 || idx >= target.length) throw new FloatError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, s);
          this.globals.assign("__last_read", target[idx]);
          return "normal";
        }
        if (typeof target === "string") {
          const idx = i < 0 ? target.length + i : i;
          if (idx < 0 || idx >= target.length) throw new FloatError(`\u5B57\u7B26\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${target.length}\uFF09`, s);
          this.globals.assign("__last_read", target[idx]);
          return "normal";
        }
        throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5217\u8868`, s);
      }
      case "lenOf": {
        yield { kind: "stmt", node: s, depth };
        const v = yield* this.evalExpr(s.value, env);
        const len = isList(v) ? v.length : typeof v === "string" ? v.length : isDict(v) ? v.map.size : isInstance(v) ? v.fields.size : -1;
        if (len < 0) throw new FloatError(`len \u53EA\u63A5\u53D7\u5217\u8868 / \u5B57\u7B26\u4E32 / \u5B57\u5178\uFF0C\u5F97\u5230 ${fmtVal(v)}`, s);
        this.globals.assign("__last_len", len);
        return "normal";
      }
      case "dictSet": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isDict(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5B57\u5178`, s);
        const key = toStr(yield* this.evalExpr(s.key, env));
        target.map.set(key, cloneVal(yield* this.evalExpr(s.value, env)));
        return "normal";
      }
      case "dictRead": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isDict(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5B57\u5178`, s);
        const key = toStr(yield* this.evalExpr(s.key, env));
        if (!target.map.has(key)) throw new FloatError(`\u5B57\u5178\u300C${this.targetName(s.target)}\u300D\u91CC\u6CA1\u6709\u952E\u300C${key}\u300D`, s);
        this.globals.assign("__last_read", target.map.get(key));
        return "normal";
      }
      case "dictDelete": {
        yield { kind: "stmt", node: s, depth };
        const target = yield* this.evalExpr(targetExpr(s.target), env);
        if (!isDict(target)) throw new FloatError(`\u300C${this.targetName(s.target)}\u300D\u4E0D\u662F\u5B57\u5178`, s);
        target.map.delete(toStr(yield* this.evalExpr(s.key, env)));
        return "normal";
      }
      case "timeRead": {
        yield { kind: "stmt", node: s, depth };
        this.globals.assign("__last_time", (performance.now() - this.time0) / 1e3);
        return "normal";
      }
      case "timeReset": {
        yield { kind: "stmt", node: s, depth };
        this.time0 = performance.now();
        return "normal";
      }
    }
  }
  /** 把任意错误转成脚本里可读的字典：e.message / e.line / e.file */
  errorObjectOf(e) {
    const msg = e instanceof FloatError ? e.message : e instanceof Error ? e.message : String(e);
    const span = e instanceof FloatError ? e.span : void 0;
    const d = dict();
    d.map.set("message", msg);
    d.map.set("line", span?.line ?? 0);
    d.map.set("file", span?.file ?? "");
    return d;
  }
  errMessageOf(v) {
    if (isDict(v) && v.map.has("message")) return toStr(v.map.get("message"));
    return toStr(v);
  }
  async *loopWhile(s, env, depth) {
    let guard = 0;
    for (; ; ) {
      if (!truthy(yield* this.evalExpr(s.cond, env))) break;
      if (++guard > 1e6) throw new FloatError("while \u5FAA\u73AF\u8D85\u8FC7 100 \u4E07\u6B21\uFF0C\u53EF\u80FD\u6B7B\u5FAA\u73AF", s);
      const sig = yield* this.execBlock(s.body, env, depth);
      if (sig === "break") break;
      if (sig === "continue") continue;
      if (sig !== "normal") return sig;
    }
    return "normal";
  }
  /** 表达式语句：独立成行的内建调用仍然写 __last_*，兼容 CW Script 老写法 */
  async *execExprStmt(e, env, depth, s) {
    if (e.k === "var") {
      if (e.name === "ans") {
        this.globals.assign("__last_ans", env.lookup("ans") ?? "");
        return null;
      }
      const v = env.lookup(e.name);
      if (v === void 0) throw new FloatError(`\u53D8\u91CF\u300C${e.name}\u300D\u6CA1\u6709\u5B9A\u4E49`, s);
      return v;
    }
    if (e.k === "call" && e.callee.k === "var") {
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
  async *evalArgs(args, env) {
    const out = [];
    for (const a of args) out.push(yield* this.evalExpr(a, env));
    return out;
  }
  /* ── 表达式 ── */
  async *evalExpr(e, env) {
    switch (e.k) {
      case "num":
        return e.v;
      case "str":
        return e.v;
      case "bool":
        return e.v;
      case "null":
        return null;
      case "list": {
        const items = [];
        for (const it of e.items) items.push(yield* this.evalExpr(it, env));
        return items;
      }
      case "dict": {
        const d = dict();
        for (const { key, val } of e.entries) {
          d.map.set(toStr(yield* this.evalExpr(key, env)), yield* this.evalExpr(val, env));
        }
        return d;
      }
      case "var": {
        const v = env.lookup(e.name);
        if (v !== void 0) return v;
        if (this.classes.has(e.name)) throw new FloatError(`\u300C${e.name}\u300D\u662F\u7C7B\u540D\uFF0C\u8981\u521B\u5EFA\u5B9E\u4F8B\u8BF7\u5199 new ${e.name}(\u2026)`, e);
        if (this.funcs.has(e.name)) throw new FloatError(`\u300C${e.name}\u300D\u662F\u51FD\u6570\uFF0C\u8C03\u7528\u8981\u5199 ${e.name}(\u2026)`, e);
        if (env.selfObj && env.selfObj.fields.has(e.name)) return env.selfObj.fields.get(e.name);
        throw new FloatError(`\u53D8\u91CF\u300C${e.name}\u300D\u6CA1\u6709\u5B9A\u4E49\uFF08\u5148\u58F0\u660E\u6216\u8D4B\u503C\uFF09`, e);
      }
      case "self": {
        const obj = env.selfObj;
        if (!obj) throw new FloatError("self \u53EA\u80FD\u5728\u7C7B\u7684\u65B9\u6CD5\u91CC\u4F7F\u7528", e);
        return obj;
      }
      case "un": {
        const v = yield* this.evalExpr(e.a, env);
        if (e.op === "-") {
          const n = toNum(v);
          if (n === null) throw new FloatError(`\u4E00\u5143 - \u9700\u8981\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(v)}`, e);
          return -n;
        }
        if (e.op === "+") {
          const n = toNum(v);
          if (n === null) throw new FloatError(`\u4E00\u5143 + \u9700\u8981\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(v)}`, e);
          return n;
        }
        if (e.op === "!") return !truthy(v);
        if (e.op === "~") {
          const n = toNum(v);
          if (n === null) throw new FloatError("~ \u9700\u8981\u6570\u5B57", e);
          return ~n;
        }
        throw new FloatError(`\u672A\u77E5\u4E00\u5143\u8FD0\u7B97\u7B26 ${e.op}`, e);
      }
      case "bin":
        return yield* this.evalBin(e, env);
      case "idx": {
        const obj = yield* this.evalExpr(e.obj, env);
        const idx = yield* this.evalExpr(e.index, env);
        return this.indexGet(obj, idx, e);
      }
      case "member": {
        const obj = yield* this.evalExpr(e.obj, env);
        return this.memberGet(obj, e.name, e);
      }
      case "call": {
        const args = yield* this.evalArgs(e.args, env);
        if (e.callee.k === "var") {
          const name = e.callee.name;
          if (name === "super") {
            const owner = this.ownerStack[this.ownerStack.length - 1] ?? null;
            const self = env.selfObj;
            if (!owner || !owner.base || !self) throw new FloatError("super(\u2026) \u53EA\u80FD\u5728\u5B50\u7C7B\u65B9\u6CD5\u91CC\u4F7F\u7528", e);
            const init = findMethod(owner.base, "init") ?? findMethod(owner.base, "__init__");
            if (!init) throw new FloatError(`\u7236\u7C7B ${owner.base.name} \u6CA1\u6709 init\uFF0C\u65E0\u9700 super(\u2026)`, e);
            return yield* this.callUser(init.fn, args, self, e, init.owner);
          }
          const b = getBuiltin(name);
          if (b) return this.callBuiltin(name, args, e);
          if (env.selfObj) {
            const m = findMethod(env.selfObj.cls, name);
            if (m) return yield* this.callUser(m.fn, args, env.selfObj, e, m.owner);
          }
          const fn = this.funcs.get(name);
          if (fn) return yield* this.callUser(fn, args, null, e);
          const cls = this.classes.get(name);
          if (cls) return yield* this.instantiate(cls, args, e);
          throw new FloatError(`\u51FD\u6570\u300C${name}\u300D\u6CA1\u6709\u5B9A\u4E49`, e);
        }
        if (e.callee.k === "member") {
          const obj = yield* this.evalExpr(e.callee.obj, env);
          try {
            const m = callBuiltinMethod(obj, e.callee.name, args);
            if (m.handled) return m.value;
          } catch (err) {
            if (err instanceof MethodError) throw new FloatError(err.message, e);
            throw err;
          }
          if (isInstance(obj)) {
            const found = findMethod(obj.cls, e.callee.name);
            if (!found) throw new FloatError(`\u7C7B ${obj.cls.name} \u6CA1\u6709\u65B9\u6CD5\u300C${e.callee.name}\u300D`, e);
            return yield* this.callUser(found.fn, args, obj, e, found.owner);
          }
          if (isDict(obj)) {
            const v = obj.map.get(e.callee.name);
            if (v === void 0) throw new FloatError(`\u5B57\u5178\u91CC\u6CA1\u6709\u952E\u300C${e.callee.name}\u300D\uFF0C\u5B83\u4E5F\u4E0D\u662F\u53EF\u8C03\u7528\u7684\u65B9\u6CD5`, e);
            throw new FloatError(`\u5B57\u5178\u7684\u952E\u300C${e.callee.name}\u300D\u4E0D\u662F\u51FD\u6570\uFF0C\u4E0D\u80FD\u8C03\u7528`, e);
          }
          throw new FloatError(`\u8FD9\u4E2A\u503C\u6CA1\u6709\u65B9\u6CD5\u300C${e.callee.name}\u300D\uFF1A${fmtVal(obj)}`, e);
        }
        throw new FloatError("\u8FD9\u4E2A\u8868\u8FBE\u5F0F\u4E0D\u80FD\u88AB\u8C03\u7528", e);
      }
      case "new": {
        const cls = this.classes.get(e.cls);
        if (!cls) throw new FloatError(`\u7C7B\u300C${e.cls}\u300D\u6CA1\u6709\u5B9A\u4E49`, e);
        const args = yield* this.evalArgs(e.args, env);
        return yield* this.instantiate(cls, args, e);
      }
    }
  }
  async *evalBin(e, env) {
    if (e.op === "&&") {
      return truthy(yield* this.evalExpr(e.a, env)) ? truthy(yield* this.evalExpr(e.b, env)) : false;
    }
    if (e.op === "||") {
      return truthy(yield* this.evalExpr(e.a, env)) ? true : truthy(yield* this.evalExpr(e.b, env));
    }
    const a = yield* this.evalExpr(e.a, env);
    const b = yield* this.evalExpr(e.b, env);
    switch (e.op) {
      case "+":
        if (typeof a === "string" || typeof b === "string") return toStr(a) + toStr(b);
        if (isList(a) && isList(b)) return [...a, ...b];
        return this.numOr(a, "+", e) + this.numOr(b, "+", e);
      case "-":
        return this.numOr(a, "-", e) - this.numOr(b, "-", e);
      case "*":
        if (typeof a === "string" && typeof b === "number") return a.repeat(Math.max(0, Math.trunc(b)));
        return this.numOr(a, "*", e) * this.numOr(b, "*", e);
      case "/": {
        const d = this.numOr(b, "/", e);
        if (d === 0) throw new FloatError("\u9664\u6570\u4E0D\u80FD\u4E3A 0", e);
        return this.numOr(a, "/", e) / d;
      }
      case "%": {
        const d = this.numOr(b, "%", e);
        if (d === 0) throw new FloatError("\u53D6\u6A21\u7684\u9664\u6570\u4E0D\u80FD\u4E3A 0", e);
        return this.numOr(a, "%", e) % d;
      }
      case "**":
        return Math.pow(this.numOr(a, "**", e), this.numOr(b, "**", e));
      case "==":
        return deepEq(a, b);
      case "===":
        return deepEq(a, b) && typeof a === typeof b;
      case "!=":
        return !deepEq(a, b);
      case "!==":
        return !(deepEq(a, b) && typeof a === typeof b);
      case "<":
        return this.less(a, b, e);
      case ">":
        return this.less(b, a, e);
      case "<=":
        return !this.less(b, a, e);
      case ">=":
        return !this.less(a, b, e);
      case "&":
        return this.intOr(a, "&", e) & this.intOr(b, "&", e);
      case "|":
        return this.intOr(a, "|", e) | this.intOr(b, "|", e);
      case "^":
        return this.intOr(a, "^", e) ^ this.intOr(b, "^", e);
      case "<<":
        return this.intOr(a, "<<", e) << this.intOr(b, "<<", e);
      case ">>":
        return this.intOr(a, ">>", e) >> this.intOr(b, ">>", e);
    }
    throw new FloatError(`\u672A\u77E5\u8FD0\u7B97\u7B26 ${e.op}`, e);
  }
  numOr(v, op, e) {
    const n = toNum(v);
    if (n === null) throw new FloatError(`\u8FD0\u7B97\u7B26\u300C${op}\u300D\u9700\u8981\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(v)}`, spanOf(e));
    return n;
  }
  intOr(v, op, e) {
    return Math.trunc(this.numOr(v, op, e));
  }
  less(a, b, e) {
    if (typeof a === "string" && typeof b === "string") return a < b;
    const x = toNum(a);
    const y = toNum(b);
    if (x === null || y === null) throw new FloatError(`\u6BD4\u8F83\u5927\u5C0F\u9700\u8981\u4E24\u4E2A\u6570\u5B57\u6216\u4E24\u4E2A\u5B57\u7B26\u4E32\uFF0C\u5F97\u5230 ${fmtVal(a)} \u4E0E ${fmtVal(b)}`, spanOf(e));
    return x < y;
  }
  callBuiltin(name, args, e) {
    const b = getBuiltin(name);
    if (!b) throw new FloatError(`\u5185\u5EFA\u51FD\u6570\u300C${name}\u300D\u4E0D\u5B58\u5728`, spanOf(e));
    if (args.length < b.min || args.length > b.max) {
      const range = b.min === b.max ? `${b.min}` : `${b.min}~${b.max}`;
      throw new FloatError(`${b.sig} \u9700\u8981 ${range} \u4E2A\u53C2\u6570\uFF0C\u5B9E\u9645\u7ED9\u4E86 ${args.length} \u4E2A`, spanOf(e));
    }
    try {
      return b.fn(args);
    } catch (err) {
      throw new FloatError(err instanceof Error ? err.message : String(err), spanOf(e));
    }
  }
  indexGet(obj, idx, e) {
    if (isList(obj)) {
      const n = toNum(idx);
      if (n === null) throw new FloatError(`\u5217\u8868\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(idx)}`, spanOf(e));
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new FloatError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${obj.length}\uFF09`, spanOf(e));
      return obj[real];
    }
    if (typeof obj === "string") {
      const n = toNum(idx);
      if (n === null) throw new FloatError(`\u5B57\u7B26\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57\uFF0C\u5F97\u5230 ${fmtVal(idx)}`, spanOf(e));
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new FloatError(`\u5B57\u7B26\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${obj.length}\uFF09`, spanOf(e));
      return obj[real];
    }
    if (isDict(obj)) {
      const key = toStr(idx);
      if (!obj.map.has(key)) throw new FloatError(`\u5B57\u5178\u91CC\u6CA1\u6709\u952E\u300C${key}\u300D`, spanOf(e));
      return obj.map.get(key);
    }
    throw new FloatError(`\u8FD9\u4E2A\u503C\u4E0D\u80FD\u7528\u4E0B\u6807\u8BBF\u95EE\uFF1A${fmtVal(obj)}`, spanOf(e));
  }
  memberGet(obj, name, e) {
    if (isInstance(obj)) {
      if (obj.fields.has(name)) return obj.fields.get(name);
      if (findMethod(obj.cls, name)) {
        throw new FloatError(`\u300C${name}\u300D\u662F ${obj.cls.name} \u7684\u65B9\u6CD5\uFF0C\u8981\u8C03\u7528\u8BF7\u5199 ${name}(\u2026)`, spanOf(e));
      }
      throw new FloatError(`${obj.cls.name} \u6CA1\u6709\u5B57\u6BB5\u300C${name}\u300D`, spanOf(e));
    }
    if (isDict(obj)) {
      if (!obj.map.has(name)) throw new FloatError(`\u5B57\u5178\u91CC\u6CA1\u6709\u952E\u300C${name}\u300D`, spanOf(e));
      return obj.map.get(name);
    }
    if (name === "length" && (isList(obj) || typeof obj === "string")) {
      return obj.length;
    }
    throw new FloatError(`\u8FD9\u4E2A\u503C\u6CA1\u6709\u6210\u5458\u300C${name}\u300D\uFF1A${fmtVal(obj)}`, spanOf(e));
  }
  targetName(t) {
    if (t.k === "var") return t.name;
    if (t.k === "member") return `.${t.name}`;
    return "\u4E0B\u6807";
  }
  async *readTarget(t, env, s) {
    if (t.k === "var") {
      const v = env.lookup(t.name);
      if (v === void 0) throw new FloatError(`\u53D8\u91CF\u300C${t.name}\u300D\u6CA1\u6709\u5B9A\u4E49`, s);
      return v;
    }
    if (t.k === "member") {
      const obj2 = yield* this.evalExpr(t.obj, env);
      return this.memberGet(obj2, t.name, s);
    }
    const obj = yield* this.evalExpr(t.obj, env);
    const idx = yield* this.evalExpr(t.index, env);
    return this.indexGet(obj, idx, s);
  }
  async *assignTo(t, op, value, env, s) {
    let next = value;
    if (op !== "=") {
      const cur = yield* this.readTarget(t, env, s);
      next = this.applyCompound(op, cur, value, s);
    }
    if (t.k === "var") {
      env.assign(t.name, next);
      return "normal";
    }
    if (t.k === "member") {
      const obj2 = yield* this.evalExpr(t.obj, env);
      if (isInstance(obj2)) {
        obj2.fields.set(t.name, next);
        return "normal";
      }
      if (isDict(obj2)) {
        obj2.map.set(t.name, next);
        return "normal";
      }
      throw new FloatError(`\u4E0D\u80FD\u7ED9 ${fmtVal(obj2)} \u7684\u6210\u5458\u8D4B\u503C`, s);
    }
    const obj = yield* this.evalExpr(t.obj, env);
    const idx = yield* this.evalExpr(t.index, env);
    if (isList(obj)) {
      const n = toNum(idx);
      if (n === null) throw new FloatError("\u5217\u8868\u4E0B\u6807\u5FC5\u987B\u662F\u6570\u5B57", s);
      const i = Math.trunc(n);
      const real = i < 0 ? obj.length + i : i;
      if (real < 0 || real >= obj.length) throw new FloatError(`\u4E0B\u6807 ${i} \u8D85\u51FA\u8303\u56F4\uFF08\u957F\u5EA6 ${obj.length}\uFF09`, s);
      obj[real] = next;
      return "normal";
    }
    if (isDict(obj)) {
      obj.map.set(toStr(idx), next);
      return "normal";
    }
    throw new FloatError(`\u8FD9\u4E2A\u503C\u4E0D\u80FD\u6309\u4E0B\u6807\u8D4B\u503C\uFF1A${fmtVal(obj)}`, s);
  }
  applyCompound(op, cur, rhs, s) {
    switch (op) {
      case "+=":
        if (typeof cur === "string" || typeof rhs === "string") return toStr(cur) + toStr(rhs);
        if (isList(cur) && isList(rhs)) return [...cur, ...rhs];
        return this.numOr(cur, "+=", s) + this.numOr(rhs, "+=", s);
      case "-=":
        return this.numOr(cur, "-=", s) - this.numOr(rhs, "-=", s);
      case "*=":
        return this.numOr(cur, "*=", s) * this.numOr(rhs, "*=", s);
      case "/=": {
        const d = this.numOr(rhs, "/=", s);
        if (d === 0) throw new FloatError("\u9664\u6570\u4E0D\u80FD\u4E3A 0", s);
        return this.numOr(cur, "/=", s) / d;
      }
      case "%=": {
        const d = this.numOr(rhs, "%=", s);
        if (d === 0) throw new FloatError("\u53D6\u6A21\u7684\u9664\u6570\u4E0D\u80FD\u4E3A 0", s);
        return this.numOr(cur, "%=", s) % d;
      }
      case "**=":
        return Math.pow(this.numOr(cur, "**=", s), this.numOr(rhs, "**=", s));
      default:
        return rhs;
    }
  }
  /* ── 函数与类调用 ── */
  async *callUser(fn, args, self, e, owner = null) {
    if (args.length > fn.params.length) {
      throw new FloatError(`${fn.name} \u53EA\u63A5\u53D7 ${fn.params.length} \u4E2A\u53C2\u6570\uFF0C\u5B9E\u9645\u7ED9\u4E86 ${args.length} \u4E2A`, spanOf(e));
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
        throw new FloatError(`\u8C03\u7528 ${fn.name} \u7F3A\u5C11\u53C2\u6570\u300C${p.name}\u300D`, spanOf(e));
      }
    }
    const savedEnv = this.env;
    const savedRet = this.retValue;
    this.retValue = null;
    this.env = env;
    this.ownerStack.push(owner);
    const caller = this.frames.length ? this.frames[this.frames.length - 1] : "";
    this.frames.push(`${self ? `${self.cls.name}.` : ""}${fn.name}() \u5728 ${this.shortFile(fn.file)} \u7B2C ${fn.line} \u884C${caller ? `\uFF08\u7531 ${caller} \u8C03\u7528\uFF09` : ""}`);
    try {
      const sig = yield* this.execBlock(fn.body, env, this.depth + 1);
      const ret = this.retValue;
      this.retValue = savedRet;
      this.env = savedEnv;
      if (sig === "stop") throw new StopSignal2();
      if (sig === "break" || sig === "continue") {
        throw new FloatError(`${fn.name} \u91CC\u51FA\u73B0\u4E86 break / balo\uFF0C\u4F46\u51FD\u6570\u4F53\u5185\u6CA1\u6709\u5BF9\u5E94\u5FAA\u73AF`, fn);
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
    }
  }
  /** 实例化：先铺字段默认值，再调用 init（init 里可以有 ask / wait，所以是生成器） */
  async *instantiate(cls, args, e) {
    const inst = instance(cls);
    const fieldEnv = new Env(this.globals, cls.name);
    fieldEnv.selfObj = inst;
    for (const f of collectFields(cls)) {
      inst.fields.set(f.name, f.init ? yield* this.evalExpr(f.init, fieldEnv) : defaultValue(f.type));
    }
    const init = findMethod(cls, "init") ?? findMethod(cls, "__init__") ?? findMethod(cls, "constructor");
    if (init) {
      yield* this.callUser(init.fn, args, inst, e, init.owner);
    } else if (args.length) {
      throw new FloatError(`\u7C7B ${cls.name} \u6CA1\u6709 init \u65B9\u6CD5\uFF0C\u4E0D\u80FD\u63A5\u6536\u53C2\u6570`, spanOf(e));
    }
    return inst;
  }
  /* ── import ── */
  async loadImport(target, fromFile, s) {
    const key = `${fromFile}::${target}`;
    if (this.imported.has(key)) return null;
    this.imported.add(key);
    const resolved = await this.resolveImport(target, fromFile);
    if (!resolved) throw new FloatError(`import \u627E\u4E0D\u5230\u300C${target}\u300D\uFF08\u76F8\u5BF9\u4E8E ${fromFile} \u4E0E\u5DE5\u7A0B\u6839\u76EE\u5F55\uFF09`, s);
    const cached = this.programs.get(resolved);
    if (cached) return cached;
    const src = this.sources().get(resolved);
    if (src === void 0) throw new FloatError(`import \u8BFB\u4E0D\u5230\u300C${target}\u300D\u7684\u5185\u5BB9`, s);
    const program = parseProgram(resolved, src);
    const errors = program.diagnostics.filter((d) => d.severity === "error");
    if (errors.length) {
      const first = errors[0];
      throw new FloatError(`${this.shortFile(resolved)} \u7B2C ${first.span.line} \u884C\u6709\u8BED\u6CD5\u9519\u8BEF\uFF1A${first.message}`, s);
    }
    this.programs.set(resolved, program);
    return program;
  }
  async resolveImport(target, fromFile) {
    const sources = this.sources();
    const root = this.importRoot.replace(/\\/g, "/").replace(/\/+$/, "");
    const baseDir = fromFile.includes("/") ? fromFile.replace(/\\/g, "/").replace(/\/[^/]*$/, "") : root;
    const join = (a, b) => a.replace(/\/+$/, "") + "/" + b;
    const candidates = target.includes(".") ? [join(baseDir, target), join(root, target)] : [join(baseDir, target + ".cw"), join(root, target + ".cw"), join(join(baseDir, target), "main.cw"), join(join(root, target), "main.cw")];
    for (const c of candidates) {
      const norm2 = c.replace(/\\/g, "/");
      if (sources.has(norm2)) return norm2;
    }
    const { api: api2 } = await Promise.resolve().then(() => (init_tauri(), tauri_exports));
    for (const c of candidates) {
      try {
        const text = await api2.fsRead(c.replace(/\//g, "\\"));
        const norm2 = c.replace(/\\/g, "/");
        sources.set(norm2, text);
        return norm2;
      } catch {
      }
    }
    return null;
  }
};
function spanOf(e) {
  if ("k" in e) return { file: e.file, line: e.line, col: e.col, endLine: e.endLine, endCol: e.endCol };
  return e;
}
function targetExpr(t) {
  if (t.k === "var") return { k: "var", name: t.name, file: t.file, line: t.line, col: t.col, endLine: t.endLine, endCol: t.endCol };
  if (t.k === "member") return { k: "member", obj: t.obj, name: t.name, file: t.file, line: t.line, col: t.col, endLine: t.endLine, endCol: t.endCol };
  return { k: "idx", obj: t.obj, index: t.index, file: t.file, line: t.line, col: t.col, endLine: t.endLine, endCol: t.endCol };
}

// tests/harness.ts
async function run(src, opts = {}) {
  const file = opts.file ?? "main.cw";
  const sources = /* @__PURE__ */ new Map([[file, src]]);
  globalThis.window = {
    __floatSources: sources,
    setTimeout: (fn, ms) => setTimeout(fn, ms)
  };
  const out = [];
  const err = [];
  const info = [];
  const answers = [...opts.answers ?? []];
  let runner = null;
  let settle = () => {
  };
  const done = new Promise((resolve) => {
    settle = resolve;
  });
  const record = (l) => {
    if (l.kind === "out") out.push(l.text);
    else if (l.kind === "err") err.push(l.text);
    else info.push(l.text);
  };
  runner = new Runner(file, {
    output: record,
    onState: () => {
    },
    onPaused: () => {
      setTimeout(() => {
        const r2 = runner;
        if (!r2 || r2.state !== "paused") return;
        if (r2.pauseReason === "ask") r2.resume(answers.shift() ?? "");
        else if (r2.pauseReason === "wait") r2.resume();
        else if (r2.pauseReason === "debug") r2.continueRun();
      }, 0);
    },
    onDone: () => settle()
  }, "/proj");
  runner.start(opts.mode ?? "run");
  const timeout = new Promise((resolve) => setTimeout(resolve, opts.timeoutMs ?? 5e3));
  await Promise.race([done, timeout]);
  const r = runner;
  return { out, err, info, vars: r.varsSnapshot(), state: r.state };
}

// tests/probe.ts
var cases = [
  ["\u5217\u8868\u91CC\u653E\u53D8\u91CF/\u8868\u8FBE\u5F0F/\u8C03\u7528", `
int a = 2
str b = "x"
list xs = [a, a * 3, b, join("k", a), len([1,2,3])]
print(xs)
`],
  ["\u5D4C\u5957\u5217\u8868\u4E0E\u5B57\u5178", `
list inner = [1, 2]
dict d = {"k": inner, "n": 3}
print(d)
print(d["k"][1])
`],
  ["\u51FD\u6570\u8C03\u7528\u7ED3\u679C\u76F4\u63A5\u4E0B\u6807", `
function mk()
    return [10, 20, 30]
print(mk()[1])
`],
  ["\u65B9\u6CD5\u94FE\u51FA\u73B0\u5728\u6761\u4EF6\u91CC", `
list xs = [3, 1, 2]
if (xs.sort().len() > 2)
    print("\u591A")
`],
  ["for \u904D\u5386\u51FD\u6570\u8FD4\u56DE\u503C", `
function rng(n)
    list out = []
    for i in range(n)
        out.push(i * 2)
    return out
for v in rng(3)
    print(v)
`],
  ["\u5B57\u5178\u503C\u91CC\u5199\u8C03\u7528", `
dict d = {"sum": add(1, 2), "up": "ab".upper()}
print(d)
`],
  ["\u7C7B\u5B57\u6BB5\u9ED8\u8BA4\u503C\u7528\u8C03\u7528", `
class P
    str tag = join("t", 1)
P p = new P()
print(p.tag)
`],
  ["\u5B57\u7B26\u4E32\u63D2\u503C", `
int score = 95
str name = "\u5C0F\u660E"
print("\u5F97\u5206\uFF1A{score}\uFF0C\u540D\u5B57\uFF1A{name}\uFF0C\u4E0B\u4E00\u540D\uFF1A{score + 1}")
`]
];
void (async () => {
  for (const [name, src] of cases) {
    const r = await run(src);
    console.log(name + " => " + (r.err.length ? "ERR " + r.err[0] : JSON.stringify(r.out)));
  }
})();
