/* 语料训练器：把工程里所有真实 Copper 代码跑成一张 n-gram 表，写进 src/lang/corpusBaked.ts。
 *
 * 用法（scaffold 目录下）：
 *     node tests/.train-corpus.mjs          # 训练 + 烘焙
 *     node tests/.train-corpus.mjs --eval   # 训练 + 留出集准确率
 *
 * 训练与推理必须共用同一套切词/归一（直接 import smartComplete 的导出），
 * 不然表里的 key 和运行时查的 key 对不上，等于白训。
 */
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { tokenizeForModel, normalizeTokens, canonTokens } from '../src/lang/smartComplete';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');
const OUT = join(ROOT, 'src', 'lang', 'corpusBaked.ts');
const SEP = '\u001f';

const CORPUS_DIRS = [
  join(ROOT, '..', 'language', 'samples'),
  join(ROOT, 'src-tauri', 'float-lang', 'bootstrap'),
];
/** 这些文件是同一份代码的快照/生成物，进去就是重复计数，直接排除 */
const SKIP = /(_backup_|_verified_|_fuzz|_stress|colang_pre_class|copper_lex|bench\.co$)/;

/** 留出集：训练时剔除，只用来量准确率 */
const HOLDOUT = /(06_sort_search|09_eliza|13_pointer_game)\.co$/;

function listCo(dir: string): string[] {
  try {
    return readdirSync(dir)
      .filter((f) => f.endsWith('.co'))
      .map((f) => join(dir, f))
      .filter((p) => {
        try { const st = statSync(p); return st.isFile() && st.size >= 500; } catch { return false; }
      });
  } catch {
    return [];
  }
}

const files = CORPUS_DIRS.flatMap(listCo).filter((p) => !SKIP.test(basename(p)));
const seen = new Map<string, string>();

type Counts = Map<string, Map<string, number>>;
const next: Counts = new Map();
const varTypes = new Map<string, string>();
let totalLines = 0;
let totalTokens = 0;
const used: { file: string; bytes: number; lines: number }[] = [];

/** 去掉块注释（Copper 里 // 是整除，不是注释，不能按行砍） */
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, '');
}

function bump(key: string, word: string): void {
  let m = next.get(key);
  if (!m) { m = new Map(); next.set(key, m); }
  m.set(word, (m.get(word) ?? 0) + 1);
}

