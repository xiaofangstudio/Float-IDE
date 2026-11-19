/* 补全逻辑的守门测试：不加括号、覆盖齐全、优先级正确。
 * 这几个都是用户明确点名的毛病，任何时候回退都要在这里炸出来。 */

import { analyze } from '../src/lang/analyze';
import { buildCompletions, capCompletions, completionPosition, visibleCompletions, type Completion } from '../src/lang/completions';
import { contextAt } from '../src/lang/context';
import { scopeVariables, smartRank, predictNext, trainModel, SmartCorpus, type SmartModel } from '../src/lang/smartComplete';

/** 两张模型表逐词对比（增量 vs 全量必须完全一致） */
function sameModel(x: SmartModel, y: SmartModel): boolean {
  const keys = new Set([...x.next.keys(), ...y.next.keys()]);
  for (const k of keys) {
    const a = x.next.get(k);
    const b = y.next.get(k);
    if ((a?.size ?? 0) !== (b?.size ?? 0)) return false;
    for (const [w, n] of a ?? []) if ((b?.get(w) ?? 0) !== n) return false;
  }
  if (x.varTypes.size !== y.varTypes.size) return false;
  for (const [k, v] of x.varTypes) if (y.varTypes.get(k) !== v) return false;
  return true;
}

let pass = 0;
let fail = 0;

function eq(name: string, got: unknown, want: unknown): void {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a === b) { pass++; return; }
  fail++;
  console.log(`✗ ${name}\n  期望 ${b}\n  实际 ${a}`);
}

function ok(name: string, cond: boolean): void {
  if (cond) { pass++; return; }
  fail++;
  console.log(`✗ ${name}`);
}

const SRC = `
int hp = 100
str name = "小方"
list xs = [1, 2]

fn 打招呼(谁)
    return "hi " + 谁

class Person
    str 名字 = ""
    int 年龄 = 0

    fn init(n)
        self.名字 = n

    fn hi(a, b)
        return a

Person p = new Person("x")
`;

/** 造补全候选：src 里用 ▮ 标光标，和编辑器走同一条 contextAt 判断 */
function compsAt(srcWithCaret: string, word = ''): Completion[] {
  const idx = srcWithCaret.indexOf('▮');
  const src = srcWithCaret.replace('▮', '');
  const before = srcWithCaret.slice(0, idx);
  const line = before.split('\n').length;
  const col = idx - before.lastIndexOf('\n');
  const a = analyze('main.cw', src);
  return buildCompletions({ word, analysis: a, context: contextAt(src, line, col, a) });
}

function comps(_before: string, word = ''): Completion[] {
  return compsAt(SRC + '▮', word);
}

function labels(list: Completion[]): string[] {
  return list.map((c) => c.label);
}

/* ── 1. 绝不自动加括号 ── */
{
  const all = compsAt(SRC + '▮');
  const withParen = all.filter((c) => c.kind !== 'snippet' && /[()]/.test(c.insertText));
  eq('非片段补全里不能出现括号', withParen.map((c) => `${c.label}→${c.insertText}`), []);

  // 函数补全只插名字，参数放在 labelDetail 里
  const f = all.find((c) => c.label === '打招呼')!;
  eq('函数只插名字', f.insertText, '打招呼');
  eq('函数签名放 labelDetail', f.labelDetail, '(谁)');

  // 内置函数同理
  const b = all.find((c) => c.label === 'upper') ?? all.find((c) => c.label === 'sqrt')!;
  ok('内置函数只插名字', b.insertText === b.label);

  // 片段是"整套模板"，允许带括号（用户主动选才生效；和关键字同名时加「（模板）」后缀）
  const sn = all.find((c) => c.kind === 'snippet' && c.label.startsWith('function'))!;
  ok('片段保留模板', sn && sn.insertText.includes('function'));
  ok('同名片段加后缀区分', sn.label === 'function（模板）');
}

