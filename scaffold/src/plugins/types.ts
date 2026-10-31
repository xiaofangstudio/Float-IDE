/* 插件清单与贡献点类型。
 *
 * 设计原则：能用「声明」解决的绝不要求写代码。
 * 大多数插件只需要在 plugin.json 里声明命令/片段/关键词，
 * 真正需要逻辑时才写 main.js（跑在受控 API 里）。
 */

export type PluginCommand = {
  /** 命令 id，全局唯一，建议 <插件id>.<名字> */
  id: string;
  title: string;
  /** 可选分类，会在命令面板里分组 */
  category?: string;
  /** 最短写法：直接运行一段 CW Script */
  run?: string;
  /** 往当前编辑器插入一段文本（支持 ${selection} ${file} ${project} 占位） */
  insert?: string;
  /** 调用 main.js 里注册的同名函数 */
  handler?: string;
  when?: 'editor' | 'always';
};

export type PluginSnippet = {
  prefix: string;
  body: string;
  description?: string;
};

export type MenuItemContrib = { command: string; label: string };
export type MenuContrib = { tools?: MenuItemContrib[] };

export type PluginContributes = {
  commands?: PluginCommand[];
  snippets?: PluginSnippet[];
  /** 补全关键词（语言中立，会并入 .cw 补全） */
  keywords?: { label: string; detail?: string; insert?: string }[];
  /** 工具菜单里加一项，点击执行命令。
   *  对象形态是规范写法（{ tools: [...] }）；数组形态是老写法，仍然兼容，
   *  免得用户照着旧例子写就整个插件加载失败。 */
  menus?: MenuContrib | MenuContrib[];
  /** 状态栏提示（只读展示） */
  statusBar?: { text: string }[];
};

export type PluginManifest = {
  id: string;
  name: string;
  version: string;
  publisher?: string;
  description?: string;
  /** 有 main.js 才需要 */
  main?: string;
  /** 语义化提示，不满足只警告不禁用 */
  engines?: { float?: string };
  icon?: string;
  homepage?: string;
  tags?: string[];
  contributes?: PluginContributes;
  /** 需要的权限，缺省视为最小权限 */
  permissions?: PluginPermission[];
};

export type PluginPermission =
  | 'editor:read'
  | 'editor:write'
  | 'files:read'
  | 'files:write'
  | 'console:write'
  | 'language:run'
  | 'ui:notify';

export type PluginRecord = {
  id: string;
  dir: string;
  manifest: PluginManifest | null;
  error: string | null;
  enabled: boolean;
  /** 内置示例插件（可禁用，不建议卸载） */
  builtin?: boolean;
};

/** 插件可用的 API（注入到 main.js 的 module.exports 里） */
export type PluginApi = {
  plugin: { id: string; name: string; version: string; storage: Storage };
  console: { print: (msg: string) => void; error: (msg: string) => void };
  editor: {
    getText: () => string;
    setText: (t: string) => void;
    insert: (t: string) => void;
    getSelection: () => string;
    getPath: () => string | null;
    getCursor: () => { line: number; col: number } | null;
  };
  files: {
    read: (rel: string) => Promise<string>;
    write: (rel: string, text: string) => Promise<void>;
    list: (rel?: string) => Promise<string[]>;
  };
  language: { run: (code: string) => Promise<{ out: string[]; error: string | null }> };
  commands: {
    register: (id: string, fn: () => void | Promise<void>) => void;
    execute: (id: string) => Promise<void>;
  };
  ui: { notify: (msg: string) => void; status: (msg: string) => void };
};

export type PluginStorage = {
  get: <T = unknown>(key: string, fallback?: T) => T;
  set: (key: string, value: unknown) => void;
};
