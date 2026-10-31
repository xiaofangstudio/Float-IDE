import React, { useCallback, useEffect, useState } from 'react';
import { api } from '../core/tauri';
import type { PluginHost } from './runtime';
import type { PluginRecord } from './types';
import { askConfirm } from '../ui/prompt';
import { IconPlus, IconSearch } from '../ui/icons';

/* 插件管理面板：已装列表 / 启用禁用 / 安装 / 卸载 / 打开插件目录。
 * 同时把「插件生态」说清楚——插件长什么样、放哪儿、怎么写。 */

type Props = {
  host: PluginHost;
  onChanged: () => void;
  onClose: () => void;
  /** 作为整页标签打开时为 true：去掉面板外框，铺满 */
  embedded?: boolean;
};

type RegEntry = {
  id: string; name: string; version: string; publisher: string;
  description: string; tags: string[]; localPath: string;
};

export function PluginPanel({ host, onChanged, onClose, embedded }: Props) {
  const [records, setRecords] = useState<PluginRecord[]>([]);
  const [busy, setBusy] = useState('');
  const [tab, setTab] = useState<'installed' | 'market' | 'learn'>('installed');
  const [registry, setRegistry] = useState<RegEntry[]>([]);
  const [q, setQ] = useState('');

  const refresh = useCallback((): void => {
    setRecords([...host.list]);
  }, [host]);

  useEffect(() => {
    refresh();
    const off = host.onChange(refresh);
    return off;
  }, [host, refresh]);

  useEffect(() => {
    if (tab !== 'market') return;
    void api.pluginRegistry().then(setRegistry).catch(() => setRegistry([]));
  }, [tab]);

  const toggle = async (r: PluginRecord): Promise<void> => {
    setBusy(r.id);
    try {
      await host.setEnabled(r.id, !r.enabled);
      onChanged();
    } finally {
      setBusy('');
    }
  };

  const install = async (): Promise<void> => {
    const dir = await api.pickFolder('选择插件文件夹（里面要有 plugin.json）');
    if (!dir) return;
    setBusy('install');
    try {
      const id = await api.pluginInstallDir(dir);
      await host.reload();
      onChanged();
      setTab('installed');
      void id;
    } catch (e) {
      // 用控制台提示，比 alert 可靠
      console.error(e);
      window.dispatchEvent(new CustomEvent('float.notify', { detail: '安装失败：' + String(e) }));
    } finally {
      setBusy('');
    }
  };

  const remove = async (r: PluginRecord): Promise<void> => {
    const yes = await askConfirm(`卸载插件「${r.manifest?.name ?? r.id}」？`, {
      hint: '插件目录会被整个删除，插件保存的设置也会一起清掉。',
      okText: '卸载', danger: true
    });
    if (!yes) return;
    setBusy(r.id);
    try {
      await api.pluginUninstall(r.id);
      await host.reload();
      onChanged();
    } finally {
      setBusy('');
    }
  };

  const filtered = records.filter((r) => {
    const k = q.trim().toLowerCase();
    if (!k) return true;
    const name = (r.manifest?.name ?? r.id).toLowerCase();
    return name.includes(k) || r.id.toLowerCase().includes(k) ||
      (r.manifest?.description ?? '').toLowerCase().includes(k);
  });

  return (
    <div className={`plugin-panel ${embedded ? 'embedded' : ''}`}>
      <div className="pp-head">
        <span className="pp-title">插件</span>
        <div className="pp-tabs">
          <button className={tab === 'installed' ? 'on' : ''} onClick={() => setTab('installed')}>
            已安装 <span className="pp-num">{records.filter((r) => r.enabled).length}</span>
          </button>
          <button className={tab === 'market' ? 'on' : ''} onClick={() => setTab('market')}>插件仓库</button>
          <button className={tab === 'learn' ? 'on' : ''} onClick={() => setTab('learn')}>写一个</button>
        </div>
        <span className="tool-grow" />
        <button className="pp-btn" onClick={() => void install()} disabled={busy === 'install'}>
          <IconPlus size={12} /> 从文件夹安装
        </button>
        <button className="pp-btn" onClick={() => void api.pluginReveal()}>打开插件目录</button>
        {!embedded && <button className="pp-x" onClick={onClose}>✕</button>}
      </div>

      {tab === 'installed' && (
        <>
          <div className="pp-search">
            <IconSearch size={13} />
            <input placeholder="搜索已安装的插件" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <div className="pp-list">
            {filtered.length === 0 && <div className="pp-empty">还没有装任何插件</div>}
            {filtered.map((r) => {
              const m = r.manifest;
              return (
                <div key={r.id} className={`pp-card ${r.error ? 'err' : ''} ${r.enabled ? '' : 'off'}`}>
                  <div className="pp-card-main">
                    <div className="pp-card-title">
                      <span className="pp-name">{m?.name ?? r.id}</span>
                      {m?.version && <span className="pp-ver">v{m.version}</span>}
                      {r.builtin && <span className="pp-tag builtin">内置示例</span>}
                      {(m?.tags ?? []).slice(0, 3).map((t) => <span key={t} className="pp-tag">{t}</span>)}
                      {!r.enabled && <span className="pp-tag off">已禁用</span>}
                    </div>
                    <div className="pp-desc">{m?.description ?? '（没有描述）'}</div>
                    <div className="pp-meta">
                      <code>{r.id}</code>
                      {m?.publisher && <span>· {m.publisher}</span>}
                      {m?.main && <span>· {m.main}</span>}
                      {(m?.permissions ?? []).length > 0 && (
                        <span className="pp-perms">权限：{(m?.permissions ?? []).join('、')}</span>
                      )}
                    </div>
                    {r.error && <div className="pp-err">{r.error}</div>}
                  </div>
                  <div className="pp-card-act">
                    <button
                      className={`pp-switch ${r.enabled ? 'on' : ''}`}
                      disabled={busy === r.id || !!r.error}
                      onClick={() => void toggle(r)}
                      title={r.enabled ? '点击禁用' : '点击启用'}
                    >
                      <span />
                    </button>
                    <button
                      className="pp-link danger"
                      disabled={busy === r.id || !r.dir}
                      onClick={() => void remove(r)}
                    >
                      卸载
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}

      {tab === 'market' && (
        <div className="pp-list">
          <div className="pp-note">
            插件仓库读取的是插件目录下的 <code>registry.json</code>。把它当成离线索引：
            团队内部把插件包放共享盘，写一份 registry 就能互相安装，不需要联网也不需要账号。
          </div>
          {registry.length === 0 && (
            <div className="pp-empty">
              还没有 registry.json。点「打开插件目录」，在里面新建 <code>registry.json</code> 即可。
              <pre className="pp-code">{`[
  {
    "id": "float.sql-helper",
    "name": "SQL 助手",
    "version": "1.0.0",
    "publisher": "数据组",
    "description": "把选中的 SQL 格式化成一段 CW Script",
    "tags": ["sql", "format"],
    "localPath": "D:/team/plugins/float.sql-helper"
  }
]`}</pre>
            </div>
          )}
          {registry.map((e) => (
            <div key={e.id} className="pp-card">
              <div className="pp-card-main">
                <div className="pp-card-title">
                  <span className="pp-name">{e.name}</span>
                  <span className="pp-ver">v{e.version}</span>
                  {(e.tags ?? []).map((t) => <span key={t} className="pp-tag">{t}</span>)}
                </div>
                <div className="pp-desc">{e.description}</div>
                <div className="pp-meta"><code>{e.id}</code>{e.publisher && <span>· {e.publisher}</span>}</div>
              </div>
              <div className="pp-card-act">
                <button
                  className="pp-btn"
                  disabled={!e.localPath || busy === e.id}
                  onClick={() => {
                    setBusy(e.id);
                    void api.pluginInstallDir(e.localPath)
                      .then(async () => { await host.reload(); onChanged(); })
                      .catch((err) => console.error(err))
                      .finally(() => setBusy(''));
                  }}
                >
                  {e.localPath ? '安装' : '未下载'}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {tab === 'learn' && (
        <div className="pp-learn">
          <h4>插件就是两个文件</h4>
          <p>
            一个文件夹里放 <code>plugin.json</code>（清单）和可选的 <code>main.js</code>（逻辑），
            丢进插件目录就装好了。不需要编译、不需要重启，禁用/启用立刻生效。
          </p>

          <h4>最小插件：只有清单</h4>
          <pre className="pp-code">{`{
  "id": "me.hello",
  "name": "打个招呼",
  "version": "1.0.0",
  "description": "往控制台打一行字",
  "contributes": {
    "commands": [
      {
        "id": "me.hello.say",
        "title": "打个招呼",
        "run": "print(\\"你好！\\")"
      }
    ]
  }
}`}</pre>
          <p className="pp-muted">
            命令里的 <code>run</code> 直接就是一段 CW Script，会被语言内核执行，输出进控制台。
            也可以用 <code>insert</code> 往编辑器里插文本，支持 <code>{'${file} ${path} ${project} ${date} ${time}'}</code> 占位符。
          </p>

          <h4>加逻辑：main.js</h4>
          <pre className="pp-code">{`// main.js —— 只能通过注入的 float 对象操作，拿不到 DOM 和 Node
module.exports = {
  activate(float) {
    float.commands.register('me.count', async () => {
      const files = await float.files.list('');
      float.console.print('共 ' + files.length + ' 个文件');
    });
  },
  commands: { count: async () => { /* 也可以在清单里用 handler 引用 */ } }
};`}</pre>

          <h4>可用的 API</h4>
          <table className="pp-api">
            <tbody>
              <tr><td><code>float.files.read / write / list</code></td><td>读写工程文件（需 <code>files:read/write</code> 权限）</td></tr>
              <tr><td><code>float.editor.getText / setText / insert</code></td><td>读写当前编辑器（需 <code>editor:read/write</code>）</td></tr>
              <tr><td><code>float.language.run(code)</code></td><td>跑一段 CW Script 拿输出（需 <code>language:run</code>）</td></tr>
              <tr><td><code>float.console.print / error</code></td><td>写控制台（需 <code>console:write</code>）</td></tr>
              <tr><td><code>float.ui.notify</code></td><td>界面提示（需 <code>ui:notify</code>）</td></tr>
              <tr><td><code>float.plugin.storage.get / set</code></td><td>插件私有键值存储</td></tr>
              <tr><td><code>float.commands.register / execute</code></td><td>注册与调用命令</td></tr>
            </tbody>
          </table>

          <h4>还能贡献什么</h4>
          <ul className="pp-ul">
            <li><code>contributes.snippets</code> —— 代码片段，输入前缀就展开（在编辑器里直接可用）</li>
            <li><code>contributes.keywords</code> —— 并进 .cw 的自动补全</li>
            <li><code>contributes.menus.tools</code> —— 在「工具」菜单里加一项</li>
            <li><code>contributes.statusBar</code> —— 状态栏的一行文字</li>
          </ul>

          <p className="pp-muted">
            内置的「起步片段包」是纯声明式的样板，「代码统计」「待办扫描」是脚本式样板，
            打开插件目录直接照着改最快。
          </p>
        </div>
      )}
    </div>
  );
}
