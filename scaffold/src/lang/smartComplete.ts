/* 本地超小补全模型（Beta，默认关闭）。
 *
 * 不是神经网络，也不下载任何东西：一张「前两个词 → 下一个词」的计数表，
 * 训练语料只有工程里打开的 .co/.cw 和下面这一小把内置 Copper 样本，
 * 全在内存里，几十 KB。它做的事只有一件：按光标前的上下文 + 当前可见变量
 * 给已经算好的候选重排，让「for 变量 in …」下面打这个变量时，
 * 循环变量压过 if / 内建函数，而不是突然套出来一整段骨架。
 *
 * 模型不新增候选、不改档位：只在**同一档内**重排。
 * 真正的档位（局部变量最前）由 completions.ts 负责，模型无权推翻，
 * 免得统计噪声把一个 if 顶到变量前面。
 */

import type { Completion } from './completions';
import type { Analysis } from './analyze';
import { BAKED_NGRAM } from './corpusBaked';
import { BUILTIN_METHODS, KEYWORDS, TYPE_WORDS } from './spec';
import { builtinList } from './builtins';

const SEP = '\u001f';

export type SmartModel = {
  /** "上上个词␟上一个词" → { 下一个词: 出现次数 }；空串表示该位置没有词 */
  next: Map<string, Map<string, number>>;
  trainedLines: number;
  /** 变量名 → 声明类型（list/dict/int…）：用来把变量名归一成 <list> 这类标注再查表，
   *  让模型"分得清变量和列表"，预测出比如 `.push` 更可能跟在列表后面。 */
  varTypes: Map<string, string>;
};