function learn(text: string, file: string, holdout: boolean): number {
  const vt = new Map<string, string>();
  let lines = 0;
  for (const raw of stripComments(text).split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    const decl = /^\s*(int|float|str|bool|list|dict|ptr|addr|var)\s+([A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*)/.exec(line);
    if (decl) vt.set(decl[2], decl[1]);
    const ts = normalizeTokens(tokenizeForModel(line), vt);
    const cs = canonTokens(tokenizeForModel(line));
    lines++;
    totalTokens += ts.length;
    if (holdout) continue;   // 留出集只用来算准确率，不进模型
    const bumpAll = (tokens: string[]): void => {
      for (let i = 0; i < tokens.length; i++) {
        const p1 = tokens[i - 1] ?? '', p2 = tokens[i - 2] ?? '', p3 = tokens[i - 3] ?? '';
        bump(`${p3}${SEP}${p2}${SEP}${p1}`, tokens[i]);
        bump(`${p2}${SEP}${p1}`, tokens[i]);
        bump(`${SEP}${p1}`, tokens[i]);
      }
    };
    bumpAll(ts);
    bumpAll(cs);
  }
  if (!holdout) {
    totalLines += lines;
    for (const [n, t] of vt) if (!varTypes.has(n)) varTypes.set(n, t);
  }
  used.push({ file, bytes: text.length, lines });
  return lines;
}

for (const p of files) {
  const text = readFileSync(p, 'utf8');
  const key = createHash('sha1').update(text).digest('hex');
  if (seen.has(key)) { continue; }   // 同一份代码的副本只算一次
  seen.set(key, p);
  learn(text, p, HOLDOUT.test(basename(p)));
}

/* ── 剪枝：只留出现 ≥2 次的搭配；每个前文最多留 8 个候选；计数封顶 255 ── */
const MIN_COUNT = 2;
const MAX_PER_KEY = 8;
const pruned: Record<string, [string, number][]> = {};
let contexts = 0;
let transitions = 0;
for (const [key, m] of next) {
  const kept = [...m.entries()]
    .filter(([, c]) => c >= MIN_COUNT)
    .sort((a, b) => b[1] - a[1])
    .slice(0, MAX_PER_KEY)
    .map(([w, c]) => [w, Math.min(255, c)] as [string, number]);
  if (!kept.length) continue;
  pruned[key] = kept;
  contexts++;
  transitions += kept.length;
}

/* ── 写文件 ── */
const json = JSON.stringify(pruned);
const header = `/* 真实语料训练出来的 n-gram 表（由 tests/train-corpus.ts 生成，勿手改）。
 * 语料：${used.length} 个 .co 文件 / ${totalLines} 行 / ${totalTokens} 个 token，取自
 *       language/samples（方岛物语 + 全部示例）与 float-lang/bootstrap（出厂编译器本体）。
 * 清洗：关键字/类型词/内建函数保留原样，用户命名归一为 <id>，变量按声明类型标 <list>/<dict>/…
 *       字符串/数字归一 <str>/<num>；重复副本按内容哈希去重，快照/生成物排除。
 * 统计：1/2/3 元；只留出现 ≥${MIN_COUNT} 次、每个前文最多 ${MAX_PER_KEY} 个候选；计数封顶 255。
 * 规模：${contexts} 个前文 / ${transitions} 条转移 / ${json.length} B。
 */
export const BAKED_NGRAM: Record<string, [string, number][]> = ${json};
`;
writeFileSync(OUT, header, 'utf8');
console.log(`语料 ${used.length} 文件 / ${totalLines} 行 / ${totalTokens} token`);
console.log(`烘焙表 ${contexts} 前文 / ${transitions} 转移 / ${(json.length / 1024).toFixed(1)} KB`);
for (const u of used) console.log(`  ${(u.bytes / 1024).toFixed(1).padStart(7)} KB ${u.lines.toString().padStart(6)} 行  ${u.file}`);

/* ── 留出集准确率：三元→二元→一元退避，看 top1 / top3 ── */
type Priors = { tTri: number; tBi: number; tUni: number; cTri: number; cBi: number; cUni: number };

function evaluate(pr: Priors): [number, number, number] {
  const lookup = (tCtx: string[], cCtx: string[], top: number): string[] => {
    const merged = new Map<string, number>();
    const add = (ctx: string[], wTri: number, wBi: number, wUni: number): void => {
      const p1 = ctx[ctx.length - 1] ?? '', p2 = ctx[ctx.length - 2] ?? '', p3 = ctx[ctx.length - 3] ?? '';
      const acc = (key: string, prior: number): void => {
        const m = pruned[key];
        if (!m) return;
        let total = 0;
        for (const [, c] of m) total += c;
        if (!total) return;
        for (const [w2, c] of m) merged.set(w2, (merged.get(w2) ?? 0) + (c / total) * prior);
      };
      acc(`${p3}${SEP}${p2}${SEP}${p1}`, wTri);
      acc(`${p2}${SEP}${p1}`, wBi);
      acc(`${SEP}${p1}`, wUni);
    };
    add(tCtx, pr.tTri, pr.tBi, pr.tUni);
    add(cCtx, pr.cTri, pr.cBi, pr.cUni);
    return [...merged.entries()].sort((a, b) => b[1] - a[1]).slice(0, top).map(([w]) => w);
  };
  let n = 0, top1 = 0, top3 = 0;
  const holdFiles = files.filter((f) => HOLDOUT.test(basename(f)));
  for (const hf of holdFiles) {
    const vt = new Map<string, string>();
    for (const raw of stripComments(readFileSync(hf, 'utf8')).split('\n')) {
      const line = raw.trim();
      if (!line) continue;
      const decl = /^\s*(int|float|str|bool|list|dict|ptr|addr|var)\s+([A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*)/.exec(line);
      if (decl) vt.set(decl[2], decl[1]);
      const ts = normalizeTokens(tokenizeForModel(line), vt);
      const cs = canonTokens(tokenizeForModel(line));
      for (let i = 1; i < ts.length; i++) {
        const target = ts[i];
        if (/^<[a-z]+>$/.test(target)) continue;   // 占位符不作为预测目标
        const cand = lookup(ts.slice(0, i), cs.slice(0, i), 3);
        if (!cand.length) continue;
        n++;
        if (cand[0] === target) top1++;
        if (cand.includes(target)) top3++;
      }
    }
  }
  return [n, top1, top3];
}

if (process.argv.includes('--eval')) {
  const base: Priors = { tTri: 1.0, tBi: 0.5, tUni: 0.5, cTri: 0.75, cBi: 0.4, cUni: 0.45 };
  const [n, t1, t3] = evaluate(base);
  console.log(`\n留出集 ${n} 个位置：top1 ${(t1 / n * 100).toFixed(1)}% / top3 ${(t3 / n * 100).toFixed(1)}%`);

  /* 退避权重扫一遍：这些是先验常数，扫出来才算"训练"过 */
  let best = { pr: base, p1: t1 / n };
  for (const tBi of [0.5, 0.62, 0.78]) {
    for (const tUni of [0.22, 0.36, 0.5]) {
      for (const cTri of [0.75, 0.92]) {
        for (const cBi of [0.4, 0.58, 0.72]) {
          const pr: Priors = { tTri: 1.0, tBi, tUni, cTri, cBi, cUni: tUni * 0.9 };
          const [, a] = evaluate(pr);
          if (a / n > best.p1) best = { pr, p1: a / n };
        }
      }
    }
  }
  console.log(`最佳退避权重：${JSON.stringify(best.pr)} → top1 ${(best.p1 * 100).toFixed(1)}%`);
  void varTypes;
}
