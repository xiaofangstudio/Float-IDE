/* 插件运行时。
 *
 * 三件事：
 *   1. 扫描 + 解析 + 启用状态持久化（state.json）
 *   2. 把各插件的贡献点汇总成「命令表 / 片段表 / 关键词表 / 菜单项」
 *   3. 给 main.js 注入受控 API 并执行（没有 DOM、没有 Node、没有任意 import）
 *
 * 安全边界：main.js 通过 new Function 在一个只拿到 float 对象的作用域里跑。
 * 它碰不到 window / document / localStorage，只能调用我们显式暴露的方法。
 */

import { api } from '../core/tauri';
import { BUILTIN_PLUGINS } from './builtin';
import type {
  PluginCommand, PluginManifest, PluginPermission, PluginRecord, PluginSnippet
} from './types';

export type PluginCommandEntry = {
  id: string;
  title: string;
  category: string;
  pluginId: string;
  command: PluginCommand;
  handler?: (...args: unknown[]) => unknown;
};

export type PluginHostOptions = {
  projectDir: string;
  /** 控制台输出 */
  log: (kind: 'out' | 'err' | 'info', text: string) => void;
  /** 弹一条界面提示 */
  notify: (text: string) => void;
  /** 编辑器桥 */
  editor: {
    getText: () => string;
    setText: (t: string) => void;
    insert: (t: string) => void;
    getSelection: () => string;
    getPath: () => string | null;
    getCursor: () => { line: number; col: number } | null;
  };
  /** 运行一段 CW Script，返回输出与错误 */
  runCode: (code: string) => Promise<{ out: string[]; error: string | null }>;
};

type StateMap = Record<string, boolean>;

export class PluginHost {
  private opts: PluginHostOptions;
  private records: PluginRecord[] = [];
  private commands = new Map<string, PluginCommandEntry>();
  private snippets: (PluginSnippet & { pluginId: string })[] = [];
  private keywords: { label: string; detail?: string; insert?: string; pluginId: string }[] = [];
  private menuItems: { command: string; label: string }[] = [];
  private state: StateMap = {};
  private listeners = new Set<() => void>();
  private activated = new Set<string>();

  constructor(opts: PluginHostOptions) {
    this.opts = opts;
  }

  onChange(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  }

  private emit(): void {
    for (const l of this.listeners) l();
  }

  get list(): PluginRecord[] { return this.records; }
  get commandList(): PluginCommandEntry[] { return [...this.commands.values()]; }
  get snippetList(): PluginSnippet[] { return this.snippets; }
  get keywordList(): { label: string; detail?: string; insert?: string }[] { return this.keywords; }
  get menus(): { command: string; label: string }[] { return this.menuItems; }

  async init(): Promise<void> {
    await this.ensureBuiltins();
    await this.loadState();
    await this.reload();
  }

  /** 首次运行把内置示例插件铺到插件目录，让用户有个能照着改的样板 */
  private async ensureBuiltins(): Promise<void> {
    let existing: { id: string }[] = [];
    try { existing = await api.pluginScan(); } catch { return; }
    const have = new Set(existing.map((e) => e.id));
    for (const b of BUILTIN_PLUGINS) {
      if (have.has(b.id)) continue;
      try {
        for (const [rel, content] of Object.entries(b.files)) {
          await api.pluginWriteFile(b.id, rel, content);
        }
      } catch {
        /* 预览环境忽略 */
      }
    }
  }

  private async loadState(): Promise<void> {
    try {
      const { path } = await api.pluginStatePath();
      const raw = await api.fsRead(path);
      this.state = JSON.parse(raw) as StateMap;
    } catch {
      this.state = {};
    }
  }

  private async saveState(): Promise<void> {
    try {
      const { path } = await api.pluginStatePath();
      await api.fsWrite(path, JSON.stringify(this.state, null, 2));
    } catch { /* 忽略 */ }
  }

  async setEnabled(id: string, on: boolean): Promise<void> {
    this.state[id] = on;
    await this.saveState();
    if (!on) this.deactivate(id);
    await this.reload();
  }

