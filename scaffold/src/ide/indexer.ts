/* 工程索引：跨文件的搜索 / 跳转 / 重命名都吃这一份东西。
 *
 * 真 IDE 和"能高亮"的区别就在这里——不是拿正则全文替换，而是先建符号，
 * 再用词法器区分「这是变量名」还是「这是字符串里凑巧一样的字」。
 * 所以 Ref 全部来自 token.ts，注释和字符串天然不会被误伤。
 */

import type { Analysis } from '../lang/analyze';
import { analyze } from '../lang/analyze';
import { codePart, tokenizeLine } from '../lang/token';
import { isCopperFile } from '../lang/spec';

export type Row = {
  path: string;
  text: string;
  lines: string[];
  /** 索引只要符号（函数/类/变量/大纲），不带 AST 和诊断 —— 比完整分析省一大截 */
  analysis: IndexAnalysis;
};

/** 索引用的精简分析结果（worker 里算好传回来，结构化克隆也轻） */
export type IndexAnalysis = Pick<Analysis, 'funcs' | 'classes' | 'vars' | 'outline'>;

const EMPTY_ANALYSIS: IndexAnalysis = { funcs: [], classes: [], vars: [], outline: [] };

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
  /** 只搜 Copper 源码（.co/.cw）；false 时连 json / md 一起搜 */
  sourceOnly: boolean;
  limit: number;
};

const MAX_FILE_BYTES = 4 * 1024 * 1024;   // 单文件超过 4MB 不进索引，避免卡死
/** 非源码文件（json/md/txt）更狠一点：超过 200KB 就不进索引。
 *  它们是搜索用的，几百 KB 的生成转储留在库里只会白占内存。 */
const MAX_NON_SOURCE_BYTES = 200 * 1024;
const MAX_ROWS = 2500;

/* ───────── 分析线程池 ─────────
 * 索引是纯 CPU 活，扔给 worker 之后主线程只管读盘和拼结果：
 * 打开大工程的卡顿感基本消失，几个文件并行算，几百行的工程几百毫秒就完事。
 * worker 起不来（老 WebView / 极简环境）就退回主线程同步算，功能不受影响。 */
type PoolTask = { path: string; text: string };
type PoolResult = IndexAnalysis;

class AnalyzePool {
  private workers: Worker[] = [];
  private idle: Worker[] = [];
  private queue: { task: PoolTask; resolve: (r: PoolResult) => void }[] = [];
  private busy = new Map<Worker, { id: number; resolve: (r: PoolResult) => void; task: PoolTask; timer: number }>();
  private seq = 0;
  readonly size: number;

  /** 单文件分析上限：超过这个时间就当它坏了，直接落一个空符号表。
   *  以前一个 worker 卡住，四条槽位会被逐个占死，进度条停在 30% 再也不动。 */
  private static readonly TASK_TIMEOUT = 6000;

  constructor() {
    const hw = (typeof navigator !== 'undefined' && navigator.hardwareConcurrency) || 4;
    // 留两个核给界面和运行：再多就是互相抢，反而更慢
    this.size = Math.max(1, Math.min(4, hw - 1));
    try {
      for (let i = 0; i < this.size; i++) this.spawn();
    } catch {
      this.workers = [];
    }
  }

  private spawn(): Worker | null {
    try {
      const w = new Worker(new URL('./analyzeWorker.ts', import.meta.url), { type: 'module' });
      w.onmessage = (e: MessageEvent<{ id: number } & PoolResult>): void => {
        const job = this.busy.get(w);
        if (!job || job.id !== e.data.id) return;
        window.clearTimeout(job.timer);
        this.busy.delete(w);
        job.resolve({ funcs: e.data.funcs, classes: e.data.classes, vars: e.data.vars, outline: e.data.outline });
        this.idle.push(w);
        this.pump();
      };
      w.onerror = (): void => {
        // worker 挂了：当前这单退回主线程算，别让索引永远等下去
        const job = this.busy.get(w);
        if (job) window.clearTimeout(job.timer);
        this.busy.delete(w);
        if (job) job.resolve(syncAnalyze(job.task));
        this.idle.push(w);
        this.pump();
      };
      this.workers.push(w);
      this.idle.push(w);
      return w;
    } catch {
      return null;
    }
  }

  get ok(): boolean { return this.workers.length > 0; }

  run(task: PoolTask): Promise<PoolResult> {
    if (!this.ok) return Promise.resolve(syncAnalyze(task));
    return new Promise<PoolResult>((resolve) => {
      this.queue.push({ task, resolve });
      this.pump();
    });
  }

