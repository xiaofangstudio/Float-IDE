/* 工程索引：跨文件的搜索 / 跳转 / 重命名都吃这一份东西。
 *
 * 真 IDE 和"能高亮"的区别就在这里——不是拿正则全文替换，而是先建符号，
 * 再用词法器区分「这是变量名」还是「这是字符串里凑巧一样的字」。
 * 所以 Ref 全部来自 token.ts，注释和字符串天然不会被误伤。
 */

import type { Analysis } from '../lang/analyze';
import { analyze } from '../lang/analyze';
import { codePart, tokenizeLine } from '../lang/token';

export type Row = { path: string; text: string; lines: string[]; analysis: Analysis };

export type Hit = {
  file: string;
  line: number;
  col: number;
  endCol: number;
  text: string;
  /** 命中片段在行内的列范围，用于高亮 */
  from: number;
  to: number;
};

export type SymRef = { file: string; line: number; col: number; endCol: number };

export type Definition = { file: string; line: number; col: number; name: string; kind: string };

export type SearchOptions = {
  query: string;
  matchCase: boolean;
  wholeWord: boolean;
  useRegex: boolean;
  /** 只搜 .cw 源码；false 时连 json / md 一起搜 */
  sourceOnly: boolean;
  limit: number;
};

const MAX_FILE_BYTES = 4 * 1024 * 1024;   // 单文件超过 4MB 不进索引，避免卡死
const MAX_ROWS = 4000;

export class ProjectIndex {
  private rows = new Map<string, Row>();
  private order: string[] = [];
  private reader: (path: string) => Promise<string>;
  private lister: (dir: string) => Promise<{ name: string; isDir: boolean; relPath: string }[]>;
  private root: string;

  constructor(
    root: string,
    reader: (path: string) => Promise<string>,
    lister: (dir: string) => Promise<{ name: string; isDir: boolean; relPath: string }[]>
  ) {
    this.root = root.replace(/\\/g, '/');
    this.reader = reader;
    this.lister = lister;
  }

  get paths(): string[] { return this.order; }
  get size(): number { return this.rows.size; }
  row(path: string): Row | undefined { return this.rows.get(path.replace(/\\/g, '/')); }

  /** 覆盖单个文件的索引（编辑器保存后立刻生效，不用整工程重扫） */
  put(path: string, text: string): void {
    const p = path.replace(/\\/g, '/');
    if (text.length > MAX_FILE_BYTES) return;
    if (!this.rows.has(p)) this.order.push(p);
    this.rows.set(p, { path: p, text, lines: text.split('\n'), analysis: analyze(p, text) });
  }

  drop(path: string): void {
    const p = path.replace(/\\/g, '/');
    this.rows.delete(p);
    this.order = this.order.filter((x) => x !== p);
  }

  clear(): void { this.rows.clear(); this.order = []; }

  async refresh(): Promise<number> {
    const files: string[] = [];
    const walk = async (dir: string, depth: number): Promise<void> => {
      if (depth > 12) return;
      let ents;
      try { ents = await this.lister(dir); } catch { return; }
      for (const e of ents) {
        if (e.name === 'node_modules' || e.name === '.git' || e.name === 'target' || e.name === '.float') continue;
        const p = dir.replace(/\\/g, '/') + '/' + e.name;
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
      } catch { /* 读不到就跳过 */ }
    }
    return this.rows.size;
  }

  /* ───────── 文本搜索 ───────── */

