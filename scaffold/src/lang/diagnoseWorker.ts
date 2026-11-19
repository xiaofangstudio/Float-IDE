/* 诊断 worker：解析 + 语义 / 类型 / 遮蔽三路检查全在别的线程跑。
 *
 * 主线程只把源码和「import 过来的名字总表」递进来，拿回一串诊断，
 * 自己 setModelMarkers —— 四千行的 colang.co 也压不住输入手感。
 */
import { analyze } from '../lang/analyze';
import type { Diagnostic } from '../lang/ast';

type Req = { id: number; path: string; text: string; names: string[] | null };

type Res = { id: number; diagnostics: Diagnostic[] };

self.onmessage = (e: MessageEvent<Req>): void => {
  const { id, path, text, names } = e.data;
  let diagnostics: Diagnostic[] = [];
  try {
    const project = names ? new Set(names) : undefined;
    diagnostics = analyze(path, text, project).diagnostics;
  } catch {
    diagnostics = [];
  }
  const res: Res = { id, diagnostics };
  (self as unknown as Worker).postMessage(res);
};
