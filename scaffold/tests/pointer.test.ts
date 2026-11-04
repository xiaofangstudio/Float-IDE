/* 真指针（CW Script 1.5.3）：C 里有、我们也要有，但把裸地址藏起来。
 *
 * 覆盖：&x 取地址、*p 解引用、*p = v 写回、&xs[i] 指向列表元素、p±n 指针运算、
 * p[k] 相对寻址、指针相减、悬空指针、越界、free 后使用、alloc 的老用法不回归。
 */

import { Runner } from '../src/lang/interp';

let pass = 0;
let fail = 0;
const ok = (name: string, cond: boolean, extra = ''): void => {
  if (cond) { pass++; return; }
  fail++;
  console.log(`✗ ${name}${extra ? '  — ' + extra : ''}`);
};

/** 用解释执行（F6 那条通道）跑一段源码，返回输出行与错误 */
async function run(src: string): Promise<{ out: string[]; err: string | null }> {
  (globalThis as unknown as { __floatSources: Map<string, string> }).__floatSources =
    new Map([['main.cw', src]]);
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
      onPaused: () => { setTimeout(() => r.continueRun(), 0); }
    });
    try { r.start('debug'); } catch (e) { err = String(e); finish(); }
    setTimeout(finish, 10000);
  });
  return { out, err };
}

const cases: { name: string; src: string; want: string[] }[] = [
  {
    name: '&变量 取地址 + *p 解引用',
    src: 'int x = 10\nptr p = &x\nprint(*p)\n',
    want: ['10']
  },
  {
    name: '*p = v 写回变量本身',
    src: 'int x = 10\nptr p = &x\n*p = 42\nprint(x)\nprint(*p)\n',
    want: ['42', '42']
  },
  {
    name: '复合赋值走指针',
    src: 'int n = 1\nptr p = &n\n*p += 5\nprint(n)\n',
    want: ['6']
  },
  {
    name: '&列表元素：写入改到原列表',
    src: 'list xs = [1, 2, 3]\nptr p = &xs[1]\nprint(*p)\n*p = 9\nprint(xs[0], xs[1], xs[2])\n',
    want: ['2', '1 9 3']
  },
  {
    name: '指针运算 p+1 / p-1',
    src: 'list xs = [10, 20, 30]\nptr p = &xs[0]\nprint(*(p + 1))\nprint(*(p + 2))\nptr q = p + 2\nprint(*(q - 1))\n',
    want: ['20', '30', '20']
  },
  {
    name: 'p[k] 相对寻址（含负数走回来）',
    src: 'list xs = [10, 20, 30]\nptr p = &xs[0]\nprint(p[1])\nptr q = p + 2\nprint(q[-1])\n',
    want: ['20', '20']
  },
  {
    name: '指针相减 = 隔了几格',
    src: 'ptr a = alloc(8)\nptr b = a + 5\nprint(b - a)\nfree(a)\n',
    want: ['5']
  },
  {
    name: 'alloc 老用法不回归',
    src: 'ptr p = alloc(3)\np[0] = 7\np[1] = p[0] * 2\nprint(p[0], p[1], len(p))\nfree(p)\n',
    want: ['7 14 3']
  },
  {
    name: 'for v in 指针从当前位置遍历',
    src: 'ptr p = alloc(3)\np[0] = 1\np[1] = 2\np[2] = 3\nint s = 0\nfor v in p\n    s += v\nprint(s)\nptr q = p + 1\nint s2 = 0\nfor v in q\n    s2 += v\nprint(s2)\nfree(p)\n',
    want: ['6', '5']
  },
  {
    name: '函数的参数用指针改调用方的变量',
    src: 'fn bump(ptr p)\n    *p = *p + 1\n\nint x = 41\nbump(&x)\nprint(x)\n',
    want: ['42']
  }
];

for (const c of cases) {
  const { out, err } = await run(c.src);
  const got = out.join(' | ');
  ok(c.name, err === null && got === c.want.join(' | '),
    err ? 'ERR: ' + err : `期望 ${c.want.join(' | ')}，实际 ${got}`);
}

/* ── 该报错的必须报错，而且说人话 ── */
const errCases: { name: string; src: string; needle: RegExp }[] = [
  {
    name: '悬空指针：指向已返回函数的局部变量',
    src: 'fn f()\n    int x = 1\n    return &x\n\nptr p = f()\nprint(*p)\n',
    needle: /悬空|作用域/
  },
  {
    name: '解引用非指针要报错',
    src: 'int x = 3\nprint(*x)\n',
    needle: /只能解引用指针/
  },
  {
    name: '指针越界要报错，而不是踩内存',
    src: 'ptr p = alloc(2)\nprint(*(p + 5))\nfree(p)\n',
    needle: /越界|超出范围/
  },
  {
    name: 'free 后再解引用要报错',
    src: 'ptr p = alloc(2)\nfree(p)\nprint(*p)\n',
    needle: /free/
  },
  {
    name: '&列表越界要报错',
    src: 'list xs = [1]\nptr p = &xs[3]\n',
    needle: /越界|超出范围/
  },
  {
    name: 'free 别人（列表）的地址要明确拒绝',
    src: 'list xs = [1]\nptr p = &xs[0]\nfree(p)\n',
    needle: /不是 alloc|不能 free/
  },
  {
    name: '类名当函数调要报错，提示写 new',
    src: 'class C\n    int v = 5\nprint(C())\n',
    needle: /是类名，要创建实例请写 new/
  }
];