/** 声明语句里抽变量类型：list xs = … / dict d / int n / bool ok … */
const DECL_RE = /^\s*(int|float|str|bool|list|dict|ptr|addr|var)\s+([A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*)/;

/* 内置的一小把语料：空工程里也知道 for / print / f{} 的常见搭配。
 * 用户说「可以先稍微喂一点点」，这就是那一点点。 */
const SEED = `
int 总数 = 0
for i in range(10)
    总数 += i
print(总数)
list 成绩 = [{"名": "阿方", "分": 93}, {"名": "小北", "分": 78}]
for s in 成绩
    print(s["名"], s["分"])
dict cfg = {"宽": 800, "高": 600}
print(f{"合计：", 总数, " 项"})
if 总数 > 0
    print("有数据")
fn 平均(xs)
    return data.avg(xs)

/* f{} 逗号拼接、切片、for 双变量 */
list nums = [1, 2, 3, 4]
print(f{"前两项：", nums[0:2]})
for i, v in nums
    print(i, v)
/* 容器可跨行写 */
dict cfg = {
    "宽": 800,
    "高": 600
}
print(data.require(cfg, "宽"))
print(data.get(cfg, "缺", 0))
int n = 0
++n
print(n)
/* 函数当值 */
function twice(x)
    return x * 2
fn g = twice
print(g(4))
/* 内联函数 lambda：fn(参数) return 表达式，能捕获外层变量 */
list xs = [1, 2, 3, 4]
print(data.filter(xs, fn(x) return x > 2))
fn 加(base)
    return fn(x) return x + base
fn 加100 = 加(100)
print(加100(5))
/* 9 个高阶函数：回调用 fn(...) return ... 或函数名 */
list 偶数 = data.map(xs, fn(x) return x * 2)
list 大的 = data.filter(xs, fn(x) return x > 2)
int 合计 = data.reduce(xs, fn(acc, x) return acc + x, 0)
bool 有 = data.any(xs, fn(x) return x > 3)
bool 全 = data.all(xs, fn(x) return x > 0)
var 首 = data.find(xs, fn(x) return x > 2)
int 位 = data.find_index(xs, fn(x) return x > 2)
/* lazy 参数：用不到的那个分支不求值，用来实现短路的 AND2 / OR2 */
fn bool AND2(bool x, lazy bool y)
    if x
        if y
            return true
    return false
fn bool OR2(bool x, lazy bool y)
    if x
        return true
    if y
        return true
    return false
print(AND2(false, 1 / 0 == 0))
/* 常用默认函数（内建 / 标准库）的调用搭配：让模型认得每个函数是干什么的 */
int 平方 = sqrt(16)
int 取整 = floor(3.7)
int 绝对值 = abs(-5)
int 随机 = rand(1, 10)
str 文本 = str(平方)
int 长度 = len("abc")
str 大写 = upper("hi")
str 小写 = lower("HI")
str 修剪 = trim("  hi  ")
list 部分 = split("a,b", ",")
list 排序 = sorted([3, 1, 2])
str 拼接 = join(["a", "b"], "-")
import data
int 总和 = data.sum([1, 2, 3])
float 均值 = data.avg([1, 2, 3])
list 唯一 = data.unique([1, 1, 2])
import json
str 序列化 = json.stringify({"a": 1})
`;

const TOKEN_RE = /[A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*|\d+(?:\.\d+)?|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[=+\-*/%,.()[\]{}:<>!]+/g;

/* 模型词表：关键字、类型词、内建/标准库函数名字留下原样；
 * 用户自己起的名字（变量、函数、列表）统一归一成 <id> 或 <类型>。
 * 归一之后，模型学的是"这个语言怎么写"，而不是某个人某个变量叫什么。 */
const KEEP_WORDS: Set<string> = (() => {
  const s = new Set<string>(KEYWORDS);
  for (const t of TYPE_WORDS) s.add(t);
  for (const m of BUILTIN_METHODS) s.add(m.name);
  for (const b of builtinList()) {
    s.add(b.name);
    if (b.name.includes('.')) for (const part of b.name.split('.')) s.add(part);
  }
  s.add('true'); s.add('false'); s.add('range'); s.add('print'); s.add('ask');
  /* 标准库的 data.* / json.* 与高阶函数：它们不是"用户起的名字"，
   * 归一成 <id> 的话「data.」后面就永远猜不出 sum / filter 了。 */
  for (const n of [
    'lazy', 'data', 'json',
    'sum', 'avg', 'median', 'stdev', 'count', 'unique', 'flatten', 'chunk', 'zip', 'enumerate',
    'reverse', 'slice', 'sort', 'sort_by', 'keys', 'values', 'entries', 'merge', 'get', 'require',
    'freq', 'tally', 'is_empty', 'max', 'min', 'pick', 'omit', 'map', 'filter', 'reduce', 'any',
    'all', 'find', 'find_index', 'each', 'flat',
    'stringify', 'parse', 'try_parse', 'valid', 'pretty',
  ]) s.add(n);
  return s;
})();

const WORD_RE = /^[A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*$/;
/** 占位符（查表时不能当成"具体词"输出给用户） */
const HOLE_RE = /^<[a-z]+>$/;

/** 导出给训练脚本：训练和推理必须用同一套切词，否则表就白烘了 */
export function tokenizeForModel(line: string): string[] {
  return tokenize(line);
}

/** 把一行 token 归一成查表用的样子：已知变量按声明类型，未知标识符一律 <id> */
export function normalizeTokens(ts: string[], varTypes: Map<string, string>): string[] {
  return ts.map((t) => {
    if (!WORD_RE.test(t) || KEEP_WORDS.has(t)) return t;
    const ty = varTypes.get(t);
    return ty ? `<${ty}>` : '<id>';
  });
}

/** 不看类型的第二条通道：所有自己起的名字一律 <id>。
 *  类型通道样本细（list 后面更可能 `.` / `[`），但 key 会被声明类型切碎；
 *  这条通道样本粗而稳。预测时先试细的，查不着就退到这条，两头都不空。 */
export function canonTokens(ts: string[]): string[] {
  return ts.map((t) => (WORD_RE.test(t) && !KEEP_WORDS.has(t) ? '<id>' : t));
}

function tokenize(line: string): string[] {
  const out: string[] = [];
  let m: RegExpExecArray | null;
  TOKEN_RE.lastIndex = 0;
  while ((m = TOKEN_RE.exec(line))) {
    const t = m[0];
    if (t[0] === '"' || t[0] === "'") out.push('<str>');
    else if (/^\d/.test(t)) out.push('<num>');
    else out.push(t);
  }
  return out;
}

function bump(table: Map<string, Map<string, number>>, key: string, word: string): void {
  let m = table.get(key);
  if (!m) { m = new Map(); table.set(key, m); }
  m.set(word, (m.get(word) ?? 0) + 1);
}

/** 一个文件自己的统计表（增量学习单位：改哪个文件就只重算哪个）。
 *  和 SmartModel 形状一致，只是下一层的 key 计数属于这一个文件。 */
export type SourceTable = {
  next: Map<string, Map<string, number>>;
  varTypes: Map<string, string>;
  lines: number;
};

/** 一个位置上的三级统计：三元（上三个词）、二元、一元一起记。
 *  预测时从三元往下退，宁可退得浅一点，也别猜不着。 */
function bumpPosition(table: Map<string, Map<string, number>>, ts: string[], i: number): void {
  const p1 = ts[i - 1] ?? '';
  const p2 = ts[i - 2] ?? '';
  const p3 = ts[i - 3] ?? '';
  bump(table, `${p3}${SEP}${p2}${SEP}${p1}`, ts[i]);
  bump(table, `${p2}${SEP}${p1}`, ts[i]);
  bump(table, `${SEP}${p1}`, ts[i]);
}

/** 只学一份文本，不掺 SEED / 烘焙表 —— 增量更新时用这个。
 *  成本与该文件行数成正比（150 KB 的方岛物语约 1~2 ms），
 *  不会随工程里其它文件一起涨。 */
export function learnSource(src: string): SourceTable {
  const next = new Map<string, Map<string, number>>();
  const varTypes = new Map<string, string>();
  let lines = 0;
  for (const raw of src.split('\n')) {
    const cut = raw.indexOf('/*');
    const line = (cut >= 0 ? raw.slice(0, cut) : raw).trim();
    if (!line) continue;
    const decl = DECL_RE.exec(line);
    if (decl) varTypes.set(decl[2], decl[1]);
    const ts = normalizeTokens(tokenize(line), varTypes);
    const cs = canonTokens(tokenize(line));
    lines++;
    for (let i = 0; i < ts.length; i++) bumpPosition(next, ts, i);
    for (let i = 0; i < cs.length; i++) bumpPosition(next, cs, i);
  }
  return { next, varTypes, lines };
}

/** 把一张表的计数按 sign（+1 加入 / -1 退出）并进目标模型。
 *  增量更新的核心：文件改了就把它旧表减掉、新表加上，其它文件的统计原样不动。 */
function mergeTable(model: SmartModel, table: SourceTable, sign: 1 | -1, all: SourceTable[]): void {
  for (const [key, words] of table.next) {
    let m = model.next.get(key);
    if (!m) { m = new Map(); model.next.set(key, m); }
    for (const [word, count] of words) {
      const n = (m.get(word) ?? 0) + sign * count;
      if (n > 0) m.set(word, n);
      else m.delete(word);
    }
    if (m.size === 0) model.next.delete(key);
  }
  for (const [name, ty] of table.varTypes) {
    if (sign > 0) { model.varTypes.set(name, ty); continue; }
    /* 退出：别的文件还声明这个名字的话不能删 */
    if (!all.some((t) => t !== table && t.varTypes.has(name))) model.varTypes.delete(name);
  }
  model.trainedLines += sign * table.lines;
}

/** 基础模型 = 方岛物语烘焙表 + SEED。这一层建一次就够，之后只往上加工程文件。 */
function baseModel(): SmartModel {
  const next = new Map<string, Map<string, number>>();
  const varTypes = new Map<string, string>();
  let trainedLines = 0;

  /* 方岛物语烘焙表：key 里已经带 <list>/<dict> 这类类型标注，直接并进来。
   * 它的计数已封顶 255，权重温和；工程源码 / SEED 的原始计数会自然盖过它。 */
  for (const key of Object.keys(BAKED_NGRAM)) {
    let m = next.get(key);
    if (!m) { m = new Map(); next.set(key, m); }
    for (const [word, count] of BAKED_NGRAM[key]) {
      m.set(word, (m.get(word) ?? 0) + count);
    }
  }
  const seed = learnSource(SEED);
  const model: SmartModel = { next, varTypes, trainedLines };
  for (const [key, words] of seed.next) {
    let m = model.next.get(key);
    if (!m) { m = new Map(); model.next.set(key, m); }
    for (const [word, count] of words) m.set(word, (m.get(word) ?? 0) + count);
  }
  for (const [name, ty] of seed.varTypes) model.varTypes.set(name, ty);
  model.trainedLines += seed.lines;
  return model;
}

/** 用工程源码训练（一次性全量）。给测试和"不想增量"的调用方用；
 *  IDE 热路径走下面的 SmartCorpus，不会每敲一个键就重跑这个。 */
export function trainModel(sources: string[]): SmartModel {
  const model = baseModel();
  const tables = sources.map((s) => learnSource(s));
  for (const t of tables) mergeTable(model, t, 1, tables);
  return model;
}

/* ── 增量语料库：编辑时只重算"变了的那个文件" ──────────────────────
 *
 * 以前是「语料总长度变了就全量重建」，打一个字母就把整个工程重训一遍，
 * 大文件下每击键几十毫秒，CPU 一直转 —— 这就是发热的来源。
 *
 * 现在：
 *   · 每个文件一张自己的表，文件内容没变就不动；
 *   · 改了的文件在**空闲 220 ms 后**才重算（打字过程中用上一版模型，不影响手感）；
 *   · 重算只算这一个文件，把它旧表减掉、新表加上，其它文件零成本；
 *   · 语料有预算（单文件 120 KB、总 4 MB、最多 16 个文件），超了不学。 */
export const SMART_BUDGET = { fileBytes: 120_000, totalBytes: 4_000_000, files: 16, idleMs: 320 };

/** 把文本切成"块"：空行分隔的一段（函数体 / 一小段语句）。
 *  编辑一行只会重算它所在的那一块，其它块原样复用；
 *  重训成本从"整个文件"降到"这一块"，上万行的文件也一样。 */
function splitBlocks(text: string): string[] {
  return text.split(/\n[ \t]*\n/);
}

export class SmartCorpus {
  private last = new Map<string, string>();
  private pending = new Map<string, string>();
  /** 文件 → 块文本数组 + 每块自己的表（块没变就不重算） */
  private blocks = new Map<string, { texts: string[]; tables: SourceTable[] }>();
  private model: SmartModel = baseModel();
  private timer: ReturnType<typeof setTimeout> | null = null;

  /** 上一次增量重建花了多少毫秒（诊断/基准用） */
  costMs = 0;
  /** 总共重建过多少次（诊断用：正常打字应该个位数，而不是每键一次） */
  builds = 0;

  /** 登记一个文件的最新内容。内容没变直接返回，零成本。 */
  setText(id: string, text: string): void {
    if (this.last.get(id) === text) return;
    this.last.set(id, text);
    this.pending.set(id, text);
    if (!this.timer) {
      this.timer = setTimeout(() => { this.timer = null; this.flush(); }, SMART_BUDGET.idleMs);
    }
  }

  /** 立刻把待处理的文件并进模型。测试里手动调，IDE 里由空闲定时器调。 */
  flush(): void {
    const t0 = performance.now();
    for (const [id, text] of this.pending) {
      const clipped = text.length > SMART_BUDGET.fileBytes ? text.slice(0, SMART_BUDGET.fileBytes) : text;
      const fresh = splitBlocks(clipped);
      const old = this.blocks.get(id);

      /* 文件数预算：先退掉最久没动的一个（先摘表，再撤统计，varTypes 才删得掉） */
      if (!old && this.blocks.size >= SMART_BUDGET.files) {
        const victim = this.blocks.keys().next().value;
        if (victim !== undefined && victim !== id) {
          const vt = this.blocks.get(victim);
          this.blocks.delete(victim);
          this.last.delete(victim);
          if (vt) for (const t of vt.tables) mergeTable(this.model, t, -1, this.allTables());
        }
      }

      if (!old) {
        const tables = fresh.map((b) => learnSource(b));
        this.blocks.set(id, { texts: fresh, tables });
        const all = this.allTables();
        for (const t of tables) mergeTable(this.model, t, 1, all);
        continue;
      }

      /* 逐块对比：文本没变的复用旧表，变了的减旧表 + 学新块 */
      const keepTexts: string[] = [];
      const keepTables: SourceTable[] = [];
      const minus: SourceTable[] = [];
      const plus: SourceTable[] = [];
      const max = Math.max(fresh.length, old.texts.length);
      for (let i = 0; i < max; i++) {
        const nt = fresh[i];
        const ot = old.texts[i];
        if (nt !== undefined && nt === ot && old.tables[i]) {
          keepTexts.push(nt);
          keepTables.push(old.tables[i]);
          continue;
        }
        if (ot !== undefined && old.tables[i]) minus.push(old.tables[i]);
        if (nt !== undefined) {
          const t = learnSource(nt);
          plus.push(t);
          keepTexts.push(nt);
          keepTables.push(t);
        }
      }
      /* 先登记新块表，再撤旧的：other 引用里不会残留已经退场的块 */
      this.blocks.set(id, { texts: keepTexts, tables: keepTables });
      const all = this.allTables();
      for (const t of minus) mergeTable(this.model, t, -1, all);
      for (const t of plus) mergeTable(this.model, t, 1, all);
    }
    this.pending.clear();
    this.builds++;
    this.costMs = performance.now() - t0;
  }

  private allTables(): SourceTable[] {
    const all: SourceTable[] = [];
    for (const f of this.blocks.values()) all.push(...f.tables);
    return all;
  }

  /** 当前模型（打字过程中可能是 320 ms 之前那一版，够用且几乎不花时间） */
  get(): SmartModel {
    return this.model;
  }

  /** 测试用：直接问"这个文件学进来没有" */
  has(id: string): boolean {
    return this.blocks.has(id);
  }
}

/** IDE 全局只用一个语料库（模型是纯统计，不含用户代码原文，不联网） */
export const smartCorpus = new SmartCorpus();

/** 当前作用域里「最该先出现」的名字：最近的几个循环变量最优先，其次是光标之前定义过的变量 */
export function scopeVariables(a: Analysis, lines: string[], cursorLine: number): string[] {
  const loop: string[] = [];
  const from = Math.min(cursorLine, lines.length) - 1;
  for (let i = from; i >= 0 && i >= cursorLine - 25; i--) {
    const m = /^\s*for\s+([A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*)\s+in\b/.exec(lines[i] ?? '');
    if (m && !loop.includes(m[1])) loop.push(m[1]);
    if (loop.length >= 3) break;
  }
  const rest = a.vars
    .filter((v) => v.line <= cursorLine && !loop.includes(v.name))
    .map((v) => v.name);
  return [...loop, ...rest];
}

/** 把 token 归一成"类型标注"：变量名换成 <list>/<dict>…，分化出变量和列表。
 *  字符串/数字/运算符原样保留。这样查表时用的是 `<list>` 的真实用法统计，
 *  而不是某个具体变量名的偶发搭配。 */
function typed(t: string, varTypes: Map<string, string>): string {
  const ty = varTypes.get(t);
  return ty ? `<${ty}>` : t;
}

/** 行内续写：从同一张计数表里，按"上三个词"往下退着猜下一个词。
 *  能连着猜（最多 3 个词 / 40 字），停在任何不确定的位置；
 *  只输出具体词，绝不把 <id>/<num> 这类占位符吐给用户。
 *
 *  三条护栏（宁可不出，也别乱出）：
 *    1. 行尾必须是空白（否则不知道要不要空格，拼起来就是错的）；
 *    2. 三元先试，退到二元要出现 ≥2 次、退到一元要 ≥6 次；
 *    3. 上下文全是自己起的名字（<id>）时不猜 —— 那是给他起新名字，不是补全。 */
const INLINE_MIN_SCORE = 0.28;
const INLINE_MAX_CHARS = 40;
const INLINE_MAX_TOKENS = 3;

/** 拼一个 token 进已有文本：按 Copper 的写法决定要不要空格 */
function joinToken(prev: string, tok: string): string {
  if (!prev) return tok;
  if (/^[)\]},.;:]/.test(tok)) return tok;
  if (prev.endsWith('(') || prev.endsWith('[') || prev.endsWith('{') || prev.endsWith(',')) return tok;
  if (tok === '(') {
    return /[A-Za-z0-9_\u4e00-\u9fa5)\]<]/.test(prev.slice(-1)) ? tok : ` ${tok}`;
  }
  if (!WORD_RE.test(tok) && !HOLE_RE.test(tok)) return ` ${tok}`;
  return ` ${tok}`;
}

