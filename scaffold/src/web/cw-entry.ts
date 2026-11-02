/* Copper · 商店应用运行时（浏览器 IIFE 包）
 * ---------------------------------------------------------------------------
 * Float IDE「发布到应用商店」的应用是 Copper 源码包。这个入口把语言内核
 * （src/lang，跟 IDE 同一份解释器）打成无依赖 IIFE，挂在 window.__cwRun 上，
 * 供 Nexus 应用框架的受限壳（apps/floatapp/index.html）在 sandbox iframe 里调用。
 * 解释器是异步生成器驱动的，所以 run 返回 Promise；ask() 会让结果带 needInput。
 */
import { Runner } from '../lang/interp';

export type CwRunResult = {
  output: string[];
  error: string | null;
  errorLine: number;
  steps: number;
  needInput: string | null;
  ms: number;
};

async function run(
  entryFile: string,
  sources: Record<string, string>,
  onLine: (text: string, isErr: boolean) => void
): Promise<CwRunResult> {
  const entry = entryFile.replace(/\\/g, '/');
  const map = new Map<string, string>();
  for (const k of Object.keys(sources)) map.set(k.replace(/\\/g, '/'), String(sources[k]));
  if (!map.has(entry)) map.set(entry, String(sources[entry] ?? ''));

  const out: string[] = [];
  const t0 = (typeof performance !== 'undefined' ? performance.now() : Date.now());
  let err: string | null = null;
  let errLine = 0;
  let needInput: string | null = null;

  return new Promise<CwRunResult>((resolve) => {
    let settled = false;
    const done = (): void => {
      if (settled) return;
      settled = true;
      window.clearInterval(watchdog);
      window.clearTimeout(guard);
      const steps = (runner as unknown as { steps?: number }).steps ?? 0;
      const ms = Math.round((typeof performance !== 'undefined' ? performance.now() : Date.now()) - t0);
      resolve({ output: out, error: err, errorLine: errLine, steps, needInput, ms });
    };
    const runner = new Runner(entry, {
      output: (l) => { out.push(l.text); onLine(l.text, l.kind === 'err'); },
      report: () => {},
      onPaused: () => {},
      onDone: () => { done(); },
      onState: () => {},
      onError: (info) => { if (!err) { err = info.message; errLine = info.line; } }
    }, '/', map);
    const watchdog = window.setInterval(() => {
      if (runner.askQuestion !== null) { needInput = runner.askQuestion; done(); return; }
      if (runner.state === 'done') done();
    }, 16);
    const guard = window.setTimeout(done, 30_000);   // 兜底：死循环脚本最多跑 30 秒
    runner.start('run');
  });
}

declare global {
  interface Window { __cwRun?: typeof run; }
}

(window as unknown as { __cwRun: typeof run }).__cwRun = run;
