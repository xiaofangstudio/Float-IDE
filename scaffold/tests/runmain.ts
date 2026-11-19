import { Runner } from '../src/lang/interp';
import * as fs from 'fs';

const src = fs.readFileSync('D:/20533/deskbox/fangwork/Float/language/samples/main.co', 'utf8');
const mode = (process.argv[2] || 'debug') as 'run' | 'debug';
const inputs = (process.argv[3] || '1,3,1').split(',');
let idx = 0;
let lines: string[] = [];
const runner = new Runner('main.co', {
  output: (l) => { lines.push(typeof l === 'string' ? l : (l as any).text ?? JSON.stringify(l)); },
  report: () => {},
  onPaused: (node, vars) => {
    const r = runner as any;
    // 有提问就答，没提问就"继续"（等价于 IDE 的 F8）
    if (r.askQuestion !== null && r.askQuestion !== undefined) {
      const a = inputs[Math.min(idx, inputs.length - 1)];
      idx++;
      if (lines.length < 400) console.log('[ask]', JSON.stringify(r.askQuestion).slice(0, 80), '->', JSON.stringify(a));
      setTimeout(() => r.resume(a), 0);
    } else {
      setTimeout(() => r.continueRun(), 0);
    }
  },
  onDone: () => { console.log('=== DONE ==='); finish(); },
  onState: () => {},
  onError: (e: any) => { console.log('=== ERROR ===', e && e.message, e && e.frames && JSON.stringify(e.frames)); finish(); },
  onCall: () => {},
} as any, 'D:/20533/deskbox/fangwork/Float/language/samples', new Map([['main.co', src]]));

function finish() {
  console.log('--- 输出 (前 60 / 共 ' + lines.length + ') ---');
  for (const l of lines.slice(0, 60)) console.log(l);
  process.exit(0);
}
runner.start(mode);
setTimeout(() => { console.log('=== TIMEOUT, state=', (runner as any).state, ' lines=', lines.length); finish(); }, 20000);
