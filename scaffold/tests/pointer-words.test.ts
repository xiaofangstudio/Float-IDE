/* 指针新写法（addr / read / write）的守门测试。
 *
 * 规则很简单，但每一条都得真的成立：
 *   · addr(x) 取地址，addr(xs, i) 取列表第 i 格
 *   · 地址能打印（0x 开头）、能加减、能比较
 *   · read 读、write 写，越界 / free 之后照样报错
 *   · 老的 &x / *p 仍然能跑，只是 IDE 会提示"建议换新写法"
 *   · nil / none / out 已经取消，报错要说清改成什么
 */

import { run } from './harness';
import { analyze } from '../src/lang/analyze';

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

/* ── 1. 基本读写 ── */
{
  const out = await outs(`ptr a = alloc(4)
write(a, 42)
print(str(read(a)))
ptr b = a + 1
write(b, 7)
print(str(read(b)))
print(str(read(a)))
print(str(b - a))
free(a)
`);
  eq('write 再 read 拿到 42', out[0], '42');
  eq('指针 + 1 到下一格', out[1], '7');
  eq('原来的格子没被改', out[2], '42');
  eq('两个地址差 1 格', out[3], '1');
}

/* ── 2. 取变量地址 ── */
{
  const out = await outs(`int n = 5
addr p = addr(n)
write(p, 9)
print(str(n))
print(str(type(p)))
`);
  eq('通过地址改回变量本身', out[0], '9');
  eq('类型是 ptr', out[1], 'ptr');
}

/* ── 3. 取列表元素地址 ── */
{
  const out = await outs(`list xs = [1, 2, 3]
addr q = addr(xs, 1)
write(q, 99)
print(str(xs))
write(q + 1, 100)
print(str(xs))
`);
  eq('改列表第 1 格', out[0], '[1, 99, 3]');
  eq('地址 + 1 到第 2 格', out[1], '[1, 99, 100]');
}

/* ── 4. 地址能打印、能比较 ── */
{
  const out = await outs(`ptr a = alloc(2)
print(str(a))
print(str(a == a))
print(str(a == a + 1))
free(a)
`);
  ok('地址打印成 0x 开头', /^0x[0-9A-F]{12}$/.test(out[0]), out[0]);
  eq('同一个地址相等', out[1], 'true');
  eq('不同格子不相等', out[2], 'false');
}

/* ── 5. 越界 / 释放后访问，照样报错 ── */
{
  const e1 = await errOf('ptr a = alloc(2)\nwrite(a + 5, 1)\nfree(a)\n');
  ok('越界写要报错', e1.includes('越界'), e1);
  const e2 = await errOf('ptr a = alloc(2)\nfree(a)\nprint(str(read(a)))\n');
  ok('free 之后读要报错', e2.includes('free'), e2);
  const e3 = await errOf('int n = 1\nfree(addr(n))\n');
  ok('不是 alloc 的地址不能 free', e3.includes('free'), e3);
}

/* ── 6. write 只能当语句用，参数不对要说清楚 ── */
{
  const e = await errOf('ptr a = alloc(1)\nwrite(a)\nfree(a)\n');
  ok('write 少一个参数要报错', e.includes('write'), e);
}

/* ── 7. nil / none / out 已取消，报错要给出改法 ── */
{
  const e1 = await errOf('print(str(nil))\n');
  ok('nil 报错并提示改 null', e1.includes('null') && e1.includes('nil'), e1);
  const e2 = await errOf('print(str(none))\n');
  ok('none 报错并提示改 null', e2.includes('null') && e2.includes('none'), e2);
  const e3 = await errOf('while (true)\n    out\n');
  ok('out 报错并提示改 break', e3.includes('break'), e3);
}

/* ── 8. 老写法仍然能跑（兼容），但 IDE 会提示换新写法 ── */
{
  const out = await outs(`int n = 5
ptr p = &n
print(str(*p))
`);
  eq('&x / *p 还能跑', out[0], '5');

  const diags = analyze('main.co', 'int n = 5\nptr p = &n\nprint(str(*p))\n').diagnostics;
  const legacy = diags.filter((d) => d.code === 'legacy');
  ok('&x 会给出兼容提示', legacy.some((d) => d.message.includes('addr(')), JSON.stringify(legacy));
  ok('*p 会给出兼容提示', legacy.some((d) => d.message.includes('read(')), JSON.stringify(legacy));
}

/* ── 9. add ... to ... 也会提示（老写法不删，只是引导） ── */
{
  const diags = analyze('main.co', 'list xs = [1]\nadd 2 to xs\nprint(str(xs))\n').diagnostics;
  ok('add to 会给出兼容提示', diags.some((d) => d.code === 'legacy' && d.message.includes('push')), JSON.stringify(diags));
  // 新写法不该有任何提示
  const clean = analyze('main.co', 'list xs = [1]\nxs.push(2)\nprint(str(xs))\n').diagnostics;
  eq('新写法没有兼容提示', clean.filter((d) => d.code === 'legacy').length, 0);
}

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