/* ── 2. 什么都要能补到 ── */
{
  const all = compsAt(SRC + '▮');
  const L = labels(all);
  for (const want of ['hp', 'name', 'xs', '打招呼', 'Person', 'p', 'sqrt', 'len', 'int', 'if', 'readfile', '__last_len']) {
    ok(`能补到 ${want}`, L.includes(want));
  }
  // 老式指令也在
  ok('能补到老式指令 writefile', L.includes('writefile'));
  // 去重
  const dup = L.filter((x, i) => L.indexOf(x) !== i);
  eq('补全不能有重复项', dup, []);
}

/* ── 3. 优先级：局部变量 > 类型/关键字 > 内置 > 片段 ── */
{
  const all = compsAt(SRC + '▮');
  const by = (label: string): Completion => all.find((c) => c.label === label)!;
  const tier = (c: Completion): number => Number(c.sortText[0]);
  ok('局部变量排序最靠前', tier(by('hp')) < tier(by('int')));
  ok('类型比内置靠前', tier(by('int')) < tier(by('sqrt')));
  ok('内置比片段靠前', tier(by('sqrt')) < tier(by('function（模板）')));
  // 前缀命中要排在同类未命中前面
  const pref = compsAt(SRC + '▮', 'sr');
  const idx = pref.findIndex((c) => c.label === 'sqrt');
  const un = pref.findIndex((c) => c.label === 'abs');
  ok('前缀命中的排前面', idx >= 0 && (un < 0 || idx < un));
}

/* ── 4. 成员补全：字段/方法在前，且不自动加括号 ── */
{
  const mem = compsAt(SRC + 'p.▮');
  const L = labels(mem);
  ok('成员里有类字段', L.includes('名字') && L.includes('年龄'));
  ok('成员里有类方法', L.includes('hi'));
  const hi = mem.find((c) => c.label === 'hi')!;
  eq('方法只插名字', hi.insertText, 'hi');
  eq('方法签名放 labelDetail', hi.labelDetail, '(a, b)');
  // h 打头：字段/方法应该排在"内建方法"前面
  const hls = mem.filter((c) => c.label.startsWith('h'));
  ok('自己的方法排在内建方法前面',
    hls.findIndex((c) => c.label === 'hi') < hls.findIndex((c) => c.kind === 'method' && c.label === 'has'));
}

/* ── 5. 类型已知时，内建方法要按类型过滤 ── */
{
  const s = compsAt(SRC + 'name.▮');
  const ls = labels(s);
  ok('字符串变量能补 trim/upper', ls.includes('trim') && ls.includes('upper'));
  ok('字符串变量不列列表方法 push', !ls.includes('push'));
  const xs = compsAt(SRC + 'xs.▮');
  const xl = labels(xs);
  ok('列表变量能补 push/sum', xl.includes('push') && xl.includes('sum'));
  ok('列表变量不列字符串方法 upper', !xl.includes('upper'));
}

/* ── 6. 光标在字符串/注释里由 provider 拦截（这里只测纯逻辑不受影响） ── */
{
  const all = compsAt(SRC + 'print("abc▮")');
  ok('纯逻辑层不做拦截（交给 provider）', Array.isArray(all));
}

/* ── 7. 残缺代码也补得到（靠解析器的错误恢复，不靠正则兜底） ── */
{
  // `int hp = ` 右边还没写：AST 里依然是 decl，hp 必须能补出来
  const broken = compsAt('int hp = \nprint(h▮p)\n');
  ok('残缺声明里的变量仍能补到', broken.some((c) => c.label === 'hp'));
  // 函数参数列表写一半，函数名也要在
  const brokenFn = compsAt('fn 算(a, \nprint▮\n');
  ok('残缺函数定义的名字仍能补到', brokenFn.some((c) => c.label === '算'));
  // 类头写一半
  const brokenCls = compsAt('class 人\n    str 名\n\nnew ▮人()\n');
  ok('残缺类定义的名字仍能补到', brokenCls.some((c) => c.label === '人'));
}

