/* 索引分析 worker：把「切词 + 建符号表」搬到别的线程去。
 *
 * 主线程只负责读盘和拼搜索结果，符号表在 worker 里算——
 * 打开大工程时界面不再一节一节地卡住，而且几个 worker 能同时啃几个文件。
 * 这里只要符号（函数/类/变量/大纲），不要诊断，所以走 symbolsOnly 快路径。
 */
import { analyze } from '../lang/analyze';

type Req = { id: number; path: string; text: string };

type Res = {
  id: number;
  funcs: ReturnType<typeof analyze>['funcs'];
  classes: ReturnType<typeof analyze>['classes'];
  vars: ReturnType<typeof analyze>['vars'];
  outline: ReturnType<typeof analyze>['outline'];
  error?: string;
};

self.onmessage = (e: MessageEvent<Req>): void => {
  const { id, path, text } = e.data;
  try {
    const a = analyze(path, text, undefined, { symbolsOnly: true });
    const res: Res = { id, funcs: a.funcs, classes: a.classes, vars: a.vars, outline: a.outline };
    (self as unknown as Worker).postMessage(res);
  } catch (err) {
    const res: Res = { id, funcs: [], classes: [], vars: [], outline: [], error: String(err) };
    (self as unknown as Worker).postMessage(res);
  }
};
