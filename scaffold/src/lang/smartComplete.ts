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

/** 用工程源码训练（重复调用就是重建，不做增量，避免旧代码一直污染模型）。
 *  训练语料 = 内置方岛物语烘焙表 + 内置 SEED + 当前工程源码。
 *  烘焙表是"真实游戏代码"的搭配统计，SEED 是语言新特性样例，工程源码是你的习惯——
 *  三者合成一张表，空工程也有靠谱的起始预测。 */
export function trainModel(sources: string[]): SmartModel {
  const next = new Map<string, Map<string, number>>();
  const varTypes = new Map<string, string>();
  let trainedLines = 0;

  /* 方岛物语烘焙表：key 里已经带 <list>/<dict> 这类类型标注，直接并进来。
   * 它的计数已封顶 255，权重温和；下面工程源码 /
   * SEED 的原始计数会自然盖过它，所以你自己的写法仍然优先。 */
  for (const key of Object.keys(BAKED_NGRAM)) {
    let m = next.get(key);
    if (!m) { m = new Map(); next.set(key, m); }
    for (const [word, count] of BAKED_NGRAM[key]) {
      m.set(word, (m.get(word) ?? 0) + count);
    }
  }

  const learn = (src: string): void => {
    for (const raw of src.split('\n')) {
      const cut = raw.indexOf('/*');
      const line = (cut >= 0 ? raw.slice(0, cut) : raw).trim();
      if (!line) continue;
      const decl = DECL_RE.exec(line);
      if (decl) varTypes.set(decl[2], decl[1]);
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
  return { next, trainedLines, varTypes };
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

/** 把 token 归一成"类型标注"：变量名换成 <list>/<dict>…，分化出变量和列表。
 *  字符串/数字/运算符原样保留。这样查表时用的是 `<list>` 的真实用法统计，
 *  而不是某个具体变量名的偶发搭配。 */
function typed(t: string, varTypes: Map<string, string>): string {
  const ty = varTypes.get(t);
  return ty ? `<${ty}>` : t;
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
  const hit2 = model.next.get(`${p2}${SEP}${p1}`);
  const hit1 = model.next.get(`${SEP}${p1}`);

  // 类型视角：把最近两个 token 归一（for i → for i；成绩 → <list>）
  const tp1 = typed(p1, model.varTypes);
  const tp2 = typed(p2, model.varTypes);
  const typedHit2 = tp1 === p1 && tp2 === p2 ? undefined : model.next.get(`${tp2}${SEP}${tp1}`);
  const typedHit1 = tp1 === p1 ? undefined : model.next.get(`${SEP}${tp1}`);

  const scope = new Set(scopeVars);
  const lowWord = word.toLowerCase();
  const tierOf = (it: Completion): number => Number(it.sortText[0] ?? '9');

  return items
    .map((it) => {
      // 候选是变量时，也用它自己的类型标注再查一次（如候选 push 不会命中，
      // 但候选若正好是某个 list 变量，typedHit 让它在合适上下文里加分）
      const candType = model.varTypes.get(it.label);
      const candTyped = candType ? `<${candType}>` : it.label;
      const rawN = (hit2?.get(it.label) ?? 0) * 2 + (hit1?.get(it.label) ?? 0);
      const typedN = (typedHit2?.get(it.label) ?? 0) * 2 + (typedHit1?.get(it.label) ?? 0)
        + ((typedHit2?.get(candTyped) ?? 0) + (typedHit1?.get(candTyped) ?? 0));
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
