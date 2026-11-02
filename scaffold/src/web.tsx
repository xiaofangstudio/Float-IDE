/* Float IDE · Nexus OS 26 Web 版入口
 * ---------------------------------------------------------------------------
 * 与 demo.tsx（无头回归床）不同，这里是给真实用户用的：
 *   · 文件系统持久化到 IndexedDB，关掉窗口工程还在
 *   · F5 用浏览器版 Copper 内核真跑
 *   · 「发布到 Nexus 应用商店」把工程打包提交，过审后直接出现在商店里
 * 布局全部走 flex/百分比，宿主窗口多大它就多大，缩放不崩。
 * --------------------------------------------------------------------------- */
import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';
import { Workspace } from './workspace/Workspace';
import { Welcome } from './ui/Welcome';
import { PromptHost } from './ui/prompt';
import { installBackend, hydrate, exportProject, DEMO_PROJECT } from './web/backend';

/* ── Nexus 桥（运行在 iframe 里时宿主会注入 window.Nexus）── */
type NexusBridge = {
  notify?: (title: string, body: string, kind?: string) => void;
  openUrl?: (u: string) => void;
  setWindowTitle?: (t: string) => void;
};
const nexus = (): NexusBridge => (window as unknown as { Nexus?: NexusBridge }).Nexus ?? {};

function toast(text: string): void {
  const n = nexus();
  if (n.notify) { n.notify('Float IDE', text, 'info'); return; }
  console.log('[Float IDE]', text);
}

/* ── 发布到应用/插件市场 ─────────────────────────────────────────────────
 * 打包规则：工程里所有 .co / .cw / .json / .md 进 sources。
 *  · 工程根有 plugin.json → 按「插件」投递（进插件市场）
 *  · 否则按「应用」投递（过审后跑在 Nexus 受限应用框架里）
 * v2.9.52：先 POST 到云端市场（跨设备可见），失败再退回本地待审队列 —— 两条路都通。 */
const N26_STORE_BASE = 'https://ai.fanggame.company';

function publishToStore(projectDir: string): void {
  const sources = exportProject(projectDir);
  const meta = (() => {
    try { return JSON.parse(sources['project.json'] ?? '{}'); } catch (e) { void e; return {}; }
  })() as { name?: string; entry?: string; version?: string };
  const name = meta.name ?? projectDir.split('/').pop() ?? '未命名工程';
  const isPlugin = !!sources['plugin.json'];
  const entry = isPlugin ? 'plugin.json' : (meta.entry ?? 'main.co');
  if (!sources[entry]) { toast(`发布失败：工程里没有入口文件 ${entry}`); return; }

  const pkg = {
    id: 'float_' + name.replace(/[^a-zA-Z0-9_]/g, '_').toLowerCase() + '_' + Date.now().toString(36),
    kind: isPlugin ? 'plugin' : 'app',
    name,
    version: meta.version ?? '0.1.0',
    entry,
    icon: isPlugin ? 'fa-plug' : 'fa-code',
    color: isPlugin ? '#0d9488' : '#0e7490',
    publisher: 'Float IDE 用户',
    submittedAt: new Date().toISOString(),
    desc: isPlugin
      ? `${name} —— Float IDE 插件（命令/片段/关键词扩展）`
      : `${name} —— 用 Copper 写的 Nexus 应用（Float IDE 发布）`,
    files: Object.keys(sources).length,
    bytes: Object.values(sources).reduce((s, t) => s + t.length, 0),
    sources
  };
  // ① 云端（跨设备；部署了市场后端才有）
  void (async (): Promise<void> => {
    try {
      const r = await fetch(N26_STORE_BASE + '/api/store/publish', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          kind: pkg.kind, name: pkg.name, desc: pkg.desc, version: pkg.version,
          publisher: pkg.publisher, icon: pkg.icon, color: pkg.color, entry: pkg.entry,
          files: sources
        })
      });
      const j = await r.json().catch(() => null) as { ok?: boolean; error?: string } | null;
      if (j && j.ok) toast(`「${name}」已提交到云端市场${isPlugin ? '（插件）' : ''}，等待审核`);
      else toast(`云端投递未受理${j && j.error ? '：' + j.error : ''}，已转本地审核队列`);
    } catch { toast('云端市场暂不可达，已转本地审核队列'); }
  })();
  // ② 本地待审队列（云端不在时审核照走）
  try {
    const ls = (window as unknown as { localStorage: Storage }).localStorage;
    const key = 'n26_store_pending';
    const list = JSON.parse(ls.getItem(key) ?? '[]') as unknown[];
    list.push(pkg);
    ls.setItem(key, JSON.stringify(list));
    if (!isPlugin) {
      // 云端提示是异步的；本地这条立刻给个确定反馈
      if (!navigator.onLine) toast(`「${name}」已提交审核（离线，存本地队列）`);
    }
  } catch (e) {
    void e;
    toast('发布失败：存储空间不足或被禁用');
  }
}

