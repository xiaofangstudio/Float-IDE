/* 语言内核测试驱动：在 Node 里跑 Runner，把输出收集成数组。
 * 用法见 tests/lang.test.ts，打包命令：
 *   esbuild tests/lang.test.ts --bundle --platform=node --format=esm --outfile=tests/.build.mjs
 */

import { Runner, type OutLine, type VarsSnapshot } from '../src/lang/interp';

export type RunResult = {
  out: string[];
  /** `报告 xxx` 的内容 */
  rep: string[];
  err: string[];
  info: string[];
  vars: VarsSnapshot;
  state: string;
};

type Opts = { answers?: string[]; file?: string; timeoutMs?: number; mode?: 'run' | 'debug' };

export async function run(src: string, opts: Opts = {}): Promise<RunResult> {
  const file = opts.file ?? 'main.cw';
  const sources = new Map<string, string>([[file, src]]);
  (globalThis as unknown as { window: unknown }).window = {
    __floatSources: sources,
    setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms)
  };

  const out: string[] = [];
  const rep: string[] = [];
  const err: string[] = [];
  const info: string[] = [];
  const answers = [...(opts.answers ?? [])];

  let runner: Runner | null = null;
  let settle: () => void = () => {};
  const done = new Promise<void>((resolve) => { settle = resolve; });

  const record = (l: OutLine): void => {
    if (l.kind === 'out') out.push(l.text);
    else if (l.kind === 'err') err.push(l.text);
    else info.push(l.text);
  };

  runner = new Runner(file, {
    output: record,
    report: (text) => rep.push(text),
    onState: () => {},
    onPaused: () => {
      setTimeout(() => {
        const r = runner;
        if (!r || r.state !== 'paused') return;
        if (r.pauseReason === 'ask') r.resume(answers.shift() ?? '');
        else if (r.pauseReason === 'wait') r.resume();
        else if (r.pauseReason === 'debug') r.continueRun();
      }, 0);
    },
    onDone: () => settle()
  }, '/proj');

  runner.start(opts.mode ?? 'run');

  const timeout = new Promise<void>((resolve) => setTimeout(resolve, opts.timeoutMs ?? 5000));
  await Promise.race([done, timeout]);

  const r = runner as Runner;
  return {
    out,
    rep,
    err,
    info, vars: r.varsSnapshot(), state: r.state };
}

/** 调试模式测试：记录每次暂停的行号，按 actions 逐步推进 */
export async function debugTrace(
  src: string,
  actions: ('step' | 'into' | 'continue')[],
  opts: { breakpoints?: number[] } = {}
): Promise<{ lines: number[]; out: string[]; err: string[] }> {
  const file = 'main.cw';
  (globalThis as unknown as { window: unknown }).window = {
    __floatSources: new Map([[file, src]]),
    setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms)
  };

  const lines: number[] = [];
  const out: string[] = [];
  const rep: string[] = [];
  const err: string[] = [];
  let acts = [...actions];
  let runner: Runner | null = null;
  let settle: () => void = () => {};
  const done = new Promise<void>((resolve) => { settle = resolve; });

  runner = new Runner(file, {
    output: (l) => { if (l.kind === 'out') out.push(l.text); else if (l.kind === 'err') err.push(l.text); },
    onState: () => {},
    onPaused: (node) => {
      if (node) lines.push(node.line);
      setTimeout(() => {
        const r = runner;
        if (!r || r.state !== 'paused') return;
        if (r.pauseReason !== 'debug') { r.resume(); return; }
        const act = acts.shift();
        if (!act) { r.stop(); return; }
        if (act === 'step') r.setStep('over');
        else if (act === 'into') r.setStep('into');
        else r.continueRun();
      }, 0);
    },
    onDone: () => settle()
  }, '/proj');

  if (opts.breakpoints) runner.setBreakpoints(file, opts.breakpoints);
  runner.start('debug');
  await Promise.race([done, new Promise<void>((r) => setTimeout(r, 4000))]);
  return { lines, out, err };
}
