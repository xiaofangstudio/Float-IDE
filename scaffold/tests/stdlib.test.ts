/* 标准库的守门测试（TS 引擎侧）。
 *
 * 两件事必须守住：
 * 1. **算对**：data / json / time / re / ui 这些纯计算的返回值。
 * 2. **报错是真的**：错参数、错类型、不存在的函数，必须当场报出来，
 *    而且文案要说人话（"需要数字，得到 x" 这种），不许静默给个 null 混过去。
 *
 * 跨引擎是否一致由 tests/parity/cases11.txt 对拍；这里只管 TS 这一侧的行为。
 */

import { run } from './harness';

let pass = 0;
let fail = 0;

function eq(name: string, got: unknown, want: unknown): void {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a === b) { pass++; return; }
  fail++;
  console.log(`✗ ${name}\n  期望 ${b}\n  实际 ${a}`);
}

function ok(name: string, cond: boolean, extra = ''): void {
  if (cond) { pass++; return; }
  fail++;
  console.log(`✗ ${name}${extra ? `\n  ${extra}` : ''}`);
}

async function outs(src: string): Promise<string[]> {
  const r = await run(src.endsWith('\n') ? src : src + '\n', { timeoutMs: 8000 });
  if (r.err.length) return [`<err>${r.err.join(' | ')}`];
  return r.out;
}

async function errOf(src: string): Promise<string> {
  const r = await run(src.endsWith('\n') ? src : src + '\n', { timeoutMs: 8000 });
  return r.err.join(' | ');
}

/* ── 1. data：排序 / 统计 / 集合操作 ── */
{
  const out = await outs(`import data
list 学生 = [{"name": "老陈", "score": 88}, {"name": "阿方", "score": 93}, {"name": "小北", "score": 78}]
for s in data.sort_by(学生, "score", "desc") {
  print(s["name"])
}
print(str(data.avg(学生, "score")))
print(str(data.median([1, 2, 3, 4])))
print(str(data.stdev([2, 4, 4, 4, 5, 5, 7, 9])))
print(str(data.sum([1, 2, 3])))
print(str(data.max(["a", "c", "b"])))
`);
  eq('排序按分数倒序', out.slice(0, 3), ['阿方', '老陈', '小北']);
  ok('平均分算对', Math.abs(Number(out[3]) - 86.333333333333) < 1e-9, out[3]);
  eq('中位数（偶数取中间两数平均）', out[4], '2.5');
  ok('样本标准差', Math.abs(Number(out[5]) - 2.13808993529939) < 1e-9, out[5]);
  eq('求和', out[6], '6');
  eq('非数字也能比大小（按文本）', out[7], 'c');
}

{
  const out = await outs(`import data
print(str(data.unique([1, 2, 2, 3, 1])))
print(str(data.freq(["a", "a", "b"])))
print(str(data.slice([1, 2, 3, 4, 5], -2)))
print(str(data.flatten([1, [2, [3]]])))
print(str(data.chunk([1, 2, 3], 2)))
print(str(data.zip([1, 2], ["a"])))
print(str(data.enumerate(["x"])))
print(str(data.entries({"k": 1})))
print(str(data.pick({"a": 1, "b": 2}, ["a"])))
print(str(data.omit({"a": 1, "b": 2}, ["a"])))
print(str(data.get({"a": 1}, "zz", 0)))
`);
  eq('去重', out[0], '[1, 2, 3]');
  eq('频次', out[1], '{a: 2, b: 1}');
  eq('负下标切片', out[2], '[4, 5]');
  eq('拍平只拍一层', out[3], '[1, 2, [3]]');
  eq('分块', out[4], '[[1, 2], [3]]');
  eq('zip 以短的为准', out[5], '[[1, "a"]]');
  eq('enumerate', out[6], '[[0, "x"]]');
  eq('entries', out[7], '[["k", 1]]');
  eq('pick', out[8], '{a: 1}');
  eq('omit', out[9], '{b: 2}');
  eq('get 默认值', out[10], '0');
}

