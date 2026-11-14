/* 无头验证器：加载方岛物语全模块，跑诊断 + 剧本化冒烟。
 * 用法：node tests/.fang.mjs <mode: diag|play> <answersFile?>
 */
import { Runner } from '../src/lang/interp';
import { parseProgram } from '../src/lang/parse';
import { analyze } from '../src/lang/analyze';
import * as fs from 'fs';
import * as path from 'path';

const ROOT = process.argv[2] ?? 'd:/20533/deskbox/files/小方趣生活游戏';
const MODE = process.argv[3] ?? 'diag';
const ANSWERS_FILE = process.argv[4] ?? '';

/* single 模式：直接审计单个 .co 文件（外部版本对比用） */
if (MODE === 'single') {
  const abs = fs.existsSync(ROOT) && fs.statSync(ROOT).isFile() ? ROOT : path.join(ROOT, '方岛物语.co');
  const src = fs.readFileSync(abs, 'utf-8');
  const prog = parseProgram('/proj/single.co', src);
  const syn = prog.diagnostics.filter((d) => d.severity === 'error');
  console.log(`SYNTAX: ${syn.length} errors`);
  for (const e of syn.slice(0, 40)) console.log(`  L${e.span.line}:C${e.span.col} ${e.message}`);
  const a = analyze('/proj/single.co', src);
  const errs = a.diagnostics.filter((d) => d.severity === 'error');
  console.log(`AUDIT: ${errs.length} errors`);
  const seen = new Map<string, number>();
  for (const e of errs) seen.set(e.message, (seen.get(e.message) || 0) + 1);
  for (const e of errs.slice(0, 60)) console.log(`  L${e.span.line}:C${e.span.col} ${e.message}`);
  console.log('— 归类 —');
  for (const [k, v] of [...seen.entries()].sort((x, y) => y[1] - x[1]).slice(0, 20)) console.log(`  ×${v} ${k}`);
  process.exit(0);
}

const ENTRY = path.join(ROOT, '方岛物语.co');
const MODULES = [
  '方岛_核心.co', '方岛_世界.co', '方岛_引擎.co', '方岛_物品.co', '方岛_猫猫.co',
  '方岛_剧情.co', '方岛_存档.co',
  '方岛_扩展_互动.co', '方岛_扩展_商店.co', '方岛_扩展_户外.co', '方岛_扩展_撬锁.co',
  '方岛_扩展_工作台.co',
  '方岛_扩展_工坊.co', '方岛_扩展_生态.co', '方岛_扩展_猫战.co', '方岛_扩展_后室.co',
  '方岛_主.co'
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

/* ── 跨模块静态审计：按 import 顺序拼接，等价 F5 摊平后做语义/类型检查 ── */
if (MODE === 'audit' || MODE === 'perfile') {
  let merged = '';
  for (const f of ['方岛物语.co', ...MODULES]) {
    merged += `\n/* ==== ${f} ==== */\n` + read(f).split('\n').map((l) => l).join('\n');
  }
  const a = analyze('/proj/_merged.co', merged);
  const errs = a.diagnostics.filter((d) => d.severity === 'error');
  console.log(`AUDIT: ${errs.length} 处错误`);
  const seen = new Map<string, number>();
  for (const e of errs) {
    const key = e.message;
    seen.set(key, (seen.get(key) || 0) + 1);
  }
  for (const e of errs.slice(0, 120)) {
    console.log(`  L${e.span.line}:C${e.span.col} ${e.message}`);
  }
  if (errs.length > 120) console.log(`  …还有 ${errs.length - 120} 处`);
  console.log('— 按消息归类 —');
  for (const [k, v] of [...seen.entries()].sort((x, y) => y[1] - x[1]).slice(0, 30)) console.log(`  ×${v} ${k}`);
  if (MODE === 'perfile') {
    console.log('');
    console.log('=== 逐文件检查（模拟 IDE 打开单个文件时的红波浪线）===');
    for (const f of ['方岛物语.co', ...MODULES]) {
      const pa = analyze('/proj/' + f, read(f));
      const pe = pa.diagnostics.filter((d) => d.severity === 'error');
      if (pe.length > 0) {
        console.log(`  ${f}: ${pe.length} 处`);
        for (const e of pe.slice(0, 3)) console.log(`     L${e.span.line} ${e.message}`);
      }
    }
  }
  process.exit(errs.length === 0 ? 0 : 1);
}

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