/** 下一词：先走"带类型"的三元/二元/一元，再退到"不分类型"的那条通道。
 *  每个通道里用的是**概率份额**（该词次数 / 这个前文的总次数），不是裸次数 ——
 *  不然样本多的通道会靠"次数大"压过更准的通道。 */
function bestNext(model: SmartModel, typedCtx: string[], canonCtx: string[]): string | null {
  const layers: [Map<string, number> | undefined, number][] = [];
  const pushLayers = (ctx: string[], wTri: number, wBi: number, wUni: number): void => {
    const p1 = ctx[ctx.length - 1] ?? '';
    const p2 = ctx[ctx.length - 2] ?? '';
    const p3 = ctx[ctx.length - 3] ?? '';
    layers.push([model.next.get(`${p3}${SEP}${p2}${SEP}${p1}`), wTri]);
    layers.push([model.next.get(`${p2}${SEP}${p1}`), wBi]);
    layers.push([model.next.get(`${SEP}${p1}`), wUni]);
  };
  const same = typedCtx.length === canonCtx.length && typedCtx.every((t, i) => t === canonCtx[i]);
  pushLayers(typedCtx, 1.0, 0.5, 0.5);
  if (!same) pushLayers(canonCtx, 0.75, 0.4, 0.45);

  let best = '';
  let bestScore = 0;
  for (const [m, prior] of layers) {
    if (!m) continue;
    /* 概率只在"能输出的候选"之间归一：<id>/<num>/运算符本来就不该猜，
     * 把它们算进分母会把 range 这种正确答案压到线下面去。 */
    let total = 0;
    for (const [word, count] of m) {
      if (HOLE_RE.test(word)) continue;
      if (!WORD_RE.test(word) && word !== '(') continue;
      total += count;
    }
    if (!total) continue;
    for (const [word, count] of m) {
      if (HOLE_RE.test(word)) continue;
      if (!WORD_RE.test(word) && word !== '(') continue;
      const score = (count / total) * prior;
      if (score > bestScore) { bestScore = score; best = word; }
    }
  }
  return bestScore >= INLINE_MIN_SCORE ? best : null;
}

