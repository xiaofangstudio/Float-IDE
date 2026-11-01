/* 源码读取的唯一入口：**内存优先，磁盘兜底**。
 *
 * 编辑器里的缓冲区、工程索引里的内容都算"内存"。
 * 任何要读源码的地方（插件、运行、预览、导入解析）都走这里，
 * 于是"改了没存盘也能被读到"这件事只需要在这里保证一次，
 * 而不是每个调用点各自判断。
 */

import { api } from './tauri';

type Provider = (path: string) => string | null;

let provider: Provider | null = null;

/** 路径归一化：统一斜杠、去掉 ./ 和重复分隔符，方便两边 key 对上 */
export function normPath(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/\.\//g, '/').replace(/\/+/g, '/');
}

/** 由编辑器注册：给定路径返回缓冲区内容（没有就返回 null） */
export function setSourceProvider(fn: Provider | null): void {
  provider = fn;
}

/** 只看内存里有没有这份源码 */
export function sourceInMemory(path: string): string | null {
  if (!provider) return null;
  try {
    return provider(normPath(path));
  } catch {
    return null;
  }
}

/** 读源码：先问内存（未保存的改动也算），没有再读盘 */
export async function readSource(path: string): Promise<string> {
  const mem = sourceInMemory(path);
  if (mem !== null) return mem;
  return api.fsRead(path);
}
