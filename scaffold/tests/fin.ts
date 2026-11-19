/* 临时夹具：验证 F6（TS 解释器）在 return + finally、带类型方法参数上的语义。
 * 用法：npx esbuild tests/fin.ts --bundle --platform=node --format=esm --outfile=tests/.fin.mjs && node tests/.fin.mjs
 */
import { Runner } from '../src/lang/interp';

const CASES: [string, string][] = [
  [
    'finally 在 return 局部变量时照跑',
    'fn work()\n    try\n        int k = 8\n        return k\n    finally\n        print("收尾 " + str(k))\nprint(work())\n'
  ],
  [
    '方法带类型参数 arity 正确',
    'class D\n    int hp = 10\n    fn set(int v)\n        return v + 1\nprint(new D().set(4))\n'
  ],
  [
    'finally 里改 k 不影响返回值',
    'fn f()\n    try\n        int k = 1\n        return k\n    finally\n        k = 99\nprint(f())\n'
  ]
];

async function runOne(name: string, src: string, mode: 'run' | 'debug'): Promise<string> {
  return new Promise((done) => {
    const out: string[] = [];
    const r = new Runner('case.co', {
      output: (l) => { if (l.kind === 'out' || l.kind === 'info' || l.kind === 'err') out.push(l.text); },
      onError: (e) => out.push('ERR ' + e.message),
      onState: () => {},
      onPaused: () => { setTimeout(() => r.continueRun(), 0); },
      onDone: () => done(out.join('\n'))
    }, '.', new Map([['case.co', src]]));
    r.start(mode);
  });
}

(async () => {
  for (const [name, src] of CASES) {
    const run = await runOne(name, src, 'run');
    const dbg = await runOne(name, src, 'debug');
    const same = run === dbg;
    console.log(`== ${name}\n  run: ${JSON.stringify(run)}\n  dbg: ${JSON.stringify(dbg)}\n  一致: ${same}`);
  }
})();