/* ── 根组件 ── */
function App(): React.ReactElement {
  const [ready, setReady] = useState(false);
  const [project, setProject] = useState<string | null>(null);
  const [pubOpen, setPubOpen] = useState(false);

  useEffect(() => {
    let alive = true;
    void (async () => {
      await hydrate();
      if (alive) setReady(true);
    })();
    const n = nexus();
    n.setWindowTitle?.('Float IDE');
    return () => { alive = false; };
  }, []);

  if (!ready) {
    return (
      <div style={{
        height: '100vh', display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center', gap: 14,
        background: 'var(--s0, #16181d)', color: 'var(--t2, #9aa3b2)'
      }}>
        <div style={{ width: 42, height: 42, borderRadius: 10, background: 'linear-gradient(135deg,#0e7490,#155e75)',
          display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#fff', fontWeight: 800, fontSize: 20 }}>
          F
        </div>
        <div style={{ fontSize: 13 }}>正在载入工作区…</div>
      </div>
    );
  }

  return (
    <>
      {project
        ? <Workspace projectDir={project} onExit={() => setProject(null)} />
        : <Welcome onOpenProject={(dir) => setProject(dir)} />}
      <PromptHost />
      {/* 发布入口：欢迎页与工作区都在，右下角固定悬浮，不占布局 */}
      <button
        className="floatweb-publish"
        title="把当前工程发布到 Nexus 应用商店（先本地审核，过审直接上架）"
        onClick={() => setPubOpen(true)}
      >
        ⬆ 发布
      </button>
      {pubOpen && (
        <div
          className="floatweb-mask"
          onClick={(e) => { if (e.target === e.currentTarget) setPubOpen(false); }}
        >
          <div className="floatweb-dialog">
            <div className="floatweb-dialog-title">发布到 Nexus 市场</div>
            <div className="floatweb-dialog-body">
              把工程打包提交审核（云端 + 本地双路）：
              <ul>
                <li>工程根有 <strong>plugin.json</strong> → 按<strong>插件</strong>投递（进插件市场）</li>
                <li>没有 → 按<strong>受限应用</strong>投递：运行在固定框架里，<strong>绝不会超出框架边界</strong></li>
                <li>过审后直接出现在 Nexus OS 26 的应用商店 / 插件市场里</li>
              </ul>
              <label className="floatweb-field">
                选择工程
                <select id="floatweb-pub-project">
                  <option value={DEMO_PROJECT}>{DEMO_PROJECT.split('/').pop()}（默认工程）</option>
                </select>
              </label>
            </div>
            <div className="floatweb-dialog-actions">
              <button className="btn" onClick={() => setPubOpen(false)}>取消</button>
              <button
                className="btn primary"
                onClick={() => {
                  const sel = document.getElementById('floatweb-pub-project') as HTMLSelectElement | null;
                  publishToStore(sel?.value ?? DEMO_PROJECT);
                  setPubOpen(false);
                }}
              >提交审核</button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

/* ── 挂载前先把后端装好（invoke 必须在模块求值前就位） ── */
installBackend();

const host = document.getElementById('root');
if (host) {
  document.documentElement.classList.add('floatweb');
  // 高度链：Nexus 的 iframe 多高，这里就多高，内部全部 flex 撑开
  const ensure = document.createElement('style');
  ensure.textContent = `
    html.floatweb, html.floatweb body, html.floatweb #root { height: 100%; overflow: hidden; }
    .floatweb-publish {
      position: fixed; right: 16px; bottom: 16px; z-index: 90;
      border: 1px solid var(--s3, rgba(255,255,255,.14));
      background: linear-gradient(135deg,#0e7490,#155e75);
      color: #fff; font-size: 12.5px; font-weight: 600;
      padding: 8px 14px; border-radius: 8px; cursor: pointer;
      box-shadow: 0 6px 18px rgba(0,0,0,.35);
    }
    .floatweb-publish:hover { filter: brightness(1.12); }
    .floatweb-mask {
      position: fixed; inset: 0; z-index: 120; background: rgba(0,0,0,.45);
      display: flex; align-items: center; justify-content: center;
    }
    .floatweb-dialog {
      width: min(430px, calc(100vw - 48px));
      background: var(--s1, #1d2026); color: var(--t1, #e6e9ef);
      border: 1px solid var(--s3, rgba(255,255,255,.12));
      border-radius: 10px; overflow: hidden;
      box-shadow: 0 18px 50px rgba(0,0,0,.5);
    }
    .floatweb-dialog-title { padding: 13px 16px; font-size: 14px; font-weight: 700; border-bottom: 1px solid var(--s2, rgba(255,255,255,.08)); }
    .floatweb-dialog-body { padding: 14px 16px; font-size: 12.5px; line-height: 1.7; color: var(--t2, #aab2c0); }
    .floatweb-dialog-body ul { margin: 8px 0 12px; padding-left: 18px; }
    .floatweb-field { display: flex; flex-direction: column; gap: 6px; font-size: 12px; }
    .floatweb-field select {
      background: var(--s0, #16181d); color: var(--t1, #e6e9ef);
      border: 1px solid var(--s3, rgba(255,255,255,.14));
      border-radius: 6px; padding: 7px 9px; font-size: 12.5px;
    }
    .floatweb-dialog-actions { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--s2, rgba(255,255,255,.08)); }
  `;
  document.head.appendChild(ensure);
  createRoot(host).render(<App />);
}
