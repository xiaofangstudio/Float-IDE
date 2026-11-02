/* Float IDE 前端类型。 */

export type FileEntry = { name: string; relPath: string; isDir: boolean };

export type RecentItem = { path: string; name: string; lastOpened: string };

export type ProjectMeta = {
  name: string;
  entry: string;
  version?: string;
  kind?: string;
};

export type DetectInfo = { edge: string | null; chrome: string | null };

/** 控制台行：out=普通输出、err=红色错误、info=蓝色信息；
 * dbg-* 是「调试 报告/警告/报错」三档（蓝/黄/红），纯给开发者看的 */
export type ConsoleLine = { kind: 'out' | 'err' | 'info' | 'dbg-report' | 'dbg-warn' | 'dbg-error'; text: string; ts: number };

export type VarRow = { name: string; type: string; value: string };

/** 「报告」面板的一条：脚本里 `报告 xxx` 写过来的 */
export type ReportRow = { text: string; line: number; ts: number };

/** 问题面板 / 状态栏用的诊断条目 */
export type ProblemItem = {
  severity: 'error' | 'warning' | 'info';
  message: string;
  file: string;
  line: number;
  col: number;
  code?: string;
  from?: 'edit' | 'run';
};