/* ── 2. json ── */
{
  const out = await outs(`import json
var j = json.parse("{\\"a\\": [1, 2.5, true, null], \\"b\\": \\"中文\\"}")
print(j["b"])
print(json.stringify(j))
print(json.pretty({"x": 1}))
print(str(json.valid("1")))
print(str(json.valid("{\"a\": }")))
`);
  eq('json 读中文字段', out[0], '中文');
  eq('json 往返', out[1], '{"a":[1,2.5,true,null],"b":"中文"}');
  eq('pretty 带缩进', out[2], '{\n  "x": 1\n}');
  ok('valid 认数字', out[3] === 'true');
  ok('valid 认出坏 JSON', out[4] === 'false');
}

{
  const e = await errOf('import json\nprint(json.parse("{\\"a\\": }"))\n');
  ok('json.parse 坏文本要报错', e.includes('json.parse'), e);
  const out = await outs(`import json
var r = json.try_parse("{\\"a\\": }")
print(str(r["ok"]))
`);
  eq('try_parse 不炸，返回 ok=false', out[0], 'false');
}

/* ── 3. time：定死时间戳，结果必须是确定的 ── */
{
  const out = await outs(`import time
print(time.format(0, "YYYY-MM-DD HH:mm:ss"))
print(time.format(86400, "YYYY/MM/DD"))
print(str(time.diff(100, 160)))
print(str(time.add(1000, 60)))
print(str(time.parts(0)["year"]))
print(str(time.parts(0)["weekday"]))
`);
  ok('时间戳 0 能格式化', /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(out[0]), out[0]);
  eq('差 60 秒', out[2], '60');
  eq('加 60 秒', out[3], '1060');
  eq('1970 年', out[4], '1970');
  ok('星期几是 1~7', Number(out[5]) >= 1 && Number(out[5]) <= 7, out[5]);
}

/* ── 4. random：种子定死就必须每次都一样 ── */
{
  const src = `import random
random.seed(42)
print(str(random.rand(1, 100)))
print(str(random.rand(1, 100)))
`;
  const a = await outs(src);
  const b = await outs(src);
  eq('同一个种子，两次跑结果一样', a, b);
  ok('rand 落在区间里', Number(a[0]) >= 1 && Number(a[0]) <= 100, a[0]);
  const withSeed = await outs(`import random
random.seed(1)
list xs = [1, 2, 3]
random.shuffle(xs)
print(str(xs))
`);
  ok('shuffle 不改长度', withSeed[0].split(',').length === 3, withSeed[0]);
}

/* ── 5. re ── */
{
  const out = await outs(`import re
print(str(re.findall("[0-9]+", "a1b22c333")))
print(str(re.test("\\\\d+", "abc")))
print(re.replace("[0-9]+", "N", "a1b22"))
print(str(re.split(",", "a,b,c")))
print(str(re.findall("href=\\"([^\\"]+)\\"", "x href=\\"/a\\" y")))
print(re.escape("a.b"))
`);
  eq('findall 全部匹配', out[0], '["1", "22", "333"]');
  eq('test 不匹配返回 false', out[1], 'false');
  eq('replace 全替换', out[2], 'aNbN');
  eq('split', out[3], '["a", "b", "c"]');
  eq('有分组只要分组内容', out[4], '["/a"]');
  eq('escape', out[5], 'a\\.b');
}

/* ── 6. ui ── */
{
  const out = await outs(`import ui
print(ui.bar(3, 10, 10))
print(ui.color("red", "红"))
print(ui.progress(1, 4))
`);
  eq('进度条', out[0], '[███░░░░░░░]');
  ok('颜色带 ANSI 转义', out[1].includes('\u001b[31m'), JSON.stringify(out[1]));
  ok('progress 带回车和百分比', out[2].includes('\r') && out[2].includes('25%'), JSON.stringify(out[2]));
}