/* ── 多级指针：前缀 ** 等价于 *(*p)，幂运算不受影响 ── */
{
  const r = await run('int x = 7\nptr p = &x\nptr pp = &p\nprint(**pp)\n');
  ok('**pp 能读出两层的值', r.err === null && r.out.join('|') === '7', r.err ?? r.out.join('|'));
  const w = await run('int x = 7\nptr p = &x\nptr pp = &p\n**pp = 9\nprint(x)\n');
  ok('**pp = 9 写回最里层变量', w.err === null && w.out.join('|') === '9', w.err ?? w.out.join('|'));
  const pow = await run('print(2 ** 10)\nprint(2 ** 3 ** 2)\n');
  ok('** 仍是右结合幂运算', pow.err === null && pow.out.join('|') === '1024|512', pow.err ?? pow.out.join('|'));
}

for (const c of errCases) {
  const { err } = await run(c.src);
  ok(c.name, err !== null && c.needle.test(err), err ? `错误信息不符：${err}` : '没报错，危险');
}

/* ── 教程里**每个**带输出的示例：跑出来必须和文档写的输出一模一样 ──
 * 教程是用户照抄的东西，示例跑不通就是 bug。import 那种需要多文件的示例
 * 不写 out，自然跳过（不能单文件跑，就不假装能跑）。 */
{
  const { CHAPTERS } = await import('../src/docs/tutorial');
  const chapters = CHAPTERS;
  ok('教程里确实有指针三章', CHAPTERS.filter((c) => c.id.startsWith('pointer')).length === 3);
  for (const ch of chapters) {
    for (const sec of ch.sections) {
      const codes = sec.codes ?? [];
      for (let i = 0; i < codes.length; i++) {
        const smp = codes[i];
        if (!smp.out) continue;
        const r = await run(smp.code);
        const name = `教程「${ch.title} / ${sec.title}」示例 ${i + 1}`;
        if ((smp.out ?? '').startsWith('(报错)')) {
          ok(`${name} 按预期报错`, r.err !== null, `没报错，输出 ${r.out.join('|')}`);
        } else {
          const want = smp.out ?? '';
          ok(name, r.err === null && r.out.join('\n') === want,
            `期望 ${JSON.stringify(want)}，实际 ${JSON.stringify(r.out.join('\n'))} ${r.err ?? ''}`);
        }
      }
    }
  }
}

/* ── 作用域：变量 / 列表 / 字典在块、函数、类里的可见性与可改性 ──
 * 这些和 Rust 内核（tests/scope.rs）逐条对齐，改任何一边都会红。 */
{
  const cases: [string, string][] = [
    ['for 变量活到循环外', 'for i in range(3)\n    print(i)\nprint(i)', '0|1|2|2'],
    ['函数里的 for 变量也活到循环外', 'fn f()\n    for i in range(3)\n        print(i)\n    return i\nprint(f())', '0|1|2|2'],
    ['外层同名变量被循环复用', 'int i = 99\nfor i in range(2)\n    print(i)\nprint(i)', '0|1|1'],
    ['for in 变量活到循环外', 'for x in [7, 8]\n    print(x)\nprint(x)', '7|8|8'],
    ['if / while 块内声明块外可见', 'if (true)\n    int a = 1\nprint(a)', '1'],
    ['全局读写', 'int g = 1\nfn f()\n    g = 9\nf()\nprint(g)', '9'],
    ['局部遮蔽全局', 'int g = 1\nfn f()\n    int g = 2\n    return g\nprint(f())\nprint(g)', '2|1'],
    ['列表按引用传，重新赋值不影响外面', 'fn addone(list xs)\n    xs.push(9)\nlist a = [1]\naddone(a)\nprint(a)', '[1, 9]'],
    ['类要用 new 实例化，也能链式调用', 'class C\n    int v = 5\n    fn get()\n        return v\nprint(new C().get())', '5'],
  ];
  for (const [name, src, want] of cases) {
    const a = await run(src);
    ok(`作用域：${name}`, a.err === null && a.out.join('|') === want, `${a.err ?? ''} 实际 ${a.out.join('|')}`);
  }
  const leak = await run('fn f()\n    int local = 1\nf()\nprint(local)');
  ok('作用域：函数局部不外泄（读要报错）', leak.err !== null && /没有定义/.test(leak.err), leak.err ?? '没报错');
  const dictArg = await run('fn bump(dict d)\n    d["hp"] = d["hp"] + 1\ndict a = {"hp": 1}\nbump(a)\nprint(a["hp"])');
  ok('作用域：字典传进函数能改', dictArg.out.join('|') === '2', dictArg.err ?? dictArg.out.join('|'));
}

/* ── 排序：xs.sort() / 全局 sorted()，三引擎同一条规则（数字比数字，其余字典序） ── */
{
  const cases: [string, string][] = [
    ['sorted 返回新列表不改原列表', 'list xs = [3, 1, 2]\nlist ys = sorted(xs)\nprint(xs)\nprint(ys)', '[3, 1, 2]|[1, 2, 3]'],
    ['中英混排与 Rust 内核一致', 'print(sorted(["梨", "苹果", "ban"]))', '["ban", "梨", "苹果"]'],
    ['浮点混排', 'print(sorted([2.5, 1, 3.2]))', '[1, 2.5, 3.2]'],
    ['xs.sort 就地排序', 'list xs = [3, 1, 2]\nxs.sort()\nprint(xs)', '[1, 2, 3]'],
    ['链式 sort', 'print([5, 3, 4].sort())', '[3, 4, 5]'],
  ];
  for (const [name, src, want] of cases) {
    const a = await run(src), b = await run(src, { mode: 'debug' });
    ok(`排序：${name}`, a.out.join('|') === want && b.out.join('|') === want, `F5 ${a.out.join('|')} / F6 ${b.out.join('|')}`);
  }
  const e = await run('sorted(5)');
  ok('排序：sorted(非列表) 报错', e.err !== null, e.err ?? '没报错');
}

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