  private pump(): void {
    while (this.queue.length > 0 && this.idle.length > 0) {
      const w = this.idle.pop()!;
      const job = this.queue.shift()!;
      const id = ++this.seq;
      const timer = window.setTimeout(() => {
        // 卡死的那条槽位直接换掉：terminate 掉、补一个新的，队列继续走
        const stuck = this.busy.get(w);
        if (!stuck || stuck.id !== id) return;
        this.busy.delete(w);
        try { w.terminate(); } catch { /* 已经死了 */ }
        this.workers = this.workers.filter((x) => x !== w);
        this.idle = this.idle.filter((x) => x !== w);
        stuck.resolve(EMPTY_ANALYSIS);
        if (this.queue.length > 0 && this.spawn()) { /* 补位 */ }
        this.pump();
      }, AnalyzePool.TASK_TIMEOUT);
      this.busy.set(w, { id, resolve: job.resolve, task: job.task, timer });
      w.postMessage({ id, path: job.task.path, text: job.task.text });
    }
  }

  dispose(): void {
    for (const job of this.busy.values()) window.clearTimeout(job.timer);
    for (const w of this.workers) w.terminate();
    this.workers = [];
    this.idle = [];
  }
}

/** 主线程兜底：和 worker 里算的是同一份东西 */
function syncAnalyze(task: PoolTask): PoolResult {
  try {
    const a = analyze(task.path, task.text, undefined, { symbolsOnly: true });
    return { funcs: a.funcs, classes: a.classes, vars: a.vars, outline: a.outline };
  } catch {
    return EMPTY_ANALYSIS;
  }
}

/** 索引进度：给底部控制台上方那条蓝色进度条用 */
export type IndexProgress = { done: number; total: number; file: string };

export class ProjectIndex {
  private rows = new Map<string, Row>();
  private order: string[] = [];
  private reader: (path: string) => Promise<string>;
  private lister: (dir: string) => Promise<{ name: string; isDir: boolean; relPath: string; size?: number }[]>;
  private root: string;
  private pool: AnalyzePool | null = null;

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

  /** 覆盖单个文件的索引（编辑器保存 / 改动后生效，不用整工程重扫）。
   *  文本立刻进库（搜索马上能用），符号表丢给 worker 后补——
   *  保存一个大文件时不会在主线程上卡一下。
   *  返回的 Promise 在符号表补完时 resolve：测试和"跳转定义"这类调用方
   *  要么 await 它，要么用 whenIdle()，别在 worker 还没回话时就查符号表。 */
  put(path: string, text: string): Promise<void> {
    const p = path.replace(/\\/g, '/');
    if (text.length > MAX_FILE_BYTES) return Promise.resolve();
    if (!this.rows.has(p)) this.order.push(p);
    if (!isCopperFile(p)) {
      this.rows.set(p, { path: p, text, lines: text.split('\n'), analysis: EMPTY_ANALYSIS });
      return Promise.resolve();
    }
    this.rows.set(p, { path: p, text, lines: text.split('\n'), analysis: EMPTY_ANALYSIS });
    if (!this.pool) this.pool = new AnalyzePool();
    this.pending++;
    return this.pool.run({ path: p, text }).then((a) => {
      this.putAnalyzed(p, text, a);
    }).finally(() => {
      this.pending--;
      if (this.pending <= 0) {
        this.pending = 0;
        const waiters = this.idleWaiters.splice(0);
        for (const w of waiters) w();
      }
    });
  }

  /** 还有多少文件在等 worker 补符号表 */
  private pending = 0;
  private idleWaiters: (() => void)[] = [];

  /** 等所有待补的符号表落库（原因同上：worker 是异步的） */
  whenIdle(): Promise<void> {
    if (this.pending <= 0) return Promise.resolve();
    return new Promise<void>((resolve) => this.idleWaiters.push(resolve));
  }

  private putAnalyzed(path: string, text: string, analysis: IndexAnalysis): void {
    const p = path.replace(/\\/g, '/');
    if (text.length > MAX_FILE_BYTES) return;
    if (!this.rows.has(p)) this.order.push(p);
    this.rows.set(p, { path: p, text, lines: text.split('\n'), analysis });
    this.collectNames(p, analysis);
  }

  /** 工程级名字总表：所有已知文件里的顶层名字（函数/类/字段/变量/参数）。
   *  诊断拿它当兜底表——工程里定义过的名字，不该在某个文件里被标成「没有定义」。 */
  private names = new Set<string>();