/* ── 7. os：不需要宿主的那些函数 ── */
{
  const out = await outs(`import os
print(os.basename("a/b/c.co"))
print(os.dirname("a/b/c.co"))
print(os.dirname("c.co"))
print(os.extname("a/b/c.co"))
print(os.extname("a/b/c"))
print(os.pathjoin("a/", "b", "c.co"))
print(os.abspath("x/y"))
`);
  eq('basename', out[0], 'c.co');
  eq('dirname', out[1], 'a/b');
  eq('没目录时 dirname 是空串', out[2], '');
  eq('extname', out[3], '.co');
  eq('没扩展名', out[4], '');
  eq('pathjoin', out[5], 'a/b/c.co');
  ok('abspath 不炸', typeof out[6] === 'string');
}

/* ── 8. 报错必须是真的、说人话 ── */
{
  const cases: [string, string, string][] = [
    ['data 里没有这个函数', 'import data\nprint(data.nothing(1))\n', '没有 nothing'],
    ['os 里没有这个函数', 'import os\nprint(os.nothing(1))\n', '没有 nothing'],
    ['avg 遇到非数字要报错', 'import data\nprint(str(data.avg(["a"])))\n', '需要数字'],
    ['sum 遇到非数字要报错', 'import data\nprint(str(data.sum([1, "x"])))\n', '不是数字'],
    ['chunk 长度必须大于 0', 'import data\nprint(str(data.chunk([1], 0)))\n', '大于 0'],
    ['空列表没有中位数', 'import data\nprint(str(data.median([])))\n', '空列表'],
    ['sort_by 字段不存在要报错', 'import data\nprint(str(data.sort_by([{"a": 1}, {"a": 2}], "zz")))\n', '没有字段'],
    ['is_empty 只认容器和字符串', 'import data\nprint(str(data.is_empty(1)))\n', '需要列表'],
    ['sample 抽得太多要报错', 'import random\nprint(str(random.sample([1, 2], 5)))\n', '数量不对'],
    ['weighted 数量要一样', 'import random\nprint(str(random.weighted([1], [1, 2])))\n', '数量要一样'],
    ['ui.color 不认识的颜色要报错', 'import ui\nprint(ui.color("皮卡丘色", "x"))\n', '不认识这个颜色'],
    ['re 模式写坏要报错', 'import re\nprint(str(re.test("([", "x")))\n', 're.test']
  ];
  for (const [name, src, want] of cases) {
    const e = await errOf(src);
    ok(name, e.includes(want), `实际报错：${e}`);
  }
}

/* ── 9. 需要宿主时给的是"桌面版"提示，不是假装成功 ── */
{
  const e = await errOf('import os\nprint(os.readfile("a.txt"))\n');
  ok('无宿主时 readfile 明确说桌面版功能', e.includes('桌面版'), e);
}

/* ── 9.5 老名字转发到同一份实现（不许两个引擎各写一份） ── */
{
  const pj = await outs('print(pathjoin("a/", "b"))\n');
  eq('老名字 pathjoin 走的是同一份实现', pj[0], 'a/b');
  const ex = await errOf('print(str(exists("nope.txt")))\n');
  ok('老名字 exists 在没有宿主时给桌面版提示而不是崩',
    ex.includes('exists') && ex.includes('桌面版'), ex);

  const e = await errOf('print(str(listdir(".")))\n');
  ok('老名字报错文案里用的是老名字', e.includes('listdir') && !e.includes('os.listdir'), e);
}

/* ── 10. os.exit 在 F6 里不会把 IDE 关掉，而是明确拒绝 ── */
{
  const e = await errOf('import os\nos.exit(0)\nprint("不该跑到这")\n');
  ok('os.exit 在 F6 里给出明确说明', e.includes('os.exit'), e);
}

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