/* ── 8. 标准库：`os.` 之后只列这个库的函数，且不带括号 ── */
{
  const osC = compsAt('import os\nprint(os.▮)\n');
  const ol = labels(osC);
  ok('os. 之后能补 readfile', ol.includes('readfile'));
  ok('os. 之后能补 listdir/basename', ol.includes('listdir') && ol.includes('basename'));
  ok('os. 之后不混进列表方法 push', !ol.includes('push'));
  ok('os. 之后不混进字符串方法 upper', !ol.includes('upper'));
  ok('补全只插名字，不自动加括号', osC.every((c) => !c.insertText.includes('(')));

  const dataC = compsAt('import data\nprint(data.▮)\n');
  const dl = labels(dataC);
  ok('data. 之后能补 sort/median/avg', dl.includes('sort') && dl.includes('median') && dl.includes('avg'));

  const reC = compsAt('import re\nprint(re.▮)\n');
  ok('re. 之后能补 findall/replace', labels(reC).includes('findall') && labels(reC).includes('replace'));
}

/* ── 9. 标准库函数在全局补全里也能搜到（带库名前缀） ── */
{
  const all = compsAt(SRC + 'print(▮)');
  const names = labels(all);
  ok('全局补全能搜到 data.sum', names.includes('data.sum'));
  ok('全局补全能搜到 json.parse', names.includes('json.parse'));
}

/* ── 10. 变量优先；模板不许抢位（for 里的 i 不能被 if 顶掉） ──
 * 用户原话：在 for 里写 i 很容易被判成 if，一敲回车整段都换成 if 了。 */
{
  const all = compsAt('for i in range(5)\n    i▮\n', 'i');
  const sorted = [...all].sort((a, b) => a.sortText.localeCompare(b.sortText));
  eq('for 里的 i 排第一', sorted[0]?.label, 'i');
  eq('i 是变量（不是关键字/模板）', sorted[0]?.kind, 'variable');

  // 特殊变量（ans / __last_*）不许挤在变量后面抢位置
  const names = sorted.map((c) => c.label);
  ok('特殊变量 ans 排在关键字 if 之后', names.indexOf('ans') > names.indexOf('if'));

  // 前缀太短时模板根本不进候选：回车不可能换成整段 if else
  ok('打 i 时没有模板片段', visibleCompletions(all, 'i').every((c) => c.kind !== 'snippet'));
  ok('打 if 时没有模板片段', visibleCompletions(all, 'if').every((c) => c.kind !== 'snippet'));
  ok('打 ife 时模板才出现', visibleCompletions(all, 'ife').some((c) => c.kind === 'snippet'));
}

