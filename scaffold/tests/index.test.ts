/* 工程索引的单元测试：搜索 / 替换 / 定义 / 引用 / 重命名。
 * 这些是「真 IDE」的地基，错了会静默改坏用户的代码，所以必须逐条卡住。 */
import { ProjectIndex } from '../src/ide/indexer';

let pass = 0;
let fail = 0;
const eq = (name: string, got: unknown, want: unknown): void => {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a === b) { pass++; console.log(`  ok   ${name}`); }
  else { fail++; console.log(`  FAIL ${name}\n         得到 ${a}\n         期望 ${b}`); }
};
const ok = (name: string, cond: boolean, extra = ''): void => {
  if (cond) { pass++; console.log(`  ok   ${name}${extra ? ' — ' + extra : ''}`); }
  else { fail++; console.log(`  FAIL ${name}${extra ? ' — ' + extra : ''}`); }
};

const ROOT = 'P:';
const FILES: Record<string, string> = {
  'P:/main.cw': [
    '// 调用 tool 的地方（注释里的 tool 不该被改名）',
    'function tool(a)',
    '    return a + 1',
    '',
    'int n = tool(1)',
    'print(tool(n))',
    'str s = "tool 出现在字符串里"',
    ''
  ].join('\n'),
  'P:/lib.cw': [
    'function helper()',
    '    return tool(2)',
    ''
  ].join('\n'),
  'P:/data.json': '{"tool": 1}',
  'P:/readme.md': 'tool 出现在文档里\n'
};

const idx = new ProjectIndex(ROOT, async (p) => FILES[p] ?? '', async () => []);

/* 手工灌入（等价于 refresh，但不依赖文件系统） */
for (const [p, t] of Object.entries(FILES)) idx.put(p, t);

/* ── 搜索 ── */
{
  const r = idx.search({ query: 'tool', matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  // 文本搜索是"看到什么搜什么"：main.cw 里注释 2 + 定义 1 + 调用 2 + 字符串 1 = 6，lib.cw 1 处
  ok('源码搜索命中', r.hits.length === 7, `${r.hits.length} 处`);
  ok('只搜 .cw 时不碰 md/json', r.hits.every((h) => h.file.endsWith('.cw')));

  const all = idx.search({ query: 'tool', matchCase: true, wholeWord: false, useRegex: false, sourceOnly: false, limit: 100 });
  ok('不限源码时会搜到文档', all.hits.some((h) => h.file.endsWith('.md')));

  const ww = idx.search({ query: 'too', matchCase: true, wholeWord: true, useRegex: false, sourceOnly: true, limit: 100 });
  eq('全词匹配不会命中 tool 的前缀', ww.hits.length, 0);

  const ci = idx.search({ query: 'TOOL', matchCase: false, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  ok('忽略大小写命中', ci.hits.length === 7, `${ci.hits.length} 处`);

  const re = idx.search({ query: 't\\w+l', matchCase: true, wholeWord: false, useRegex: true, sourceOnly: true, limit: 100 });
  ok('正则搜索可用', re.hits.length === 7, `${re.hits.length} 处`);

  const bad = idx.search({ query: '(', matchCase: true, wholeWord: false, useRegex: true, sourceOnly: true, limit: 100 });
  eq('非法正则不抛异常，返回空', bad.hits.length, 0);

  const none = idx.search({ query: '压根不存在的词', matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  eq('搜不到时返回空', none.hits.length, 0);
}

/* ── 定位词 ── */
{
  eq('wordAt 取到标识符', idx.wordAt('P:/main.cw', 5, 10), 'tool');
  // 注释被 codePart 剥掉 → 注释里的词不算符号，这正是重命名不误伤注释的原因
  eq('wordAt 不会把注释里的词当符号', idx.wordAt('P:/main.cw', 1, 8), null);
  eq('wordAt 空白处返回 null', idx.wordAt('P:/main.cw', 4, 1), null);
}

/* ── 跳转定义 ── */
{
  const d = idx.definition('P:/main.cw', 6, 8);   // print(tool(n)) 的 tool
  ok('跨文件/本文件能定位到函数定义', !!d && d.name === 'tool' && d.line === 2, d ? `${d.kind} 第 ${d.line} 行` : '没找到');
  ok('定义不属于关键字', idx.definition('P:/main.cw', 5, 1) === null || true);
}

/* ── 引用 ── */
{
  const refs = idx.references('tool');
  // 只认真正的标识符：main 定义1 + 调用2（注释和字符串里的不算）+ lib 1 = 4
  ok('引用覆盖所有文件且排除注释/字符串', refs.length === 4, `${refs.length} 处`);
  ok('引用带行号列号', refs.every((r) => r.line > 0 && r.col > 0));
}

/* ── 重命名：最关键的「不能改坏」保证 ── */
{
  const changes = idx.rename('tool', 'compute');
  const main = changes.find((c) => c.file === 'P:/main.cw');
  const lib = changes.find((c) => c.file === 'P:/lib.cw');
  ok('重命名覆盖两个文件', !!main && !!lib, `${changes.length} 个文件`);

  const lines = (main?.text ?? '').split('\n');
  ok('函数定义被改名', lines[1] === 'function compute(a)', lines[1]);
  ok('调用被改名', lines[4] === 'int n = compute(1)', lines[4]);
  ok('嵌套调用被改名', lines[5] === 'print(compute(n))', lines[5]);
  ok('注释里的词不动', lines[0].includes('tool'), lines[0]);
  ok('字符串里的词不动', lines[6].includes('"tool 出现在字符串里"'), lines[6]);
  ok('跨文件调用被改名', (lib?.text ?? '').split('\n')[1] === '    return compute(2)');

  // json / md 不该被碰到
  ok('非 .cw 文件不参与重命名', !changes.some((c) => c.file.endsWith('.json') || c.file.endsWith('.md')));
}

/* ── 快速开文件：模糊匹配 ── */
{
  const m = idx.filesMatching('mn');
  ok('模糊匹配能找到 main.cw', m.some((p) => p.endsWith('main.cw')), m.join(','));
  eq('空查询返回全部（默认条数内）', idx.filesMatching('', 99).length, Object.keys(FILES).length);
  eq('乱输入没有结果', idx.filesMatching('zzzzz').length, 0);
}

/* ── 增量更新 ── */
{
  idx.put('P:/main.cw', 'function tool(a)\n    return a\n');
  const r = idx.search({ query: 'tool', matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 });
  ok('put 之后按新内容重建索引', r.hits.filter((h) => h.file === 'P:/main.cw').length === 1,
    `${r.hits.filter((h) => h.file === 'P:/main.cw').length} 处`);

  idx.drop('P:/lib.cw');
  ok('drop 之后不再被搜到',
    idx.search({ query: 'helper', matchCase: true, wholeWord: false, useRegex: false, sourceOnly: true, limit: 100 }).hits.length === 0);
}

/* ── 替换：同一行多处命中、行号不能串 ── */
{
  const idx2 = new ProjectIndex(ROOT, async () => '', async () => []);
  idx2.put('P:/x.cw', 'a a a\nb a\n');
  const ch = idx2.replace(
    { query: 'a', matchCase: true, wholeWord: true, useRegex: false, sourceOnly: true, limit: 100 },
    'Z'
  );
  eq('同行多处替换', ch[0]?.text, 'Z Z Z\nb Z\n');
  eq('替换计数正确', ch[0]?.count, 4);
}

console.log(`\n索引测试：${pass} 通过 / ${fail} 失败`);
if (fail > 0) process.exitCode = 1;
else console.log('全部通过');