  search(opt: SearchOptions): { hits: Hit[]; truncated: boolean } {
    const hits: Hit[] = [];
    if (!opt.query) return { hits, truncated: false };
    const re = buildMatcher(opt);
    if (!re) return { hits, truncated: false };

    for (const path of this.order) {
      if (opt.sourceOnly && !path.toLowerCase().endsWith('.cw')) continue;
      const row = this.rows.get(path);
      if (!row) continue;
      row.lines.forEach((raw, i) => {
        re.lastIndex = 0;
        let m: RegExpExecArray | null;
        while ((m = re.exec(raw)) !== null) {
          if (m[0].length === 0) { re.lastIndex++; continue; }
          hits.push({
            file: path, line: i + 1, col: m.index + 1, endCol: m.index + m[0].length + 1,
            text: raw, from: m.index, to: m.index + m[0].length
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
  replace(opt: SearchOptions, replacement: string): { file: string; count: number; text: string }[] {
    const groups = new Map<string, { hits: Hit[]; lines: string[] }>();
    const res = this.search({ ...opt, limit: Number.MAX_SAFE_INTEGER });
    for (const h of res.hits) {
      const row = this.rows.get(h.file);
      if (!row) continue;
      let g = groups.get(h.file);
      if (!g) { g = { hits: [], lines: [...row.lines] }; groups.set(h.file, g); }
      g.hits.push(h);
    }
    const out: { file: string; count: number; text: string }[] = [];
    for (const [file, g] of groups) {
      // 从后往前改，避免前面的替换把后面的列号挤偏
      const sorted = [...g.hits].sort((a, b) => b.line - a.line || b.from - a.from);
      const byLine = new Map<number, Hit[]>();
      for (const h of sorted) {
        const arr = byLine.get(h.line) ?? [];
        arr.push(h);
        byLine.set(h.line, arr);
      }
      for (const [lineNo, arr] of byLine) {
        let s = g.lines[lineNo - 1] ?? '';
        // 同一行内按列从右往左
        const asc = [...arr].sort((a, b) => b.from - a.from);
        for (const h of asc) {
          const rep = opt.useRegex ? s.slice(h.from, h.to).replace(buildMatcher(opt)!, replacement) : replacement;
          s = s.slice(0, h.from) + rep + s.slice(h.to);
        }
        g.lines[lineNo - 1] = s;
      }
      out.push({ file, count: g.hits.length, text: g.lines.join('\n') });
    }
    return out;
  }

  /* ───────── 符号：跳转 / 引用 / 重命名 ───────── */

  /** 光标下的词（用 tokenizeLine 拿，字符串/注释里的不会被当成符号） */
  wordAt(path: string, line: number, col: number): string | null {
    const row = this.row(path);
    if (!row) return null;
    const raw = row.lines[line - 1];
    if (raw === undefined) return null;
    const { code } = codePart(raw);
    let toks: ReturnType<typeof tokenizeLine>;
    try { toks = tokenizeLine(code, line, 1); } catch { return null; }
    for (const t of toks) {
      if (t.k !== 'ident' && t.k !== 'kw') continue;
      if (col >= t.col && col <= t.endCol) return t.v;
    }
    return null;
  }

  /** 定义位置：优先用符号表（函数/类/变量），找不到再退化到"第一次出现" */
  definition(file: string, line: number, col: number): Definition | null {
    const name = this.wordAt(file, line, col);
    if (!name) return null;
    if (isKeyword(name)) return null;

    // 先在符号表里找（跨文件）
    for (const path of this.order) {
      const row = this.rows.get(path);
      if (!row) continue;
      if (!path.toLowerCase().endsWith('.cw')) continue;
      const a = row.analysis;
      const f = a.funcs.find((x) => x.name === name);
      if (f) return { file: path, line: f.line, col: 1, name, kind: f.owner ? '方法' : '函数' };
      const c = a.classes.find((x) => x.name === name);
      if (c) return { file: path, line: c.line, col: 1, name, kind: '类' };
    }
    // 再找变量：先看本文件，再跨文件
    const own = this.row(file)?.analysis.vars.find((v) => v.name === name);
    if (own) return { file: file.replace(/\\/g, '/'), line: own.line, col: 1, name, kind: '变量' };
    for (const path of this.order) {
      if (!path.toLowerCase().endsWith('.cw')) continue;
      const v = this.rows.get(path)?.analysis.vars.find((x) => x.name === name);
      if (v) return { file: path, line: v.line, col: 1, name, kind: '变量' };
    }
    return null;
  }

  /** 全部引用点（词法级 + 跳过注释和字符串） */
  references(name: string, includeDecl = true): SymRef[] {
    const out: SymRef[] = [];
    for (const path of this.order) {
      if (!path.toLowerCase().endsWith('.cw')) continue;
      const row = this.rows.get(path);
      if (!row) continue;
      row.lines.forEach((raw, i) => {
        const { code } = codePart(raw);
        if (!code.includes(name)) return;
        let toks: ReturnType<typeof tokenizeLine>;
        try { toks = tokenizeLine(code, i + 1, 1); } catch { return; }
        for (const t of toks) {
          if (t.k !== 'ident' && t.k !== 'kw') continue;
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

  private definitionOfDecl(name: string): { file: string; line: number } | null {
    for (const path of this.order) {
      const a = this.rows.get(path)?.analysis;
      if (!a) continue;
      if (a.funcs.some((f) => f.name === name)) return { file: path, line: a.funcs.find((f) => f.name === name)!.line };
      if (a.classes.some((c) => c.name === name)) return { file: path, line: a.classes.find((c) => c.name === name)!.line };
    }
    return null;
  }

  /** 跨文件重命名；返回每个文件的新内容 */
  rename(oldName: string, newName: string): { file: string; count: number; text: string }[] {
    const groups = new Map<string, { refs: SymRef[]; lines: string[] }>();
    for (const r of this.references(oldName)) {
      const row = this.rows.get(r.file);
      if (!row) continue;
      let g = groups.get(r.file);
      if (!g) { g = { refs: [], lines: [...row.lines] }; groups.set(r.file, g); }
      g.refs.push(r);
    }
    const out: { file: string; count: number; text: string }[] = [];
    for (const [file, g] of groups) {
      const byLine = new Map<number, SymRef[]>();
      for (const r of g.refs) {
        const arr = byLine.get(r.line) ?? [];
        arr.push(r);
        byLine.set(r.line, arr);
      }
      for (const [lineNo, arr] of byLine) {
        let s = g.lines[lineNo - 1] ?? '';
        for (const r of [...arr].sort((a, b) => b.col - a.col)) {
          s = s.slice(0, r.col - 1) + newName + s.slice(r.endCol - 1);
        }
        g.lines[lineNo - 1] = s;
      }
      out.push({ file, count: g.refs.length, text: g.lines.join('\n') });
    }
    return out;
  }

  /** 按文件名前缀做快速跳转（Ctrl+P 用） */
  filesMatching(prefix: string, limit = 40): string[] {
    const q = prefix.toLowerCase();
    const scored: { p: string; s: number }[] = [];
    for (const p of this.order) {
      const name = p.split('/').pop() ?? p;
      const nl = name.toLowerCase();
      if (!q) { scored.push({ p, s: 0 }); continue; }
      let idx = -1; let score = 0;
      let cursor = 0;
      for (const ch of q) {
        const found = nl.indexOf(ch, cursor);
        if (found < 0) { idx = -1; break; }
        score += found === cursor ? 2 : 1;
        cursor = found + 1;
        idx = cursor;
      }
      if (idx < 0) continue;
      // 连续命中 + 越短越靠前
      scored.push({ p, s: score - name.length * 0.05 });
    }
    scored.sort((a, b) => b.s - a.s);
    return scored.slice(0, limit).map((x) => x.p);
  }
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function buildMatcher(opt: SearchOptions): RegExp | null {
  let body: string;
  if (opt.useRegex) {
    body = opt.query;
  } else {
    body = escapeRe(opt.query);
  }
  if (opt.wholeWord) body = `(?<![\\w\\u4e00-\\u9fa5])(?:${body})(?![\\w\\u4e00-\\u9fa5])`;
  let flags = 'g';
  if (!opt.matchCase) flags += 'i';
  try { return new RegExp(body, flags); } catch { return null; }
}

function isKeyword(w: string): boolean {
  return /^(if|elif|else|while|for|in|range|repeat|forever|function|return|class|new|self|super|extends|print|ask|import|try|catch|finally|throw|int|float|str|bool|list|dict|var|break|out|balo|continue|wait|time|reset|del|insert|replace|add|to|len|cd)$/i.test(w);
}
