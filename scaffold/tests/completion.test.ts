/* 补全逻辑的守门测试：不加括号、覆盖齐全、优先级正确。
 * 这几个都是用户明确点名的毛病，任何时候回退都要在这里炸出来。 */

import { analyze } from '../src/lang/analyze';
import { buildCompletions, completionPosition, visibleCompletions, type Completion } from '../src/lang/completions';
import { contextAt } from '../src/lang/context';
import { scopeVariables, smartRank, trainModel } from '../src/lang/smartComplete';

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
}

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
