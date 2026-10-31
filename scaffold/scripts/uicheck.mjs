/**
 * 无头 UI 自测跑手。
 *
 *   用法：node scripts/uicheck.mjs [url]
 *   默认：http://127.0.0.1:1430/demo.html?debug=1&selftest=1
 *         （debug=1 必须带上，补全那几条断言要用到 window.__floatBus）
 *
 * demo.tsx 里的 selftest() 会真的去点 DOM（新建文件、补全、改名……），
 * 结果写进 #selftest-result。这里用 Edge 无头 --dump-dom 把那段文本捞出来，
 * 数 FAIL / ok 行，失败就 exit 1。
 */
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const URL_ = process.argv[2] ?? 'http://127.0.0.1:1430/demo.html?debug=1&selftest=1';
const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const BUDGET = 20000; // 虚拟时间预算（ms），selftest 里全是 await sleep

const profile = mkdtempSync(join(tmpdir(), 'float-uicheck-'));

function run() {
  return new Promise((resolve, reject) => {
    // 注意：不要加 --disable-gpu / --run-all-compositor-stages-before-draw，
    // 这两玩意会让 requestAnimationFrame 直接不跑，Monaco 的补详情面板就永远渲不出来。
    const args = [
      '--headless=new',
      '--no-first-run',
      '--no-default-browser-check',
      `--user-data-dir=${profile}`,
      `--virtual-time-budget=${BUDGET}`,
      '--dump-dom',
      URL_,
    ];
    const p = spawn(EDGE, args, { stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    let err = '';
    p.stdout.on('data', (d) => (out += d));
    p.stderr.on('data', (d) => (err += d));
    p.on('error', reject);
    p.on('close', () => resolve({ out, err }));
    // 兜底：无头偶尔卡住，最多等 90s
    setTimeout(() => { try { p.kill(); } catch {} }, 90000);
  });
}

const { out } = await run();
rmSync(profile, { recursive: true, force: true });

// 从 dump 出来的 DOM 里抠 #selftest-result 的文本
const m = out.match(/<div id="selftest-result"[^>]*>([\s\S]*?)<\/div>/);
const text = m ? m[1] : '';
if (!text) {
  console.error('没拿到 #selftest-result。可能 dev server 没起、或 selftest 超时了。');
  console.error('DOM 长度：' + out.length);
  process.exit(2);
}

const unescapeHtml = (s) =>
  s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"')
   .replace(/&#0?39;/g, "'").replace(/&amp;/g, '&').trim();

const lines = unescapeHtml(text).split('\n').map((l) => l.trim()).filter(Boolean);
const fails = lines.filter((l) => /^\s*(FAIL|✗|✘|失败)/i.test(l) || l.includes('FAIL'));
const oks = lines.filter((l) => /^\s*(ok|✓|通过)/i.test(l));

console.log(lines.join('\n'));
console.log('\n———————————————');
console.log(`通过 ${oks.length} · 失败 ${fails.length}`);
if (fails.length) {
  console.error('\n失败项：\n' + fails.join('\n'));
  process.exit(1);
}