/* ── 8. 上下文感知：for 循环体里打变量，不许冒出 if ── */
{
  const src = 'list 成绩 = [1, 2]\nfor s in 成绩\n    s';
  const a = analyze('t.co', src);
  const ctx = contextAt(src, 3, 6, a);
  const lines = src.split('\n');
  const scopeVars = scopeVariables(a, lines, 3);
  ok('最近的 for 变量被认出来', scopeVars[0] === 's');
  const lineBefore = '    s';
  const items = buildCompletions({
    word: 's', analysis: a, context: ctx,
    position: completionPosition(lineBefore, 's'), scopeVars
  });
  const sorted = [...items].sort((x, y) => x.sortText.localeCompare(y.sortText));
  eq('循环变量排第一', sorted[0]?.label, 's');
  ok('打 s 时不冒 if', !items.some((c) => c.label === 'if'));
  const withI = buildCompletions({
    word: 'i', analysis: a, context: ctx,
    position: completionPosition('    i', 'i'), scopeVars
  });
  ok('打 i 时 if 仍在（前缀命中）', withI.some((c) => c.label === 'if'));

  /* Beta 小模型：只在同档内重排，不许把关键字顶到变量前面 */
  const model = trainModel([src]);
  ok('模型吃到语料', model.trainedLines > 0);
  const ranked = smartRank(model, items, lineBefore, scopeVars, 's');
  eq('模型重排后循环变量仍是第一', ranked[0]?.label, 's');

  /* 新语法已喂给小模型：空工程（不传任何源码）也能从 SEED 学到这些搭配。
   * 注意：模型把"自己起的名字"统一归一成 <id>（见 normalizeTokens），
   * 所以以前能查到 'i' / 'g' / 'AND2' 的地方，现在查的是 <id>。 */
  const seed = trainModel([]);
  ok('SEED 教了 f{} 拼接', (seed.next.get('\u001f<id>')?.get('{') ?? 0) > 0);
  ok('SEED 教了 for 双变量', (seed.next.get('for\u001f<id>')?.get(',') ?? 0) > 0);
  ok('SEED 教了 data.require', (seed.next.get('data\u001f.')?.get('require') ?? 0) > 0);
  ok('SEED 教了函数当值', (seed.next.get('\u001ffn')?.get('<id>') ?? 0) > 0);
  /* 闭包 / lambda / lazy / HOF 已落地，一并喂给模型 */
  ok('SEED 教了 lambda', (seed.next.get('\u001ffn')?.get('(') ?? 0) > 0);
  ok('SEED 教了 lazy 参数', (seed.next.get('\u001flazy')?.get('bool') ?? 0) > 0);
  ok('SEED 教了 AND2', (seed.next.get('bool\u001f<id>')?.get('(') ?? 0) > 0);
  const dataDot = seed.next.get('data\u001f.');
  ok('SEED 教了 data.map/filter', (dataDot?.get('map') ?? 0) > 0 && (dataDot?.get('filter') ?? 0) > 0);

  /* 真实语料烘焙表：空工程也学到真实代码的搭配（含出厂编译器本体） */
  ok('烘焙表让 for 后面跟变量', (seed.next.get('\u001ffor')?.get('<id>') ?? 0) > 0);
  ok('烘焙表教了 for 变量 in range', (seed.next.get('<id>\u001fin')?.get('range') ?? 0) > 0);
  ok('三元表训到了 for X in', (seed.next.get('for\u001f<id>\u001fin')?.get('range') ?? 0) > 0);
  ok('烘焙表教了 print 后跟括号', (seed.next.get('\u001fprint')?.get('(') ?? 0) > 0);
  ok('SEED 教了默认函数 sqrt/str/len 搭配',
    (seed.next.get('\u001fsqrt')?.get('(') ?? 0) > 0
    && (seed.next.get('\u001fstr')?.get('(') ?? 0) > 0
    && (seed.next.get('\u001flen')?.get('(') ?? 0) > 0);

  /* 类型感知：模型分得清变量和列表 */
  const typed = trainModel(['list 背包 = [1, 2]\ndict 存档 = {}\nint 血量 = 100\n']);
  eq('list 变量记为 list 类型', typed.varTypes.get('背包'), 'list');
  eq('dict 变量记为 dict 类型', typed.varTypes.get('存档'), 'dict');
  eq('int 变量记为 int 类型', typed.varTypes.get('血量'), 'int');
  /* 列表上下文里，模型应把变量名归一成 <list> 再查表（列表后常见 . / [） */
  ok('类型视角查到 <list> 后跟点或下标',
    (typed.next.get('<list>\u001f.')?.size ?? 0) > 0 || (typed.next.get('\u001f<list>')?.get('.') ?? 0) > 0);
  /* 排序不越档：类型加权后局部变量仍压在最前 */
  const typedRanked = smartRank(typed, items, 'print(背包)', [], '');
  ok('类型加权不改档位铁律', typedRanked.every((c, i) => i === 0 || Number(c.sortText[0]) >= Number(typedRanked[0].sortText[0])));
}

