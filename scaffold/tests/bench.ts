/* 性能基准：编译通道 / 解释通道 / CPython 三方对比。
 * 运行：npm run bench:lang
 */

import { parseProgram } from '../src/lang/parse';
import { compileProgram, CompiledRuntime } from '../src/lang/codegen';
import { Runner } from '../src/lang/interp';

const CASES = {
  'fib(30) 递归': `
function fib(n)
    if (n < 2)
        return n
    return fib(n - 1) + fib(n - 2)
print(fib(30))
`,
  '循环累加 500 万': `
int s = 0
for i in range(5000000)
    s += i
print(s)
`,
  '字符串拼接 5 万': `
str out = ""
for i in range(50000)
    out += "x"
print(len(out))
`
};

function timeIt(fn: () => void): number {
  const t0 = performance.now();
  fn();
  return performance.now() - t0;
}

function runCompiled(src: string): number {
  const prog = parseProgram('bench.cw', src);
  const unit = compileProgram(prog);
  if (!unit) throw new Error('编译失败');
  const rt = new CompiledRuntime();
  rt.out = () => {};
  const gen = unit.main(rt);
  let t0 = performance.now();
  // 同步驱动（没有 ask/wait）
  const first = gen.next();
  if (!first.done) throw new Error('基准脚本不应有挂起');
  return performance.now() - t0;
}

async function runInterpreted(src: string): Promise<number> {
  const sources = new Map<string, string>([['bench.cw', src]]);
  (globalThis as unknown as { window: unknown }).window = {
    __floatSources: sources,
    setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms)
  };
  let runner: Runner | null = null;
  let settle: () => void = () => {};
  const done = new Promise<void>((r) => { settle = r; });
  runner = new Runner('bench.cw', {
    output: () => {},
    onState: () => {},
    onPaused: () => {},
    onDone: () => settle()
  }, '/proj');
  const t0 = performance.now();
  runner.start('debug'); // 解释通道
  // 调试模式会停在第一条，需要放开
  await new Promise<void>((r) => setTimeout(r, 0));
  runner.continueRun();
  await done;
  return performance.now() - t0;
}

async function main(): Promise<void> {
  const results: { name: string; compile: number; run: number; total: number; interp: number }[] = [];

  for (const [name, src] of Object.entries(CASES)) {
    // 预热一次
    runCompiled(src);
    const probe = parseProgram('bench.cw', src);
    const t0 = performance.now();
    const unit = compileProgram(probe);
    const compileMs = performance.now() - t0;
    if (!unit) { console.log(`${name}: 编译失败`); continue; }
    const rt = new CompiledRuntime();
    rt.out = () => {};
    const t1 = performance.now();
    unit.main(rt).next();
    const runMs = performance.now() - t1;

    const interpMs = name.includes('500 万')
      ? await runInterpreted(src)
      : await runInterpreted(src);

    results.push({ name, compile: compileMs, run: runMs, total: compileMs + runMs, interp: interpMs });
  }

  console.log('\n用例'.padEnd(20) + '编译(ms)'.padStart(10) + '执行(ms)'.padStart(10) + '合计(ms)'.padStart(10) + '解释(ms)'.padStart(12));
  for (const r of results) {
    console.log(
      r.name.padEnd(20) + r.compile.toFixed(1).padStart(10) + r.run.toFixed(1).padStart(10) +
      r.total.toFixed(1).padStart(10) + r.interp.toFixed(0).padStart(12)
    );
  }

  // 输出 JSON 供与 Python 对比
  console.log('\nJSON:' + JSON.stringify(results.map((r) => ({ name: r.name, run: r.run, total: r.total, interp: r.interp }))));
  void timeIt;
}

void main();
