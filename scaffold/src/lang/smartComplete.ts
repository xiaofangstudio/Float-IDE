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

const SEP = '\u001f';

export type SmartModel = {
  /** "上上个词␟上一个词" → { 下一个词: 出现次数 }；空串表示该位置没有词 */
  next: Map<string, Map<string, number>>;
  trainedLines: number;
};

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
`;

const TOKEN_RE = /[A-Za-z_\u4e00-\u9fa5][A-Za-z0-9_\u4e00-\u9fa5]*|\d+(?:\.\d+)?|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[=+\-*/%,.()[\]{}:<>!]+/g;

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

/** 用工程源码训练（重复调用就是重建，不做增量，避免旧代码一直污染模型） */
export function trainModel(sources: string[]): SmartModel {
  const next = new Map<string, Map<string, number>>();
  let trainedLines = 0;
  const learn = (src: string): void => {
    for (const raw of src.split('\n')) {
      const cut = raw.indexOf('/*');
      const line = (cut >= 0 ? raw.slice(0, cut) : raw).trim();
      if (!line) continue;
      const ts = tokenize(line);
      trainedLines++;
      for (let i = 0; i < ts.length; i++) {
        bump(next, `${ts[i - 2] ?? ''}${SEP}${ts[i - 1] ?? ''}`, ts[i]);
        bump(next, `${SEP}${ts[i - 1] ?? ''}`, ts[i]);
      }
    }
  };
  learn(SEED);
  for (const s of sources) learn(s);
  return { next, trainedLines };
}

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

/** 同一档内按上下文重排；返回新数组，不改原候选 */
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
  const hit2 = model.next.get(`${p2}${SEP}${p1}`);
  const hit1 = model.next.get(`${SEP}${p1}`);
  const scope = new Set(scopeVars);
  const lowWord = word.toLowerCase();
  const tierOf = (it: Completion): number => Number(it.sortText[0] ?? '9');

  return items
    .map((it) => {
      const n = (hit2?.get(it.label) ?? 0) * 2 + (hit1?.get(it.label) ?? 0);
      let boost = 0;
      if (scope.has(it.label)) boost += 6;
      if (word && it.label.toLowerCase().startsWith(lowWord)) boost += 2;
      if (n > 0) boost += Math.min(6, 1 + Math.log2(1 + n));
      return { it, boost };
    })
    .sort((x, y) =>
      tierOf(x.it) - tierOf(y.it)
      || y.boost - x.boost
      || x.it.sortText.localeCompare(y.it.sortText))
    .map((x) => x.it);
}
