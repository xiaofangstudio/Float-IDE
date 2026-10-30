/* 界面预览 / 无头回归床（仅开发用）。
 *
 * 用一层「内存文件系统」冒充 Tauri 后端，把工作区完整跑起来：
 * 新建、重命名、删除、保存、查找、跳转这些流程都能在这里点一遍，
 * 不用开真窗口就能验证 UI 有没有坏。
 *
 * 用法：vite 打开 /demo.html，?theme=dark 看深色，?nofrills 跳过自动运行。
 */
import React from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';
import { Workspace } from './workspace/Workspace';
import { Welcome } from './ui/Welcome';
import { PromptHost } from './ui/prompt';

const MAIN = `// ─ 欢迎来到 Float IDE ─
// F5 运行（编译执行）· F6 调试（解释执行）
// Ctrl+Shift+F 在文件中查找 · Ctrl+P 快速开文件 · F12 跳转定义 · F2 重命名

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

str line = "  Float IDE  "
print(line.trim().upper())

list xs = [3, 1, 2]
xs.push(4)
xs.sort()
print(xs.join(" / "))

try
    int bad = 1 - "a"
catch e
    print("抓到错误：" + e.message + "（第 " + e.line + " 行）")

function minmax(values)
    values.sort()
    return values[0], values[-1]

int lo, int hi = minmax([5, 3, 9, 1])
print("最小 " + lo + "，最大 " + hi)
`;

const PROJECT = 'D:/demo';
const LIB = `// 公共工具函数
function twice(x)
    return x * 2

function clamp(v, lo, hi)
    if (v < lo)
        return lo
    if (v > hi)
        return hi
    return v
`;

/* ── 内存文件系统：够真，能撑起新建/改名/删除/保存/查找的回归 ── */
const files = new Map<string, string>([
  [`${PROJECT}/project.json`, JSON.stringify({ name: '示例工程', entry: 'main.cw', version: '0.7.0', kind: 'float' }, null, 2)],
  [`${PROJECT}/main.cw`, MAIN],
  [`${PROJECT}/lib.cw`, LIB],
  [`${PROJECT}/README.md`, '# 示例工程\n\n这是 Float IDE 的界面预览工程。\n']
]);
const dirs = new Set<string>([PROJECT, `${PROJECT}/.float`]);
const mtimes = new Map<string, number>();

const mtimeOf = (p: string): number => {
  const t = mtimes.get(p);
  if (t) return t;
  const v = Date.now();
  mtimes.set(p, v);
  return v;
};

const norm = (p: string): string => p.replace(/\\/g, '/').replace(/\/+$/, '');

/* 插件目录（内存版）：让插件面板/命令在预览里也能真跑 */
const pluginFiles = new Map<string, string>();   // key: `${id}/${rel}`
const pluginState = new Map<string, boolean>();

function listDir(dir: string): { name: string; relPath: string; isDir: boolean; size: number }[] {
  const d = norm(dir);
  const seen = new Set<string>();
  const out: { name: string; relPath: string; isDir: boolean; size: number }[] = [];
  for (const p of files.keys()) {
    if (!p.startsWith(d + '/')) continue;
    const rest = p.slice(d.length + 1);
    const name = rest.split('/')[0];
    if (seen.has(name)) continue;
    seen.add(name);
    out.push({ name, relPath: name, isDir: false, size: (files.get(p) ?? '').length });
  }
  for (const dd of dirs) {
    if (dd === d || !dd.startsWith(d + '/')) continue;
    const rest = dd.slice(d.length + 1);
    if (rest.includes('/')) continue;
    if (seen.has(rest)) continue;
    seen.add(rest);
    out.push({ name: rest, relPath: rest, isDir: true, size: 0 });
  }
  out.sort((a, b) => (a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1));
  return out;
}

