/* 压力探针：专找会让 IDE 崩掉的输入（深嵌套 / 深递归 / 超长代码 / 残缺语法）。
 * 这里每一条都是「用户真可能写出来」的东西，崩了就是真 bug。 */
import { parseProgram } from '../src/lang/parse';
import { Runner } from '../src/lang/interp';
import { analyze } from '../src/lang/analyze';
import { fmtVal } from '../src/lang/values';

let pass = 0;
let fail = 0;
const log = (ok: boolean, name: string, extra = ''): void => {
  if (ok) { pass++; console.log(`  ok   ${name}${extra ? '  — ' + extra : ''}`); }
  else { fail++; console.log(`  FAIL ${name}${extra ? '  — ' + extra : ''}`); }
};

async function run(src: string): Promise<{ out: string[]; err: string | null }> {
  const map = new Map<string, string>([['main.cw', src]]);
  (globalThis as any).__floatSources = map;
  const out: string[] = [];
  let err: string | null = null;
  await new Promise<void>((done) => {
    let settled = false;
    const finish = (): void => { if (!settled) { settled = true; done(); } };
    const r = new Runner('main.cw', {
      output: (l) => { if (l.kind === 'out') out.push(l.text); else if (l.kind === 'err') err = l.text; },
      onError: (e) => { err = e.message; },
      onDone: finish,
      onState: () => {},
      onPaused: (n, s) => { void n; void s; }
    });
    try { r.start('run'); } catch (e) { err = String(e); finish(); }
    setTimeout(finish, 20000);
  });
  return { out, err };
}

/* ── 1. 深嵌套括号 ── */
for (const d of [20, 60, 200, 600]) {
  const src = 'print ' + '('.repeat(d) + '1+2' + ')'.repeat(d) + '\n';
  try { parseProgram('main.cw', src); log(true, `括号嵌套 ${d} 层`); }
  catch (e) { log(false, `括号嵌套 ${d} 层`, String(e).slice(0, 70)); }
}

/* ── 2. 深嵌套缩进块 ── */
for (const d of [20, 60, 150]) {
  let src = '';
  for (let i = 0; i < d; i++) src += '  '.repeat(i) + 'if (1 == 1)\n' + '  '.repeat(i + 1) + `print ${i}\n`;
  try { parseProgram('main.cw', src); log(true, `缩进嵌套 ${d} 层`); }
  catch (e) { log(false, `缩进嵌套 ${d} 层`, String(e).slice(0, 70)); }
}

/* ── 3. 无限递归：必须优雅报错，不能爆栈崩进程 ── */
{
  const src = 'function f(n)\n    return f(n + 1)\n\nprint f(1)\n';
  const { err } = await run(src);
  log(err !== null, '无限递归被拦住（不崩进程）', (err ?? '未报错，危险！').slice(0, 70));
}

/* ── 4. 自引用容器：print 不能死循环 ── */
{
  const src = 'list a = [1]\nadd a to a\nprint(a)\nprint("ok")\n';
  const { out, err } = await run(src);
  log(err === null, '自引用列表不卡死', (out[0] ?? err ?? '无输出').slice(0, 60));
}

/* ── 5. 超长代码：300 函数 + 50 类 + 3000 变量 ── */
{
  let src = '';
  for (let i = 0; i < 300; i++) src += `function fn${i}(a, b)\n    if (a > b)\n        return a - b\n    else\n        return b - a\n`;
  for (let i = 0; i < 50; i++) src += `class C${i}\n    str name = "c${i}"\n    function get()\n        return self.name\n`;
  for (let i = 0; i < 3000; i++) src += `int v${i} = ${i} * 2\n`;
  src += 'print(fn0(3, 9))\n';
  src += 'C7 o = new C7()\nprint(o.get())\nprint(v2999)\n';
  const lines = src.split('\n').length;

  const t0 = Date.now();
  try { const p = parseProgram('main.cw', src); log(true, `超长代码解析（${lines} 行）`, `${p.body.length} 条语句 · ${Date.now() - t0}ms`); }
  catch (e) { log(false, `超长代码解析（${lines} 行）`, String(e).slice(0, 80)); }

  const t1 = Date.now();
  try { const a = analyze('main.cw', src); log(true, '超长代码静态分析', `${a.funcs.length} 函数 / ${a.classes.length} 类 · ${Date.now() - t1}ms`); }
  catch (e) { log(false, '超长代码静态分析', String(e).slice(0, 80)); }

  const t2 = Date.now();
  const { out, err } = await run(src);
  log(err === null && out.includes('6') && out.includes('c7') && out.includes('5998'),
    '超长代码执行结果正确', `${out.slice(0, 3).join(' | ')} · ${Date.now() - t2}ms${err ? ' ERR:' + err : ''}`);
}

/* ── 6. 大列表 ── */
{
  let src = 'list big = []\nfor i in range(20000)\n    add i to big\nprint(len(big))\nprint(big[19999])\n';
  const { out, err } = await run(src);
  log(err === null && out[0] === '20000' && out[1] === '19999', '2 万元素列表', `${out.join(',')}${err ? ' ERR:' + err : ''}`);
}

/* ── 7. 超长字符串拼接 ── */
{
  const src = 'str s = ""\nfor i in range(20000)\n    s = s + "x"\nprint(len(s))\n';
  const { out, err } = await run(src);
  log(err === null && out[0] === '20000', '2 万次字符串拼接', `${out.join(',')}${err ? ' ERR:' + err : ''}`);
}

/* ── 8. 残缺语法：必须给诊断，不能静默通过 / 不能崩 ── */
{
  const bads = ['print (', 'if\n', 'function f(', 'class A\n', 'list a = [', 'dict d = {',
    'print "未闭合', 'a = = 1', 'for\n', 'return\n', 'print )', 'function\n', 'class\n'];
  let silent = 0; let reported = 0;
  for (const b of bads) {
    try {
      const p = parseProgram('main.cw', b);
      if (p.diagnostics.length === 0) silent++; else reported++;
    } catch { reported++; }
  }
  log(silent === 0, '残缺语法全部有诊断', `${reported}/${bads.length} 有反馈，${silent} 静默通过`);
}

/* ── 9. fmtVal 环形结构 ── */
{
  try {
    const a: any[] = [1]; a.push(a);
    const s = fmtVal(a as any);
    log(s.length < 300, '环形结构格式化不死循环', s.slice(0, 40));
  } catch (e) { log(false, '环形结构格式化不死循环', String(e).slice(0, 70)); }
}

/* ── 10. 单文件 2 万行 ── */
{
  let src = '';
  for (let i = 0; i < 20000; i++) src += `int x${i} = ${i}\n`;
  src += 'print(x19999)\n';
  const t0 = Date.now();
  const { out, err } = await run(src);
  log(err === null && out[0] === '19999', '2 万行单文件可执行', `${Date.now() - t0}ms${err ? ' ERR:' + err : ''}`);
}

console.log(`\n压力探针：${pass} 通过 / ${fail} 失败`);
process.exit(fail > 0 ? 1 : 0);