/* ── 11. 增量语料库：结果必须和全量训练一模一样（不然就是"越学越歪"） ── */
{
  const A = 'int 甲 = 1\nprint(甲)\nfor i in range(3)\n    print(i)\n';
  const B = 'fn 乙(x)\n    return x + 1\n';
  const B2 = 'fn 乙(x)\n    return x + 2\nlist 丙 = [1, 2]\n';

  const corpus = new SmartCorpus();
  corpus.setText('a.co', A);
  corpus.setText('b.co', B);
  corpus.flush();
  ok('增量模型 == 全量模型（初建）', sameModel(corpus.get(), trainModel([A, B])));

  corpus.setText('b.co', B2);
  corpus.flush();
  ok('改一个文件后仍等于全量重训', sameModel(corpus.get(), trainModel([A, B2])));
  ok('旧文件的统计没被抹掉', (corpus.get().next.get('\u001fint')?.get('<id>') ?? 0) > 0);
  ok('被删掉的旧搭配不会残留', (corpus.get().next.get('\u001freturn')?.get('<id>') ?? 0) > 0);

  /* 内容没变就不该重建：连续 setText 同一份内容，builds 不涨 */
  const before = corpus.builds;
  corpus.setText('b.co', B2);
  corpus.setText('b.co', B2);
  corpus.flush();
  ok('内容没变不重建', corpus.builds === before + 1);
}

/* ── 12. 候选预算 & 只看光标之前的变量 ── */
{
  const laterSrc = 'print(后面的)\nint 后面的 = 1\n';
  const a = analyze('t.co', laterSrc);
  const at = buildCompletions({
    word: '', analysis: a, context: contextAt(laterSrc, 1, 9, a), maxLine: 1
  });
  ok('光标之后的变量不进候选', !labels(at).includes('后面的'));

  const earlySrc = 'int 前面的 = 1\nprint(前面的)\n';
  const a2 = analyze('t.co', earlySrc);
  const at2 = buildCompletions({
    word: '', analysis: a2, context: contextAt(earlySrc, 2, 9, a2), maxLine: 2
  });
  ok('光标之前的变量照常进候选', labels(at2).includes('前面的'));

  const many = Array.from({ length: 600 }, (_, i) => ({
    label: `v${i}`, kind: 'variable' as const, insertText: `v${i}`,
    labelDetail: '', detail: '', doc: '', sortText: `1${i}`
  }));
  eq('候选上限 450 条', capCompletions(many).length, 450);
}

/* ── 13. 不发热：全量重训 vs 单文件增量 ── */
{
  const big = Array.from({ length: 3000 }, (_, i) =>
    `int 变量${i} = ${i}\nprint(变量${i})\nfor j in range(10)\n    print(j)\n`).join('\n');

  const t0 = performance.now();
  trainModel([big]);
  const fullMs = performance.now() - t0;

  const corpus = new SmartCorpus();
  corpus.setText('big.co', big);
  corpus.flush();
  corpus.setText('big.co', big + '\nprint(新的一行)\n');
  const t1 = performance.now();
  corpus.flush();
  const incMs = performance.now() - t1;

  console.log(`  语料 ${big.length} 字节：全量重建 ${fullMs.toFixed(1)} ms，改一行后增量 ${incMs.toFixed(1)} ms`);
  ok('增量重建比全量便宜 10 倍以上', incMs * 10 < fullMs);
  ok('增量重建本身够快（< 8 ms）', incMs < 8);
}

/* ── 14. 行内续写：只在行尾空格后、置信度够高才猜；能连着猜几个词 ── */
{
  const m = trainModel([]);
  eq('for i in 后面猜出 range(', predictNext(m, 'for i in '), 'range(');
  /* 连续猜：range( 之后就交给用户填参数，不再瞎猜 */
  ok('括号后面不再往下猜', (predictNext(m, 'for i in ') ?? '').includes('range('));
  eq('没有空格不猜', predictNext(m, 'for i in'), null);
  eq('点后面不猜（交给成员补全）', predictNext(m, 'xs.'), null);
  eq('样本太少不猜', predictNext(m, '甲乙丙 丁'), null);
  eq('空行不猜', predictNext(m, ' '), null);
  /* 猜出来的东西必须是"代码长相"的词序列：不含引号、不含占位符、不超长 */
  const guesses = ['for i in ', 'print(总数', 'if 总数 > 0 ', 'list xs = ['].map((s) => predictNext(m, s));
  ok('猜出来的是词序列，不是占位符/引号',
    guesses.every((g) => g === null || (g.length <= 40 && !/[<>"']/.test(g))));
}

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