  private deactivate(id: string): void {
    for (const [cid, entry] of this.commands) {
      if (entry.pluginId === id) this.commands.delete(cid);
    }
    this.activated.delete(id);
  }

  async reload(): Promise<void> {
    this.commands.clear();
    this.snippets = [];
    this.keywords = [];
    this.menuItems = [];
    this.records = [];

    let scanned: { id: string; dir: string; manifest: string; error: string | null }[] = [];
    try {
      scanned = await api.pluginScan();
    } catch {
      scanned = [];
    }

    /* 内置插件即使目录没铺成功也让它出现在列表里，标明是内置的 */
    const seen = new Set(scanned.map((s) => s.id));

    for (const s of scanned) {
      let manifest: PluginManifest | null = null;
      let error = s.error;
      if (!error && s.manifest) {
        try {
          manifest = normalizeManifest(JSON.parse(s.manifest) as PluginManifest, s.id);
        } catch (e) {
          error = 'plugin.json 不是合法 JSON：' + String(e);
        }
      }
      const enabled = this.state[s.id] !== false;
      const rec: PluginRecord = {
        id: s.id,
        dir: s.dir,
        manifest,
        error,
        enabled,
        builtin: BUILTIN_PLUGINS.some((b) => b.id === s.id)
      };
      this.records.push(rec);

      if (enabled && manifest && !error) {
        // 一个插件坏掉不能拖垮其他插件：各自独立 try/catch，错误只记在该插件上
        try {
          await this.register(rec);
        } catch (e) {
          rec.error = '加载失败：' + String(e);
          this.opts.log('err', `插件 ${rec.id} 加载失败：${String(e)}`);
        }
      }
    }

    for (const b of BUILTIN_PLUGINS) {
      if (seen.has(b.id)) continue;
      const m = b.manifest as unknown as PluginManifest;
      this.records.push({
        id: b.id,
        dir: '',
        manifest: m,
        error: '插件目录不可用（预览环境），仅显示',
        enabled: false,
        builtin: true
      });
    }

    this.records.sort((a, b) => Number(b.builtin) - Number(a.builtin) || a.id.localeCompare(b.id));
    this.emit();
  }

  private async register(rec: PluginRecord): Promise<void> {
    const m = rec.manifest;
    if (!m) return;
    const c = m.contributes ?? {};

    /* 1) main.js：先把 handler 注册进来 */
    const handlers = new Map<string, (...args: unknown[]) => unknown>();
    if (m.main) {
      const apiObj = this.makeApi(rec, handlers);
      try {
        const code = await api.pluginReadFile(rec.id, m.main);
        const mod = runModule(code, apiObj);
        // 约定：export.activate(float) 在启用时调用一次
        const act = (mod as { activate?: (f: unknown) => void }).activate;
        if (typeof act === 'function') {
          act(apiObj);
          this.activated.add(rec.id);
        }
        // export.commands = { name: fn } → 供 contribute.commands[].handler 引用
        const cmds = (mod as { commands?: Record<string, (...a: unknown[]) => unknown> }).commands;
        if (cmds) {
          for (const [k, fn] of Object.entries(cmds)) {
            if (typeof fn === 'function') handlers.set(k, fn);
          }
        }
      } catch (e) {
        this.opts.log('err', `插件 ${m.name} 的 main.js 加载失败：${String(e)}`);
      }
    }

    /* 2) 命令 */
    for (const cmd of c.commands ?? []) {
      if (!cmd.id || !cmd.title) continue;
      if (this.commands.has(cmd.id)) {
        this.opts.log('err', `插件 ${m.name} 的命令 ${cmd.id} 与已装插件冲突，已跳过`);
        continue;
      }
      const entry: PluginCommandEntry = {
        id: cmd.id,
        title: cmd.title,
        category: cmd.category ?? '插件',
        pluginId: rec.id,
        command: cmd,
        handler: cmd.handler ? handlers.get(cmd.handler) : undefined
      };
      this.commands.set(cmd.id, entry);
      commandsById.set(cmd.id, entry);
    }

    /* 3) 片段 / 关键词 / 菜单 */
    for (const s of c.snippets ?? []) {
      if (s.prefix && s.body) this.snippets.push({ ...s, pluginId: rec.id });
    }
    for (const k of c.keywords ?? []) {
      if (k.label) this.keywords.push({ ...k, pluginId: rec.id });
    }
    // menus 既可能是 { tools: [...] }，也可能是 [{ tools: [...] }]，都吃
    const menus = c.menus
      ? (Array.isArray(c.menus) ? c.menus : [c.menus])
      : [];
    for (const mi of menus) {
      for (const t of mi?.tools ?? []) {
        if (t?.command && t?.label) this.menuItems.push({ command: t.command, label: t.label });
      }
    }
  }

