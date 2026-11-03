/* 无头验证器：加载方岛物语全模块，跑诊断 + 剧本化冒烟。
 * 用法：node tests/.fang.mjs <mode: diag|play> <answersFile?>
 */
import { Runner } from '../src/lang/interp';
import { parseProgram } from '../src/lang/parse';
import * as fs from 'fs';
import * as path from 'path';

const ROOT = process.argv[2] ?? 'd:/20533/deskbox/files/小方趣生活游戏';
const MODE = process.argv[3] ?? 'diag';
const ANSWERS_FILE = process.argv[4] ?? '';

const ENTRY = path.join(ROOT, '方岛物语.co');
const MODULES = [
  '方岛_核心.co', '方岛_世界.co', '方岛_引擎.co', '方岛_物品.co', '方岛_猫猫.co',
  '方岛_剧情.co', '方岛_存档.co',
  '方岛_扩展_互动.co', '方岛_扩展_商店.co', '方岛_扩展_户外.co', '方岛_扩展_撬锁.co',
  '方岛_扩展_工作台.co', '方岛_主.co'
];

const read = (f: string): string => fs.readFileSync(path.join(ROOT, f), 'utf-8');

/* ── 语法诊断：逐文件 parse ── */
let syntaxErrors = 0;
for (const f of ['方岛物语.co', ...MODULES]) {
  const prog = parseProgram('/proj/' + f, read(f));
  const errs = prog.diagnostics.filter((d) => d.severity === 'error');
  for (const e of errs) {
    syntaxErrors++;
    console.log(`SYNTAX-ERR ${f} line ${e.span.line} col ${e.span.col}: ${e.message}`);
  }
}
console.log(`SYNTAX: ${syntaxErrors === 0 ? 'OK (0 errors)' : syntaxErrors + ' errors'}`);
if (MODE === 'diag' || syntaxErrors > 0) {
  process.exit(syntaxErrors === 0 ? 0 : 1);
}

/* ── 运行（解释引擎 = F6 通道）── */
let bootStrip = false;
if (MODE === 'lib') {
  bootStrip = true;
  if (!ANSWERS_FILE) { console.log('lib 模式需要测试代码文件'); process.exit(1); }
}
const sources = new Map<string, string>([['/proj/main.co', read('方岛物语.co')]]);
for (const m of MODULES) {
  let src = read(m);
  if (bootStrip && m === '方岛_主.co') {
    const cut = src.indexOf('BOOT-START');
    if (cut >= 0) src = src.slice(0, cut);
  }
  sources.set('/proj/' + m, src);
}

let answers: string[] = [];
let libTest = '';
if (MODE === 'lib') {
  libTest = fs.readFileSync(ANSWERS_FILE, 'utf-8');
} else if (ANSWERS_FILE) {
  answers = fs.readFileSync(ANSWERS_FILE, 'utf-8').split(/\r?\n/).filter((l) => l.length > 0);
  console.log(`answers: ${answers.length} 条`);
}
if (MODE === 'lib') {
  sources.set('/proj/main.co', read('方岛物语.co') + '\n' + libTest);
}

(globalThis as unknown as { window: unknown }).window = {
  __floatSources: sources,
  setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms)
};

const out: string[] = [];
const err: string[] = [];
let runner: Runner | null = null;
let done = false;

const settle = new Promise<void>((resolve) => {
  const check = () => { if (done) resolve(); else setTimeout(check, 50); };
  check();
});

const t0 = Date.now();
runner = new Runner('/proj/main.co', {
  output: (l) => {
    if (l.kind === 'out') { out.push(l.text); if (out.length <= 4000) console.log(l.text); }
    else if (l.kind === 'err') { err.push(l.text); console.log('ERR> ' + l.text); }
  },
  report: () => {},
  onState: () => {},
  onPaused: () => {
    const r = runner;
    if (!r || r.state !== 'paused') return;
    if (r.pauseReason === 'ask') {
      if (answers.length === 0) {
        console.log('>> 答案队列耗尽，终止');
        r.stop();
        return;
      }
      r.resume(answers.shift() ?? '');
    } else if (r.pauseReason === 'wait') r.resume();
    else r.continueRun();
  },
  onDone: () => { done = true; }
}, '/proj');

runner.start('run');

setTimeout(() => { if (!done) { console.log('>> 超时，终止'); runner?.stop(); } }, 120000);
await settle;

console.log('────────────────');
console.log(`RUN DONE state=${runner?.state} out=${out.length} err=${err.length} time=${((Date.now() - t0) / 1000).toFixed(1)}s`);
const tail = out.slice(-5).join('\n');
if (tail.includes('SMOKE OK')) console.log('SMOKE: PASS');
else console.log('SMOKE: 未见 SMOKE OK 标记');
if (err.length > 0) {
  console.log('错误输出（前 10 条）：');
  err.slice(0, 10).forEach((e) => console.log('  ' + e));
  process.exit(1);
}
