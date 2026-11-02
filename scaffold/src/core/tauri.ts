import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { FileEntry, RecentItem } from './types';

export type McpStatus = { running: boolean; port: number; project: string; tools: string[] };

/* Tauri 命令封装（Float IDE）：文件 / 工程 / 终端 / Git。 */

export { listen };

/** 是不是跑在 Tauri 桌面壳里（浏览器 demo 里为 false）。
 *  全工程只此一处判断，别各文件自己写一份。 */
export function isTauriEnv(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export const api = {
  /* 文件 */
  fsList: (dir: string) => invoke<FileEntry[]>('fs_list', { dir }),
  /** 开始监视工程目录（.cw/.json/.txt 的增删改），变更通过 fs:changed 事件推送 */
  fsWatchStart: (project: string) =>
    invoke<{ watching: boolean; root: string }>('fs_watch_start', { project }),
  fsRead: (path: string) => invoke<string>('fs_read', { path }),
  fsWrite: (path: string, contents: string) => invoke<void>('fs_write', { path, contents }),
  /** 文件指纹（mtime+大小），用于检测磁盘内容是否被别的进程改动 */
  fsStat: (path: string) => invoke<{ mtimeMs: number; size: number; exists: boolean }>('fs_stat', { path }),
  /** 带版本校验的写：expectMtimeMs 与磁盘不符时抛 CONFLICT，绝不静默覆盖 */
  fsWriteChecked: (path: string, contents: string, expectMtimeMs: number) =>
    invoke<{ mtimeMs: number; size: number; exists: boolean }>(
      'fs_write_checked', { path, contents, expectMtimeMs }),
  fsMkdir: (path: string) => invoke<void>('fs_mkdir', { path }),

  /* 工程占用锁：防止两个 Float IDE 同时改同一个工程 */
  lockAcquire: (dir: string, id: string) =>
    invoke<{ id: string; ts: number; pid: number }>('project_lock_acquire', { dir, id }),
  lockTouch: (dir: string, id: string) => invoke<boolean>('project_lock_touch', { dir, id }),
  lockRelease: (dir: string, id: string) => invoke<void>('project_lock_release', { dir, id }),
  fsRename: (from: string, to: string) => invoke<void>('fs_rename', { from, to }),
  fsDelete: (path: string) => invoke<void>('fs_delete', { path }),

  /* 语言内核（Rust） */
  langRun: (
    file: string,
    source: string,
    inputs: string[] = [],
    /** 当前所有打开的文件（路径 → 源码）：没存盘的库文件也能被 import 到 */
    extra: Record<string, string> = {}
  ) =>
    invoke<{
      output: string;
      reports: string[];
      error: string | null;
      error_line: number;
      steps: number;
      /** 有值 = 脚本停在某个 ask 上等人回答 */
      needInput: string | null;
    }>('lang_run', { file, source, inputs, extra }),
  langCheck: (file: string, source: string) =>
    invoke<{ severity: 'error' | 'warning' | 'info'; message: string; line: number; col: number }[]>(
      'lang_check', { file, source }
    ),

  /* 工程 */
  projectNew: (dir: string, name: string, files: Record<string, string>) =>
    invoke<string>('project_new', { args: { dir, name, files } }),
  projectExportZip: (projectDir: string, outPath: string) =>
    invoke<void>('project_export_zip', { projectDir, outPath }),
  projectImportZip: (zipPath: string, destDir: string) =>
    invoke<string>('project_import_zip', { zipPath, destDir }),
  pickFolder: (title: string) => invoke<string | null>('pick_folder', { title }),
  /** 选单个文件（.co / .cw 之类）。"打开"以前只能选文件夹，所以根本看不到脚本文件 */
  pickOpenFile: (title: string, exts: string[] = ['co', 'cw', 'md', 'txt']) =>
    invoke<string | null>('pick_open_file', { title, exts }),
  /** 启动参数里带进来的路径（双击 .co / .cw 打开时） */
  startupPath: () => invoke<string | null>('startup_path'),
  pickSaveFile: (title: string, defaultName: string, ext: string) =>
    invoke<string | null>('pick_save_file', { title, defaultName, ext }),
  openPath: (path: string) => invoke<void>('open_path', { path }),
  pickImage: (title: string) => invoke<string | null>('pick_image', { title }),
  detectEnv: () => invoke<{ edge: string | null; chrome: string | null }>('detect_env'),

  /* 最近工程 */
  recentGet: () => invoke<RecentItem[]>('recent_get'),
  recentPush: (path: string, name: string) => invoke<void>('recent_push', { path, name }),
  recentRemove: (path: string) => invoke<void>('recent_remove', { path }),

  /* 终端（ConPTY） */
  ptyCreate: (id: string, cwd: string, rows: number, cols: number) =>
    invoke<void>('pty_create', { id, cwd, rows, cols }),
  ptyWrite: (id: string, data: string) => invoke<void>('pty_write', { id, data }),
  ptyResize: (id: string, cols: number, rows: number) => invoke<void>('pty_resize', { id, cols, rows }),
  ptyClose: (id: string) => invoke<void>('pty_close', { id }),

  /* 插件 */
  pluginRootDir: () => invoke<string>('plugin_root_dir'),
  pluginScan: () => invoke<{ id: string; dir: string; manifest: string; error: string | null }[]>('plugin_scan'),
  pluginReadFile: (id: string, rel: string) => invoke<string>('plugin_read_file', { id, rel }),
  pluginWriteFile: (id: string, rel: string, contents: string) =>
    invoke<void>('plugin_write_file', { id, rel, contents }),
  pluginInstallDir: (src: string) => invoke<string>('plugin_install_dir', { src }),
  pluginUninstall: (id: string) => invoke<void>('plugin_uninstall', { id }),
  pluginReveal: () => invoke<void>('plugin_reveal'),
  pluginStatePath: () => invoke<{ path: string }>('plugin_state_path'),
  pluginRegistry: () => invoke<{
    id: string; name: string; version: string; publisher: string;
    description: string; tags: string[]; localPath: string;
  }[]>('plugin_registry'),

  /* MCP */
  mcpStart: (port: number, project: string) =>
    invoke<McpStatus>('mcp_start', { port, project }),
  mcpStop: () => invoke<void>('mcp_stop'),
  mcpStatus: () => invoke<McpStatus>('mcp_status'),
  mcpLog: () => invoke<string[]>('mcp_log'),
  mcpReply: (id: string, result: string) => invoke<void>('mcp_reply', { id, result }),

  /* Git */
  gitAvailable: () => invoke<boolean>('git_available'),
  gitRun: (dir: string, args: string[]) =>
    invoke<{ code: number; stdout: string; stderr: string }>('git_run', { dir, args }),

  /* 文件关联（Windows 用户级注册表）：让资源管理器给 .co 显示图标、双击能打开 */
  assocStatus: () => invoke<boolean>('assoc_status'),
  assocRegister: () => invoke<void>('assoc_register'),
  assocUnregister: () => invoke<void>('assoc_unregister')
};