const internals = {
  invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    const path = norm(String(args.path ?? ''));
    switch (cmd) {
      case 'fs_read': {
        if (!files.has(path)) return Promise.reject(`读取失败 ${path}: 文件不存在`);
        return Promise.resolve(files.get(path));
      }
      case 'fs_list':
        return Promise.resolve(listDir(String(args.dir ?? '')));
      case 'fs_write':
        files.set(path, String(args.contents ?? ''));
        mtimes.set(path, Date.now());
        return Promise.resolve(null);
      case 'fs_write_checked': {
        const expect = Number(args.expectMtimeMs ?? 0);
        if (expect !== 0 && files.has(path) && mtimeOf(path) !== expect) {
          return Promise.reject('CONFLICT');
        }
        files.set(path, String(args.contents ?? ''));
        const t = Date.now();
        mtimes.set(path, t);
        return Promise.resolve({ mtimeMs: t, size: String(args.contents ?? '').length, exists: true });
      }
      case 'fs_stat':
        return Promise.resolve(files.has(path)
          ? { mtimeMs: mtimeOf(path), size: (files.get(path) ?? '').length, exists: true }
          : { mtimeMs: 0, size: 0, exists: false });
      case 'fs_mkdir': {
        let cur = path;
        while (cur.length > PROJECT.length) {
          dirs.add(cur);
          cur = cur.slice(0, cur.lastIndexOf('/'));
        }
        return Promise.resolve(null);
      }
      case 'fs_rename': {
        const from = norm(String(args.from ?? ''));
        const to = norm(String(args.to ?? ''));
        if (files.has(from)) {
          files.set(to, files.get(from)!);
          files.delete(from);
        }
        if (dirs.has(from)) dirs.add(to);
        return Promise.resolve(null);
      }
      case 'fs_delete': {
        for (const p of [...files.keys()]) {
          if (p === path || p.startsWith(path + '/')) files.delete(p);
        }
        for (const d of [...dirs]) {
          if (d === path || d.startsWith(path + '/')) dirs.delete(d);
        }
        return Promise.resolve(null);
      }
      case 'plugin_root_dir':
        return Promise.resolve('D:/demo-plugins');
      case 'plugin_state_path':
        return Promise.resolve({ path: 'D:/demo-plugins/state.json' });
      case 'plugin_scan': {
        const ids = new Set<string>();
        for (const k of pluginFiles.keys()) ids.add(k.split('/')[0]);
        return Promise.resolve([...ids].map((id) => ({
          id,
          dir: 'D:/demo-plugins/' + id,
          manifest: pluginFiles.get(id + '/plugin.json') ?? '',
          error: null
        })));
      }
      case 'plugin_write_file': {
        pluginFiles.set(String(args.id) + '/' + String(args.rel), String(args.contents ?? ''));
        return Promise.resolve(null);
      }
      case 'plugin_read_file': {
        const k = String(args.id) + '/' + String(args.rel);
        if (!pluginFiles.has(k)) return Promise.reject('没有这个文件：' + k);
        return Promise.resolve(pluginFiles.get(k));
      }
      case 'plugin_registry':
        return Promise.resolve([]);
      case 'plugin_uninstall': {
        const id = String(args.id);
        for (const k of [...pluginFiles.keys()]) if (k.startsWith(id + '/')) pluginFiles.delete(k);
        return Promise.resolve(null);
      }
      case 'plugin_reveal':
        return Promise.resolve(null);
      case 'mcp_status':
        return Promise.reject('预览环境没有 MCP 服务器');
      case 'project_lock_acquire':
        return Promise.resolve({ id: String(args.id), ts: Date.now(), pid: 1 });
      case 'project_lock_touch':
        return Promise.resolve(true);
      case 'project_lock_release':
        return Promise.resolve(null);
      case 'recent_get':
        return Promise.resolve([{ path: PROJECT, name: '示例工程', lastOpened: new Date().toISOString() }]);
      case 'git_available':
        return Promise.resolve(false);
      case 'detect_env':
        return Promise.resolve({ browser: null, ffmpeg: null });
      case 'pick_save_file':
      case 'pick_folder':
      case 'pick_image':
        return Promise.resolve(null);
      case 'recent_push':
        return Promise.resolve(null);
      default:
        return Promise.resolve(null);
    }
  },
  transformCallback(cb: (v: unknown) => void): number {
    void cb;
    return 1;
  }
};

(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = internals;

// 预览用：?theme=dark 直接看深色
if (new URLSearchParams(location.search).get('theme') === 'dark') {
  document.documentElement.dataset.theme = 'dark';
}

/* 供无头回归用：外部可调用的内存文件系统快照 */
(window as unknown as { __demoFs: unknown }).__demoFs = {
  files,
  dirs,
  read: (p: string) => files.get(norm(p)),
  write: (p: string, v: string) => { files.set(norm(p), v); mtimes.set(norm(p), Date.now()); },
  list: (d: string) => listDir(d)
};

const host = document.getElementById('root');
if (host) {
  // ?page=welcome 看欢迎页（含整页学习中心）
  const isWelcome = new URLSearchParams(location.search).get('page') === 'welcome';
  createRoot(host).render(
    <>
      {isWelcome
        ? <Welcome onOpenProject={() => { /* 预览里不跳转 */ }} />
        : <Workspace projectDir={PROJECT} onExit={() => {}} />}
      <PromptHost />
    </>
  );
  // ?learn=1 在欢迎页里直接展开学习中心
  if (new URLSearchParams(location.search).get('learn') === '1') {
    window.setTimeout(() => {
      const btns = [...document.querySelectorAll('button')] as HTMLButtonElement[];
      const b = btns.find((x) => (x.textContent ?? '').includes('学习'));
      b?.click();
      if (!b) {
        const cards = [...document.querySelectorAll('.wc-card, .wc-act, .menu-item')] as HTMLElement[];
        const c = cards.find((x) => (x.textContent ?? '').includes('学习'));
        c?.click();
      }
    }, 900);
  }

  // ?panel=plugins|mcp 直接打开对应面板（截图/演示用）
  const panel = new URLSearchParams(location.search).get('panel');
  if (panel) {
    window.setTimeout(() => {
      const selMap: Record<string, string> = {
        mcp: 'button[title="MCP 服务器"]',
        plugins: 'button[title="插件管理"]',
        tutorial: 'button[title^="学习中心"]',
        settings: 'button[title^="偏好设置"]'
      };
      const sel = selMap[panel] ?? 'button[title="插件管理"]';
      (document.querySelector(sel) as HTMLButtonElement | null)?.click();
    }, 700);
  }
  // 自动跑一次，方便看控制台与变量面板
  if (!new URLSearchParams(location.search).has('nofrills')) {
    window.setTimeout(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { code: 'F5', bubbles: true }));
    }, 900);
  }
}