  /* ─────── 执行 ─────── */

  async execute(id: string): Promise<void> {
    const entry = this.commands.get(id) ?? commandsById.get(id);
    if (!entry) {
      this.opts.log('err', `找不到插件命令 ${id}`);
      return;
    }
    const p = entry.command;
    try {
      if (!this.hasPermission(entry.pluginId, 'editor:write') && (p.insert || p.run)) {
        // 声明式命令默认允许（它属于插件自己的贡献点），这里只做记录
      }
      if (p.insert !== undefined) {
        this.opts.editor.insert(expandPlaceholders(p.insert, this.opts.projectDir, this.opts.editor.getPath()));
        this.opts.notify(`已插入：${entry.title}`);
        return;
      }
      if (p.run !== undefined) {
        const res = await this.opts.runCode(p.run);
        for (const line of res.out) this.opts.log('out', line);
        if (res.error) this.opts.log('err', res.error);
        return;
      }
      if (entry.handler) {
        await entry.handler();
        return;
      }
      this.opts.log('info', `命令 ${entry.title} 没有可执行内容`);
    } catch (e) {
      this.opts.log('err', `插件命令 ${entry.title} 执行失败：${String(e)}`);
    }
  }

  private hasPermission(pluginId: string, perm: PluginPermission): boolean {
    const rec = this.records.find((r) => r.id === pluginId);
    if (!rec?.manifest) return false;
    return (rec.manifest.permissions ?? []).includes(perm);
  }

  /** 构造注入给 main.js 的 API 对象 —— 只有这里列出来的，插件才够得到 */
  private makeApi(rec: PluginRecord, handlers: Map<string, (...a: unknown[]) => unknown>): Record<string, unknown> {
    const self = this;
    const m = rec.manifest;
    const allow = (p: PluginPermission): boolean =>
      rec.builtin || (m?.permissions ?? []).includes(p);

    const guard = <T>(perm: PluginPermission, fallback: T, fn: () => T): T => {
      if (!allow(perm)) {
        self.opts.log('err', `插件 ${m?.name ?? rec.id} 没有 ${perm} 权限，操作被拒绝`);
        return fallback;
      }
      return fn();
    };

    const storageKey = `float.plugin.${rec.id}`;
    const readStore = (): Record<string, unknown> => {
      try { return JSON.parse(window.localStorage.getItem(storageKey) ?? '{}') as Record<string, unknown>; }
      catch { return {}; }
    };

    return {
      plugin: {
        id: rec.id,
        name: m?.name ?? rec.id,
        version: m?.version ?? '0.0.0',
        storage: {
          get: <T,>(k: string, fallback?: T): T => {
            const s = readStore();
            return (k in s ? (s[k] as T) : (fallback as T));
          },
          set: (k: string, v: unknown): void => {
            const s = readStore();
            s[k] = v;
            try { window.localStorage.setItem(storageKey, JSON.stringify(s)); } catch { /* 忽略 */ }
          }
        }
      },
      console: {
        print: (msg: string): void => guard('console:write', undefined, () => self.opts.log('out', String(msg))),
        error: (msg: string): void => guard('console:write', undefined, () => self.opts.log('err', String(msg)))
      },
      editor: {
        getText: (): string => guard('editor:read', '', () => self.opts.editor.getText()),
        setText: (t: string): void => guard('editor:write', undefined, () => self.opts.editor.setText(String(t))),
        insert: (t: string): void => guard('editor:write', undefined, () => self.opts.editor.insert(String(t))),
        getSelection: (): string => guard('editor:read', '', () => self.opts.editor.getSelection()),
        getPath: (): string | null => self.opts.editor.getPath(),
        getCursor: () => self.opts.editor.getCursor()
      },
      files: {
        read: async (rel: string): Promise<string> => {
          if (!allow('files:read')) throw new Error(`插件 ${rec.id} 没有 files:read 权限`);
          return api.fsRead(joinPath(self.opts.projectDir, rel));
        },
        write: async (rel: string, text: string): Promise<void> => {
          if (!allow('files:write')) throw new Error(`插件 ${rec.id} 没有 files:write 权限`);
          await api.fsWrite(joinPath(self.opts.projectDir, rel), String(text));
        },
        list: async (rel = ''): Promise<string[]> => {
          if (!allow('files:read')) throw new Error(`插件 ${rec.id} 没有 files:read 权限`);
          return listFlat(joinPath(self.opts.projectDir, rel), self.opts.projectDir);
        }
      },
      language: {
        run: async (code: string) => {
          if (!allow('language:run')) throw new Error(`插件 ${rec.id} 没有 language:run 权限`);
          return self.opts.runCode(String(code));
        }
      },
      commands: {
        register: (id: string, fn: () => unknown): void => { handlers.set(id, fn); },
        execute: async (id: string): Promise<void> => {
          const h = handlers.get(id);
          if (h) { await h(); return; }
          await self.execute(id);
        }
      },
      ui: {
        notify: (msg: string): void => guard('ui:notify', undefined, () => self.opts.notify(String(msg))),
        status: (msg: string): void => guard('ui:notify', undefined, () => self.opts.log('info', String(msg)))
      }
    };
  }
}

