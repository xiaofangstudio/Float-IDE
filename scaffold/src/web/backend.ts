/* Float IDE · Web 版后端
 * ---------------------------------------------------------------------------
 * 桌面端这套 Workspace 是靠 Tauri 的 invoke 跟 Rust 说话的（文件系统 / 语言内核 /
 * 终端 / Git）。搬到 Nexus OS 26 里没有 Rust，所以这里做一个同形状的浏览器后端：
 *
 *   文件系统  —— 内存 Map 作权威 + IndexedDB 落盘（关掉窗口工程还在）
 *   语言内核  —— 直接跑 src/lang 里的 TS 解释器（Runner），F5/F6 真能出结果
 *   终端      —— 浏览器没有 ConPTY，退化成一条提示，不假装能用
 *   Git / MCP —— 浏览器里没有，明确报"不可用"，不静默返回空
 *
 * 对外仍然是 window.__TAURI_INTERNALS__.invoke(cmd, args)，
 * 所以 src/core/tauri.ts 一个字都不用改。
 * --------------------------------------------------------------------------- */
import { Runner } from '../lang/interp';
import { parseProgram } from '../lang/parse';
import { analyze } from '../lang/analyze';

/* ══ 一、文件系统 ═══════════════════════════════════════════════════════ */

const DB_NAME = 'float-ide-web';
const DB_STORE = 'fs';
const ROOT = 'D:/Nexus';
const norm = (p: string): string => p.replace(/\\/g, '/').replace(/\/+$/, '');

/** 权威副本在内存里（读写都是同步的，Workspace 用起来跟桌面端一样跟手），
 *  IndexedDB 只当持久化层：启动灌入、改动后防抖回写。 */
const files = new Map<string, string>();
const dirs = new Set<string>([ROOT]);
const mtimes = new Map<string, number>();