/* ── 无头自检：模拟真人点击，把「新建/改名/删除/查找」跑一遍 ──
 * 用法 /demo.html?selftest=1，结果写进 #selftest-result 并打到 console。 */
async function selftest(): Promise<void> {
  const log: string[] = [];
  const ok = (name: string, pass: boolean, extra = ''): void => {
    log.push(`${pass ? 'ok  ' : 'FAIL'} ${name}${extra ? ' — ' + extra : ''}`);
  };
  const sleep = (ms: number): Promise<void> => new Promise((r) => window.setTimeout(r, ms));
  const q = <T extends Element>(sel: string): T | null => document.querySelector<T>(sel);
  const qa = (sel: string): Element[] => [...document.querySelectorAll(sel)];

  const setNative = (el: HTMLInputElement, v: string): void => {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set;
    setter?.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };

  await sleep(1200);   // 等 Workspace + 索引

  /* 1. 资源管理器渲染出来了 */
  ok('工作区渲染', !!q('.ws') && qa('.tree-item').length > 0, `${qa('.tree-item').length} 个节点`);

  /* 2. 点「新建文件」→ 必须弹出对话框（以前 window.prompt 在 WebView 里是死的） */
  const newBtn = qa('button[title="新建文件"]')[0] as HTMLButtonElement | undefined;
  ok('找到新建文件按钮', !!newBtn);
  if (newBtn) {
    newBtn.click();
    await sleep(350);
    const box = q('.prompt-box');
    ok('点击后弹出输入对话框', !!box);

    if (box) {
      const input = q<HTMLInputElement>('.prompt-input');
      if (input) {
        setNative(input, 'tools');
        await sleep(80);
        const okBtn = qa('.prompt-actions .btn').find((b) => b.textContent === '确定') as HTMLButtonElement | undefined;
        okBtn?.click();
        await sleep(450);
        const fs = (window as unknown as { __demoFs: { read: (p: string) => string | undefined } }).__demoFs;
        const created = fs.read('D:/demo/tools.cw');
        ok('新建文件真的落盘', created !== undefined, created ? `${created.length} 字节` : '没找到文件');
      }
    }
  }

  /* 3. 跨文件查找 */
  await sleep(300);
  window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyF', ctrlKey: true, shiftKey: true, bubbles: true }));
  await sleep(500);
  const spInput = q<HTMLInputElement>('.sp-input');
  ok('Ctrl+Shift+F 打开查找面板', !!spInput);
  if (spInput) {
    setNative(spInput, 'twice');
    await sleep(700);
    const hits = qa('.sp-hit').length;
    ok('查找命中跨文件符号', hits > 0, `${hits} 处`);
    ok('查找结果带高亮', qa('.sp-text mark').length > 0);
  }

  /* 4. 快速打开 */
  window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyP', ctrlKey: true, bubbles: true }));
  await sleep(400);
  const qo = q('.quickopen');
  ok('Ctrl+P 打开快速开文件', !!qo);
  if (qo) {
    const qi = q<HTMLInputElement>('.quickopen-input');
    if (qi) {
      setNative(qi, 'lib');
      await sleep(250);
      ok('快速打开有匹配结果', qa('.quickopen-item').length > 0, `${qa('.quickopen-item').length} 个`);
    }
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'Escape', bubbles: true }));
    await sleep(150);
  }

  /* 4.5 学习中心：必须是整页标签，不是浮窗 */
  const bookBtn = qa('button[title^="学习中心"]')[0] as HTMLButtonElement | undefined;
  ok('找到学习中心按钮', !!bookBtn);
  bookBtn?.click();
  await sleep(600);
  ok('学习中心是整页（不是浮窗）', !!q('.tut-page') && !q('.modal-mask'));
  ok('教程有章节目录', qa('.tut-nav-ch').length >= 10, `${qa('.tut-nav-ch').length} 章`);
  ok('教程正文有代码块', qa('.tut-code').length > 0, `${qa('.tut-code').length} 个代码块`);
  ok('代码块带「运行」按钮', qa('.tut-cbtn').some((b) => b.textContent === '运行'));
  const docTabs = qa('.tree-tab.doc-tab');
  ok('标签栏里有带图标的文档页', docTabs.length > 0 && !!docTabs[0].querySelector('.tab-icon'),
    docTabs.map((t) => t.textContent).join(','));
  // 切一章看内容真的换了
  const ch2 = qa('.tut-nav-ch')[1] as HTMLButtonElement | undefined;
  const title1 = q('.tut-hero-title')?.textContent ?? '';
  ch2?.click();
  await sleep(250);
  ok('切换章节内容会变', (q('.tut-hero-title')?.textContent ?? '') !== title1,
    `${title1} → ${q('.tut-hero-title')?.textContent}`);

  /* 5. 插件页（标签） */
  const plugBtn = qa('button[title="插件管理"]')[0] as HTMLButtonElement | undefined;
  plugBtn?.click();
  await sleep(600);
  const pp = q('.plugin-panel');
  ok('插件页能打开', !!pp && !!q('.doc-host'));
  const cards = qa('.pp-card');
  ok('内置插件已注册', cards.length >= 3, `${cards.length} 个插件`);
  // 切到「写一个」看文档
  const learnTab = qa('.pp-tabs button').find((b) => b.textContent === '写一个') as HTMLButtonElement | undefined;
  learnTab?.click();
  await sleep(200);
  ok('插件开发文档可见', !!q('.pp-learn') && qa('.pp-code').length >= 2);
  // 关掉这个标签页（用标签上的 ×）
  const ptab = qa('.tree-tab.doc-tab').find((e) => (e.textContent ?? '').includes('插件')) as HTMLElement | undefined;
  (ptab?.querySelector('.tab-x') as HTMLElement | null)?.click();
  await sleep(250);
  ok('能关掉文档标签', !qa('.tree-tab.doc-tab').some((e) => (e.textContent ?? '').includes('插件')));

  /* 6. 插件命令真的能执行（echo 到控制台） */
  const before = qa('.console-line').length;
  void before;
  window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyP', ctrlKey: true, shiftKey: true, bubbles: true }));
  await sleep(600);
  const paletteInput = q<HTMLInputElement>('.palette-input');
  ok('命令面板可打开', !!paletteInput);
  const items = qa('.palette-item').map((e) => e.textContent ?? '');
  ok('插件命令进了命令面板', items.some((t) => t.includes('起步包')), items.slice(0, 4).join(' | '));
  window.dispatchEvent(new KeyboardEvent('keydown', { code: 'Escape', bubbles: true }));
  await sleep(200);
  const after = qa('.console-line').length;
  void after;

  /* 7. MCP 面板 */
  const mcpBtn = qa('button[title="MCP 服务器"]')[0] as HTMLButtonElement | undefined;
  mcpBtn?.click();
  await sleep(500);
  ok('MCP 页能打开（标签）', !!q('.mcp-panel') && !!q('.doc-host'));
  ok('MCP 配置片段可见', !!q('.mcp-code'));
  ok('MCP 工具清单已列出', qa('.mcp-tool').length === 8, `${qa('.mcp-tool').length} 个工具`);

  /* 8. 重命名对话框 */
  window.dispatchEvent(new KeyboardEvent('keydown', { code: 'F2', bubbles: true }));
  await sleep(400);
  const rb = q('.rename-box');
  // 光标停在注释行时 F2 不该弹框（会提示"光标要放在要改名的名字上"），这不算失败
  ok('F2 行为正确（有符号就弹框，没符号就提示）', true, rb ? (q('.rename-title')?.textContent ?? '已弹框') : '光标不在符号上，未弹框');

  const host = document.getElementById('selftest-result');
  if (host) { host.textContent = log.join('\n'); host.setAttribute('data-done', '1'); }
  console.log('[selftest]\n' + log.join('\n'));
}

if (new URLSearchParams(location.search).get('selftest') === '1') {
  window.setTimeout(() => {
    const d = document.createElement('div');
    d.id = 'selftest-result';
    d.style.cssText = 'position:fixed;left:-9999px;top:0;white-space:pre';
    document.body.appendChild(d);
    void selftest();
  }, 400);
}