export function predictNext(model: SmartModel, lineBefore: string): string | null {
  if (!/\s$/.test(lineBefore)) return null;
  const raw = tokenize(lineBefore);
  if (raw.length === 0) return null;
  const ctx = normalizeTokens(raw, model.varTypes);
  const can = canonTokens(raw);
  const p1 = ctx[ctx.length - 1] ?? '';
  if (p1 === '<str>' || p1 === '<num>') return null;
  /* 最近三个词里全是占位符：说明上下文全是用户自己的命名，猜出来也没意义 */
  if (!can.slice(-3).some((t) => !HOLE_RE.test(t))) return null;

  let text = '';
  let prev = p1;
  const curT = ctx.slice();
  const curC = can.slice();
  const generated: string[] = [];
  for (let step = 0; step < INLINE_MAX_TOKENS; step++) {
    const cand = bestNext(model, curT, curC);
    if (!cand) break;
    if (generated.length && cand === generated[generated.length - 1]) break;
    const piece = step === 0 ? cand : joinToken(prev, cand);
    if (text.length + piece.length > INLINE_MAX_CHARS) break;
    text += piece;
    generated.push(cand);
    curT.push(cand);
    curC.push(cand);
    prev = cand;
    /* 左括号之后该是用户自己填的参数，再猜就是瞎猜了 */
    if (/^[)\]},.;:]/.test(cand) || cand === '(') break;
  }
  return text || null;
}