  private collectNames(p: string, analysis: IndexAnalysis): void {
    void p;
    for (const f of analysis.funcs) this.names.add(f.name);
    for (const c of analysis.classes) {
      this.names.add(c.name);
      for (const f of c.fields) this.names.add(f.name);
      for (const m of c.methods) this.names.add(m.name);
    }
    for (const v of analysis.vars) this.names.add(v.name);
  }

  /** 给诊断层用的工程名字总表（拷贝一份，外面改坏了不影响索引） */
  allNames(): Set<string> { return new Set(this.names); }

  drop(path: string): void {
    const p = path.replace(/\\/g, '/');
    this.rows.delete(p);
    this.order = this.order.filter((x) => x !== p);
  }

  clear(): void { this.rows.clear(); this.order = []; }

  /** 重建整个工程的索引。
   *  onProgress：每分析完一个文件回报一次（done/total/文件名），给控制台上方的进度条；
   *  分析在 worker 线程池里并行做，读盘和拼结果留在主线程，打开大工程不再整屏卡死。 */
  async refresh(onProgress?: (p: IndexProgress) => void): Promise<number> {
    const files: string[] = [];
    const walk = async (dir: string, depth: number): Promise<void> => {
      if (depth > 12) return;
      let ents;
      try { ents = await this.lister(dir); } catch { return; }
      for (const e of ents) {
        if (e.name === 'node_modules' || e.name === '.git' || e.name === 'target' || e.name === '.float') continue;
        const p = dir.replace(/\\/g, '/') + '/' + e.name;
        if (e.isDir) { await walk(p, depth + 1); continue; }
        if (!/\.(co|cw|json|md|txt)$/i.test(e.name)) continue;
        // 按体积先筛掉，别把几十 MB 的日志/转储先读进内存再扔掉——
        // 一个 bootstrap 目录里全是 500KB 的 AST 转储，读完当场几百 MB。
        const cap = isCopperFile(p) ? MAX_FILE_BYTES : MAX_NON_SOURCE_BYTES;
        if (e.size !== undefined && e.size > cap) continue;
        files.push(p);
      }
    };
    await walk(this.root, 0);
    this.clear();
    const list = files.slice(0, MAX_ROWS);
    const total = list.length;
    onProgress?.({ done: 0, total, file: '' });
    if (!this.pool) this.pool = new AnalyzePool();

    // 先并发读盘（一次 12 个），再把源码文件丢给 worker 池分析
    let done = 0;
    const inFlight: Promise<void>[] = [];
    const startFile = (f: string): Promise<void> => {
      const job = (async (): Promise<void> => {
        try {
          const text = await this.reader(f);
          if (isCopperFile(f) && text.length <= MAX_FILE_BYTES) {
            this.putAnalyzed(f, text, await this.pool!.run({ path: f.replace(/\\/g, '/'), text }));
          } else {
            this.putAnalyzed(f, text, EMPTY_ANALYSIS);
          }
        } catch { /* 读不到就跳过 */ }
        done++;
        onProgress?.({ done, total, file: f });
      })();
      inFlight.push(job);
      return job;
    };

    const CONCURRENCY = 12;
    for (let i = 0; i < list.length; i++) {
      void startFile(list[i]);
      if (inFlight.length >= CONCURRENCY) {
        await inFlight.shift();
      }
    }
    await Promise.all(inFlight);
    onProgress?.({ done: total, total, file: '' });
    return this.rows.size;
  }

  dispose(): void {
    this.pool?.dispose();
    this.pool = null;
  }

  /* ───────── 文本搜索 ───────── */

  search(opt: SearchOptions): { hits: Hit[]; truncated: boolean } {
    const hits: Hit[] = [];
    if (!opt.query) return { hits, truncated: false };
    const re = buildMatcher(opt);
    if (!re) return { hits, truncated: false };

    for (const path of this.order) {
      if (opt.sourceOnly && !isCopperFile(path)) continue;
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
      if (!isCopperFile(path)) continue;
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
      if (!isCopperFile(path)) continue;
      const v = this.rows.get(path)?.analysis.vars.find((x) => x.name === name);
      if (v) return { file: path, line: v.line, col: 1, name, kind: '变量' };
    }
    return null;
  }

  /** 全部引用点（词法级 + 跳过注释和字符串） */
  references(name: string, includeDecl = true): SymRef[] {
    const out: SymRef[] = [];
    for (const path of this.order) {
      if (!isCopperFile(path)) continue;
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