let dbPromise: Promise<IDBDatabase> | null = null;
function db(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => {
      const d = req.result;
      if (!d.objectStoreNames.contains(DB_STORE)) d.createObjectStore(DB_STORE);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbPromise;
}

type FsSnapshot = { files: [string, string][]; dirs: string[]; mtimes: [string, number][] };

/** 启动灌入：把上次会话的工程读回内存 */
export async function hydrate(): Promise<void> {
  try {
    const d = await db();
    const snap = await new Promise<FsSnapshot | null>((resolve) => {
      const tx = d.transaction(DB_STORE, 'readonly');
      const r = tx.objectStore(DB_STORE).get('snapshot');
      r.onsuccess = () => resolve((r.result as FsSnapshot) ?? null);
      r.onerror = () => resolve(null);
    });
    if (snap) {
      for (const [k, v] of snap.files) files.set(k, v);
      for (const k of snap.dirs) dirs.add(k);
      for (const [k, v] of snap.mtimes) mtimes.set(k, v);
      return;
    }
  } catch (e) {
    void e;
  }
  seedDemoProject();
}

let saveTimer: number | null = null;
function scheduleSave(): void {
  if (saveTimer !== null) return;
  saveTimer = window.setTimeout(() => {
    saveTimer = null;
    void persist();
  }, 400);
}
async function persist(): Promise<void> {
  try {
    const d = await db();
    const snap: FsSnapshot = {
      files: [...files.entries()],
      dirs: [...dirs],
      mtimes: [...mtimes.entries()]
    };
    const tx = d.transaction(DB_STORE, 'readwrite');
    tx.objectStore(DB_STORE).put(snap, 'snapshot');
  } catch (e) {
    void e;
  }
}

const mtimeOf = (p: string): number => {
  const t = mtimes.get(p);
  if (t) return t;
  const v = Date.now() - 60_000;
  mtimes.set(p, v);
  return v;
};

function listDir(dir: string): { name: string; relPath: string; isDir: boolean; size: number }[] {
  const d = norm(dir);
  const seen = new Set<string>();
  const out: { name: string; relPath: string; isDir: boolean; size: number }[] = [];
  for (const p of files.keys()) {
    if (!p.startsWith(d + '/')) continue;
    const rest = p.slice(d.length + 1);
    const parts = rest.split('/');
    const name = parts[0];
    if (seen.has(name)) continue;
    seen.add(name);
    if (parts.length > 1) out.push({ name, relPath: name, isDir: true, size: 0 });
    else out.push({ name, relPath: name, isDir: false, size: (files.get(p) ?? '').length });
  }
  for (const dd of dirs) {
    if (dd === d || !dd.startsWith(d + '/')) continue;
    const rest = dd.slice(d.length + 1);
    if (rest.includes('/') || seen.has(rest)) continue;
    seen.add(rest);
    out.push({ name: rest, relPath: rest, isDir: true, size: 0 });
  }
  out.sort((a, b) => (a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1));
  return out;
}

/* ══ 二、示例工程（第一次进来有东西可看） ═════════════════════════════════ */

const MAIN = `// ─ 欢迎来到 Float IDE · Nexus OS 26 版 ─
// F5 运行 · F6 调试 · Ctrl+P 快速开文件 · Ctrl+Shift+F 全局查找
// 语言叫 Copper，点顶栏「学习中心」有一整本教程

class Hero
    str name = ""
    int hp = 100

    function init(name, hp)
        self.name = name
        self.hp = hp

    function hurt(damage)
        hp -= damage
        if (hp < 0)
            hp = 0
        return hp

    function info()
        return name + " 剩余血量 " + hp

Hero hero = new Hero("阿方", 120)
print(hero.info())

for i in range(3)
    hero.hurt(25)
    print("第 " + (i + 1) + " 击：" + hero.info())

str line = "  float ide  "
print(line.trim().upper())

list xs = [3, 1, 2]
xs.push(4)
xs.sort()
print(xs.join(" / "))

try
    int bad = 1 - "a"
catch e
    print("抓到错误：" + e.message)

function minmax(values)
    values.sort()
    return values[0], values[-1]

int lo, int hi = minmax([5, 3, 9, 1])
print("最小 " + lo + "，最大 " + hi)
`;

const LIB = `// 公共工具函数（被 main.co import 到）
function twice(x)
    return x * 2

function clamp(v, lo, hi)
    if (v < lo)
        return lo
    if (v > hi)
        return hi
    return v
`;

export const DEMO_PROJECT = ROOT + '/hello';

function seedDemoProject(): void {
  const p = DEMO_PROJECT;
  dirs.add(p);
  dirs.add(p + '/.float');
  dirs.add(p + '/core');
  files.set(p + '/project.json', JSON.stringify(
    { name: 'hello', entry: 'main.co', version: '0.1.0', kind: 'float' }, null, 2));
  files.set(p + '/main.co', MAIN);
  files.set(p + '/lib.co', LIB);
  files.set(p + '/core/engine.co', '// 引擎\nfunction tick()\n    return 1\n');
  files.set(p + '/README.md', '# hello\n\n在 Nexus OS 26 里用 Copper 写的第一个工程。\n');
  for (const k of files.keys()) mtimes.set(k, Date.now() - 60_000);
}

/* ══ 三、语言内核：真跑一遍，把 output / error / 提问交回去 ══════════════ */

type LangResult = {
  output: string;
  reports: string[];
  error: string | null;
  error_line: number;
  steps: number;
  needInput: string | null;
};

/** 用 Runner（生成器解释器）跑到结束或撞上 ask()：
 *  桌面端靠 Rust 内核重放输入，浏览器端我们就一次性把已有 answers 喂进去。 */
function langRun(file: string, source: string, inputs: string[], extra: Record<string, string>): Promise<LangResult> {
  const entry = norm(file);
  const sources = new Map<string, string>();
  for (const [k, v] of files) sources.set(norm(k), v);
  for (const [k, v] of Object.entries(extra)) sources.set(norm(k), v);
  sources.set(entry, source);

  const out: string[] = [];
  const reports: string[] = [];
  const answers = [...inputs];
  let needInput: string | null = null;
  let errLine = 0;
  let errMsg: string | null = null;
  let steps = 0;

  // 先把 resolver 与结算函数造出来，再交给 Runner。
  // 顺序很要紧：Runner 在 start() 里遇到空文件/语法错误时会**同步**走到 finish，
  // 那时如果 done 还没定义，就会踩到 TDZ 直接抛 ReferenceError。
  let resolveFn: (r: LangResult) => void = () => {};
  let settled = false;
  let watchdog = 0;
  const done = (): void => {
    if (settled) return;
    settled = true;
    if (watchdog) { window.clearInterval(watchdog); watchdog = 0; }
    steps = (runner as unknown as { steps: number }).steps ?? 0;
    resolveFn({ output: out.join('\n'), reports, error: errMsg, error_line: errLine, steps, needInput });
  };

  const runner = new Runner(entry, {
    output: (l) => { out.push(l.text); },
    report: (t) => { reports.push(t); },
    onPaused: () => {},
    onDone: () => { done(); },
    onState: () => {},
    onError: (info) => {
      if (!errMsg) { errMsg = info.message; errLine = info.line; }
    }
  }, ROOT, sources);

  const p = new Promise<LangResult>((resolve) => { resolveFn = resolve; });
  runner.start('run');
  // Runner 是异步生成器驱动的：正常路径 onDone 会自己叫 done()。
  // 这里再挂一道轮询，负责把 ask() 的提问接出来，并兜住"既不 done 也不抛"的异常路径。
  watchdog = window.setInterval(() => {
    const ask = runner.askQuestion;
    if (ask !== null) {
      needInput = ask;
      const a = answers.shift();
      // 用户已经答过就把答案灌回去继续跑，没有就停在这里等 UI 收答案
      if (a === undefined) { done(); return; }
      needInput = null;
      runner.resume(a);
      return;
    }
    if (runner.state === 'done') { done(); }
  }, 16);
  window.setTimeout(done, 20_000);
  return p;
}

/** 静态检查：语法错误 + 语义诊断，形状跟 Rust 端一致 */
function langCheck(file: string, source: string): { severity: 'error' | 'warning' | 'info'; message: string; line: number; col: number }[] {
  const out: { severity: 'error' | 'warning' | 'info'; message: string; line: number; col: number }[] = [];
  try {
    const prog = parseProgram(norm(file), source);
    for (const d of prog.diagnostics) {
      out.push({ severity: d.severity, message: d.message, line: d.span.line, col: d.span.col });
    }
  } catch (e) {
    out.push({ severity: 'error', message: e instanceof Error ? e.message : String(e), line: 1, col: 1 });
  }
  try {
    // analyze 内部会再 parse 一遍，语法错误已经在上面收过了，这里只补语义诊断
    const a = analyze(norm(file), source);
    const seen = new Set(out.map((d) => `${d.line}:${d.col}:${d.message}`));
    for (const d of a.diagnostics) {
      const key = `${d.span.line}:${d.span.col}:${d.message}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ severity: d.severity, message: d.message, line: d.span.line, col: d.span.col });
    }
  } catch (e) { void e; }
  return out;
}

/* ══ 四、invoke 分发 ════════════════════════════════════════════════════ */

type Args = Record<string, unknown>;
type Recent = { path: string; name: string; lastOpened: string };

const recents: Recent[] = [];
const pluginFiles = new Map<string, string>();

export const internals = {
  invoke(cmd: string, args: Args): Promise<unknown> {
    const path = norm(String(args.path ?? ''));
    switch (cmd) {
      case 'fs_read':
        if (!files.has(path)) return Promise.reject(new Error(`读取失败 ${path}：文件不存在`));
        return Promise.resolve(files.get(path));
      case 'fs_list':
        return Promise.resolve(listDir(String(args.dir ?? '')));
      case 'fs_write':
        files.set(path, String(args.contents ?? ''));
        mtimes.set(path, Date.now());
        scheduleSave();
        return Promise.resolve(null);
      case 'fs_write_checked': {
        const expect = Number(args.expectMtimeMs ?? 0);
        if (expect !== 0 && files.has(path) && mtimeOf(path) !== expect) {
          return Promise.reject(new Error('CONFLICT'));
        }
        files.set(path, String(args.contents ?? ''));
        const t = Date.now();
        mtimes.set(path, t);
        scheduleSave();
        return Promise.resolve({ mtimeMs: t, size: String(args.contents ?? '').length, exists: true });
      }
      case 'fs_stat':
        return Promise.resolve(files.has(path)
          ? { mtimeMs: mtimeOf(path), size: (files.get(path) ?? '').length, exists: true }
          : { mtimeMs: 0, size: 0, exists: false });
      case 'fs_mkdir': {
        let cur = path;
        dirs.add(cur);
        while (cur.includes('/')) { cur = cur.slice(0, cur.lastIndexOf('/')); if (cur) dirs.add(cur); }
        scheduleSave();
        return Promise.resolve(null);
      }
      case 'fs_rename': {
        const from = norm(String(args.from ?? ''));
        const to = norm(String(args.to ?? ''));
        if (files.has(from)) { files.set(to, files.get(from)!); files.delete(from); }
        for (const d of [...dirs]) if (d === from || d.startsWith(from + '/')) { dirs.add(to + d.slice(from.length)); dirs.delete(d); }
        scheduleSave();
        return Promise.resolve(null);
      }
      case 'fs_delete': {
        for (const p of [...files.keys()]) if (p === path || p.startsWith(path + '/')) files.delete(p);
        for (const d of [...dirs]) if (d === path || d.startsWith(path + '/')) dirs.delete(d);
        scheduleSave();
        return Promise.resolve(null);
      }

      /* 语言内核 */
      case 'lang_run':
        return langRun(
          String(args.file ?? ''), String(args.source ?? ''),
          (args.inputs as string[] | undefined) ?? [],
          (args.extra as Record<string, string> | undefined) ?? {}
        );
      case 'lang_check':
        return Promise.resolve(langCheck(String(args.file ?? ''), String(args.source ?? '')));

      /* 工程 */
      case 'project_new': {
        const a = (args.args ?? {}) as { dir?: string; name?: string; files?: Record<string, string> };
        const dir = norm(String(a.dir ?? ROOT));
        const name = String(a.name ?? 'project');
        const target = `${dir}/${name}`;
        dirs.add(target);
        dirs.add(target + '/.float');
        const given = a.files ?? {};
        if (Object.keys(given).length === 0) {
          files.set(target + '/main.co', 'print("你好，Nexus OS 26")\n');
        } else {
          for (const [rel, text] of Object.entries(given)) {
            const fp = target + '/' + norm(rel);
            files.set(fp, text);
            let cur = fp.slice(0, fp.lastIndexOf('/'));
            while (cur.length >= target.length) { dirs.add(cur); cur = cur.slice(0, cur.lastIndexOf('/')); }
          }
        }
        files.set(target + '/project.json', JSON.stringify(
          { name, entry: 'main.co', version: '0.1.0', kind: 'float' }, null, 2));
        for (const k of files.keys()) if (!mtimes.has(k)) mtimes.set(k, Date.now());
        scheduleSave();
        return Promise.resolve(target);
      }
      case 'project_export_zip':
        return Promise.reject(new Error('浏览器版不支持导出 zip'));
      case 'project_import_zip':
        return Promise.reject(new Error('浏览器版不支持导入 zip'));
      case 'pick_folder':
        return Promise.resolve(null);
      case 'pick_open_file':
        return Promise.resolve(null);
      case 'startup_path':
        return Promise.resolve(null);
      case 'pick_save_file':
        return Promise.resolve(null);
      case 'pick_image':
        return Promise.resolve(null);
      case 'open_path':
        return Promise.resolve(null);
      case 'detect_env':
        return Promise.resolve({ browser: 'nexus', ffmpeg: null });

      case 'project_lock_acquire':
        return Promise.resolve({ id: String(args.id), ts: Date.now(), pid: 1 });
      case 'project_lock_touch':
        return Promise.resolve(true);
      case 'project_lock_release':
        return Promise.resolve(null);

      case 'recent_get':
        return Promise.resolve(recents.length
          ? recents
          : [{ path: DEMO_PROJECT, name: 'hello', lastOpened: new Date().toISOString() }]);
      case 'recent_push': {
        const p = norm(String(args.path ?? ''));
        const name = String(args.name ?? p.split('/').pop() ?? p);
        const i = recents.findIndex((r) => r.path === p);
        if (i >= 0) recents.splice(i, 1);
        recents.unshift({ path: p, name, lastOpened: new Date().toISOString() });
        return Promise.resolve(null);
      }
      case 'recent_remove':
        return Promise.resolve(null);

      /* 终端：浏览器没有 ConPTY，明确说不可用，不假装有 shell */
      case 'pty_create':
      case 'pty_write':
      case 'pty_resize':
      case 'pty_close':
        return Promise.reject(new Error('浏览器版没有系统终端（桌面端才有 ConPTY）'));

      /* 插件：内存里跑 */
      case 'plugin_root_dir':
        return Promise.resolve('D:/Nexus Plugins');
      case 'plugin_state_path':
        return Promise.resolve({ path: 'D:/Nexus Plugins/state.json' });
      case 'plugin_scan': {
        const ids = new Set<string>();
        for (const k of pluginFiles.keys()) ids.add(k.split('/')[0]);
        return Promise.resolve([...ids].map((id) => ({
          id, dir: 'D:/Nexus Plugins/' + id,
          manifest: pluginFiles.get(id + '/plugin.json') ?? '', error: null
        })));
      }
      case 'plugin_write_file':
        pluginFiles.set(String(args.id) + '/' + String(args.rel), String(args.contents ?? ''));
        return Promise.resolve(null);
      case 'plugin_read_file': {
        const k = String(args.id) + '/' + String(args.rel);
        if (!pluginFiles.has(k)) return Promise.reject(new Error('没有这个文件：' + k));
        return Promise.resolve(pluginFiles.get(k));
      }
      case 'plugin_registry':
        return Promise.resolve([]);
      case 'plugin_uninstall': {
        const id = String(args.id);
        for (const k of [...pluginFiles.keys()]) if (k.startsWith(id + '/')) pluginFiles.delete(k);
        return Promise.resolve(null);
      }
      case 'plugin_install_dir':
        return Promise.reject(new Error('浏览器版不支持安装本地插件目录'));
      case 'plugin_reveal':
        return Promise.resolve(null);

      /* MCP / Git：浏览器里没有，明确拒绝 */
      case 'mcp_start':
      case 'mcp_stop':
      case 'mcp_status':
      case 'mcp_log':
      case 'mcp_reply':
        return Promise.reject(new Error('浏览器版没有 MCP 服务器'));
      case 'git_available':
        return Promise.resolve(false);
      case 'git_run':
        return Promise.reject(new Error('浏览器版没有 Git'));

      default:
        return Promise.resolve(null);
    }
  },
  transformCallback(cb: (v: unknown) => void): number { void cb; return 1; }
};

/* ══ 五、@tauri-apps/api 的浏览器替身 ═══════════════════════════════════
 * 构建时靠 vite alias 把 @tauri-apps/api/* 指到这里，所以：
 *   - isTauri() 恒为 false → 标题栏的自绘窗口按钮、窗口缩放把手自动不画
 *     （Nexus 自己的窗口已经有交通灯，再画一套是两层）
 *   - Workspace 里的 canNative 为假 → 运行/调试走浏览器内的 TS 解释器
 * 这条路径比"伪造 __TAURI_INTERNALS__"干净：UI 是真的知道自己跑在浏览器里。
 * --------------------------------------------------------------------------- */
export function invoke<T = unknown>(cmd: string, args: Args = {}): Promise<T> {
  return internals.invoke(cmd, args) as Promise<T>;
}
/** 事件监听：浏览器版没有后端事件源，返回空取消函数即可 */
export function listen(_event: string, _cb: (e: unknown) => void): Promise<() => void> {
  void _event; void _cb;
  return Promise.resolve(() => {});
}
export function convertFileSrc(path: string): string { return path; }

/** 装到 window 上，让 src/core/tauri.ts 的 invoke 走这里 */
export function installBackend(): void {
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = internals;
  // 让 Runner 的 import 解析能拿到全部源码（跟桌面端 host.__floatSources 一致）
  (window as unknown as { __floatSources: Map<string, string> }).__floatSources = files;
}

/** 给「发布到应用商店」用：拿到整个工程的源码表 */
export function exportProject(projectDir: string): Record<string, string> {
  const d = norm(projectDir);
  const out: Record<string, string> = {};
  for (const [p, text] of files) {
    if (p.startsWith(d + '/')) out[p.slice(d.length + 1)] = text;
  }
  return out;
}

export function listProjects(): string[] {
  return [...dirs].filter((d) => d !== ROOT && !d.slice(ROOT.length + 1).includes('/') && files.has(d + '/project.json'));
}

export function writeFile(path: string, text: string): void {
  files.set(norm(path), text);
  mtimes.set(norm(path), Date.now());
  let cur = norm(path);
  while (cur.includes('/')) { cur = cur.slice(0, cur.lastIndexOf('/')); if (cur) dirs.add(cur); }
  scheduleSave();
}

export function readFile(path: string): string | undefined { return files.get(norm(path)); }
export function deletePath(path: string): void {
  const p = norm(path);
  for (const k of [...files.keys()]) if (k === p || k.startsWith(p + '/')) files.delete(k);
  for (const d of [...dirs]) if (d === p || d.startsWith(p + '/')) dirs.delete(d);
  scheduleSave();
}
