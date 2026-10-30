/* Float IDE 前端类型。 */

export type FileEntry = { name: string; relPath: string; isDir: boolean };

export type RecentItem = { path: string; name: string; lastOpened: string };

export type ProjectMeta = {
  name: string;
  entry: string;
  version?: string;
  kind?: string;
};

export type DetectInfo = { browser: string | null; ffmpeg: string | null };

export type ConsoleLine = { kind: 'out' | 'err' | 'info'; text: string; ts: number };

export type VarRow = { name: string; type: string; value: string };

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
