/* 诊断线程池的客户端：主线程只管排队和取消，算在 worker 里。
 *
 * 只有一个 worker（诊断是"最新的一份说了算"，并行反而浪费），
 * 同一文件连打时用 id 丢弃过期结果——不会出现「旧代码的红线」闪一下。
 * worker 起不来就退回主线程同步算，行为完全一致。
 */
import type { Diagnostic } from './ast';

type Req = { id: number; path: string; text: string; names: string[] | null };

export type DiagnoseResult = { diagnostics: Diagnostic[] };

let worker: Worker | null = null;
let broken = false;
let seq = 0;
const pending = new Map<number, (r: DiagnoseResult) => void>();

function ensure(): Worker | null {
  if (broken) return null;
  if (worker) return worker;
  try {
    const w = new Worker(new URL('./diagnoseWorker.ts', import.meta.url), { type: 'module' });
    w.onmessage = (e: MessageEvent<{ id: number; diagnostics: Diagnostic[] }>): void => {
      const cb = pending.get(e.data.id);
      if (cb) {
        pending.delete(e.data.id);
        cb({ diagnostics: e.data.diagnostics });
      }
    };
    w.onerror = (): void => {
      broken = true;
      worker = null;
      // 所有在等的任务改走主线程兜底
      for (const [, cb] of pending) cb({ diagnostics: [] });
      pending.clear();
    };
    worker = w;
    return w;
  } catch {
    broken = true;
    return null;
  }
}

/** 后台诊断。返回的 Promise 永远 resolve（失败给空诊断，不报错、不挡着人写代码）。 */
export async function diagnoseAsync(path: string, text: string, names: Set<string> | undefined): Promise<DiagnoseResult> {
  const w = ensure();
  if (!w) return { diagnostics: [] };
  const id = ++seq;
  return new Promise<DiagnoseResult>((resolve) => {
    pending.set(id, resolve);
    const req: Req = { id, path, text, names: names ? [...names] : null };
    w.postMessage(req);
  });
}

/** 有没有可用的 worker（没有就别白等） */
export function diagnoseWorkerReady(): boolean {
  return ensure() !== null;
}
