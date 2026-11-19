/* 补全热路径基准：一次"击键 → 弹候选"到底花多少毫秒。
 *
 * 盯三件事（都是"发烫"的来源）：
 *   1. 光标语义 + 候选构建 + 智能重排的合计；
 *   2. 智能模型：老路径（整工程重训）vs 新路径（改一块只重算那一块）；
 *   3. 大文件下 getValue/split 的重复开销（现在按版本缓存，只取一次）。
 *
 * 跑法：npm run bench:complete
 */

import { readFileSync } from 'node:fs';
import { analyze } from '../src/lang/analyze';
import { buildCompletions, capCompletions, completionPosition, visibleCompletions } from '../src/lang/completions';
import { contextAt } from '../src/lang/context';
import { scopeVariables, smartRank, trainModel, SmartCorpus } from '../src/lang/smartComplete';

const GAME = '../language/samples/main.co';
let src: string;
try {
  src = readFileSync(GAME, 'utf8');
} catch {
  src = Array.from({ length: 1200 }, (_, i) => `int 变量${i} = ${i}\nprint(变量${i})`).join('\n');
}
const lines = src.split('\n');

/** 挑一个"函数体里、表达式位置"的行来模拟真实补全 */
function pickLine(): number {
  for (let i = lines.length - 1; i > 0; i--) {
    const t = lines[i].trim();
    if (t.length > 12 && !t.startsWith('/*') && /^[a-zA-Z\u4e00-\u9fa5_]/.test(t)) return i + 1;
  }
  return 2;
}
const L = pickLine();
const col = Math.min(lines[L - 1].length, 40);

function bench(name: string, n: number, fn: () => void): number {
  fn(); // 预热
  const t0 = performance.now();
  for (let i = 0; i < n; i++) fn();
  const ms = (performance.now() - t0) / n;
  console.log(`  ${name.padEnd(34)} ${ms.toFixed(3)} ms/次`);
  return ms;
}

console.log(`语料：${GAME}（${src.length} 字节 / ${lines.length} 行）`);

/* 1. 分析（真实 IDE 里按版本缓存，一次编辑只发生一次） */
const t0 = performance.now();
const a = analyze('main.co', src);
const analyzeMs = performance.now() - t0;
console.log(`  全文件分析（每次编辑一次）        ${analyzeMs.toFixed(1)} ms`);

const tSym = performance.now();
const sym = analyze('main.co', src, undefined, { symbolsOnly: true });
const symbolMs = performance.now() - tSym;
console.log(`  符号表分析（补全用）              ${symbolMs.toFixed(1)} ms（省 ${(100 - (symbolMs / analyzeMs) * 100).toFixed(0)}%）`);
if (sym.vars.length !== a.vars.length) console.log('  ⚠ 符号表与全量分析的变量数不一致');

const corpus = new SmartCorpus();
corpus.setText('main.co', src);
corpus.flush();

const scopeVars = scopeVariables(a, lines, L);
const lineBefore = (lines[L - 1] ?? '').slice(0, col - 1);
const ctx = contextAt(src, L, col, a, lines);
const word = '';

const perKey = bench('击键 → 候选+重排（缓存命中）', 500, () => {
  const c = contextAt(src, L, col, a, lines);
  const items = buildCompletions({ word, analysis: a, context: c, position: completionPosition(lineBefore, word), scopeVars, maxLine: L });
  const vis = visibleCompletions(items, word);
  const ranked = smartRank(corpus.get(), vis, lineBefore, scopeVars, word);
  capCompletions(ranked);
});
void lines;

/* 2. 模型更新：老路径 vs 新路径 */
const full = bench('模型：整工程全量重训（老路径）', 5, () => { trainModel([src]); });

corpus.setText('main.co', src + '\nprint("新加一行")\n');
const inc = bench('模型：改一行后增量（新路径）', 20, () => {
  corpus.setText('main.co', src + `\nprint("${Math.random()}")\n`);
  corpus.flush();
});

console.log(`  → 模型更新便宜了 ${(full / Math.max(inc, 0.001)).toFixed(0)} 倍`);

console.log(`\n合计：一次补全 ${perKey.toFixed(2)} ms，改一行 ${inc.toFixed(2)} ms`);
if (perKey > 4 || inc > 4) {
  console.log('✗ 超出预算（补全 > 4 ms 或增量 > 4 ms）');
  process.exit(1);
}
console.log('✓ 在预算内');