/** 同一档内按上下文重排；返回新数组，不改原候选。
 *  打分来源（叠加，都是"上一个/上两个词 → 下一个词"的概率估计）：
 *   1. 原始 token 的 2/3-gram（你自己的写法 + SEED）；
 *   2. 类型归一后的 2/3-gram（方岛物语烘焙表：list 后更可能 `.` / `[`，for 后更可能 i …）；
 *   3. 作用域变量压顶、前缀命中（老规则，保留）。 */
export function smartRank(
  model: SmartModel,
  items: Completion[],
  lineBefore: string,
  scopeVars: string[],
  word: string
): Completion[] {
  const ts = tokenize(lineBefore);
  const p1 = ts[ts.length - 1] ?? '';
  const p2 = ts[ts.length - 2] ?? '';
  const p3 = ts[ts.length - 3] ?? '';
  const tri = model.next.get(`${p3}${SEP}${p2}${SEP}${p1}`);
  const bi = model.next.get(`${p2}${SEP}${p1}`);
  const uni = model.next.get(`${SEP}${p1}`);

  // 类型视角：把最近三个 token 归一（成绩 → <list>、未知名 → <id>）
  const tp1 = typed(p1, model.varTypes);
  const tp2 = typed(p2, model.varTypes);
  const tp3 = typed(p3, model.varTypes);
  const tTri = tp1 === p1 && tp2 === p2 && tp3 === p3 ? undefined : model.next.get(`${tp3}${SEP}${tp2}${SEP}${tp1}`);
  const tBi = tp1 === p1 && tp2 === p2 ? undefined : model.next.get(`${tp2}${SEP}${tp1}`);
  const tUni = tp1 === p1 ? undefined : model.next.get(`${SEP}${tp1}`);

  const scope = new Set(scopeVars);
  const lowWord = word.toLowerCase();
  const tierOf = (it: Completion): number => Number(it.sortText[0] ?? '9');

  return items
    .map((it) => {
      // 候选是变量时，也用它自己的类型标注再查一次（如候选 push 不会命中，
      // 但候选若正好是某个 list 变量，typedHit 让它在合适上下文里加分）
      const candType = model.varTypes.get(it.label);
      const candTyped = candType ? `<${candType}>` : it.label;
      const rawN = (tri?.get(it.label) ?? 0) * 4 + (bi?.get(it.label) ?? 0) * 2 + (uni?.get(it.label) ?? 0);
      const typedN = (tTri?.get(it.label) ?? 0) * 4 + (tBi?.get(it.label) ?? 0) * 2 + (tUni?.get(it.label) ?? 0)
        + (tTri?.get(candTyped) ?? 0) * 4 + (tBi?.get(candTyped) ?? 0) * 2 + (tUni?.get(candTyped) ?? 0);
      const n = rawN + typedN;
      let boost = 0;
      if (scope.has(it.label)) boost += 6;
      if (word && it.label.toLowerCase().startsWith(lowWord)) boost += 2;
      if (n > 0) boost += Math.min(8, 1 + Math.log2(1 + n));
      return { it, boost };
    })
    .sort((x, y) =>
      tierOf(x.it) - tierOf(y.it)
      || y.boost - x.boost
      || x.it.sortText.localeCompare(y.it.sortText))
    .map((x) => x.it);
}