/* 全局命令表：让「先执行后注册」也能工作（命令面板可能比插件先起来） */
const commandsById = new Map<string, PluginCommandEntry>();

function normalizeManifest(raw: PluginManifest, dirId: string): PluginManifest {
  return {
    ...raw,
    id: raw.id || dirId,
    name: raw.name || raw.id || dirId,
    version: raw.version || '0.0.0'
  };
}

/** 在受控作用域里跑 main.js：只给 float 与 module/exports，其余全无 */
function runModule(code: string, floatApi: Record<string, unknown>): Record<string, unknown> {
  const module = { exports: {} as Record<string, unknown> };
  // eslint-disable-next-line no-new-func
  const fn = new Function('float', 'module', 'exports', '"use strict";\n' + code);
  fn(floatApi, module, module.exports);
  return module.exports;
}

function joinPath(root: string, rel: string): string {
  const r = root.replace(/\\/g, '/').replace(/\/+$/, '');
  const t = String(rel).replace(/\\/g, '/').replace(/^\/+/, '');
  return t ? r + '/' + t : r;
}

async function listFlat(dir: string, root: string, depth = 0): Promise<string[]> {
  if (depth > 8) return [];
  let entries: { name: string; isDir: boolean }[];
  try { entries = await api.fsList(dir); } catch { return []; }
  const out: string[] = [];
  for (const e of entries) {
    if (e.name === '.git' || e.name === 'node_modules' || e.name === 'target' || e.name === '.float') continue;
    const p = dir.replace(/\\/g, '/') + '/' + e.name;
    if (e.isDir) {
      out.push(...await listFlat(p, root, depth + 1));
    } else {
      const r = root.replace(/\\/g, '/').replace(/\/+$/, '');
      out.push(p.startsWith(r + '/') ? p.slice(r.length + 1) : p);
    }
  }
  return out;
}

/** 命令里可用的占位符 */
export function expandPlaceholders(text: string, projectDir: string, currentFile: string | null): string {
  const now = new Date();
  const pad = (n: number): string => String(n).padStart(2, '0');
  return text
    .replace(/\$\{file\}/g, currentFile ? (currentFile.split(/[\\/]/).pop() ?? '') : '')
    .replace(/\$\{path\}/g, currentFile ?? '')
    .replace(/\$\{project\}/g, projectDir)
    .replace(/\$\{date\}/g, `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`)
    .replace(/\$\{time\}/g, `${pad(now.getHours())}:${pad(now.getMinutes())}`);
}
