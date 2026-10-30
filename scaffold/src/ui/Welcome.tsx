import React, { useEffect, useMemo, useState } from 'react';
import { api } from '../core/tauri';
import { Runner } from '../lang/interp';
import { TutorialPage } from '../docs/TutorialPage';
import { TEMPLATES, type Template } from '../core/templates';
import { Logo, IconFolder, IconPlus, IconImport, IconClose, IconBook, IconImage, IconSearch } from './icons';
import { TitleBar } from './TitleBar';
import { MenuBar, type MenuGroup } from './MenuBar';
import { ShortcutsDialog } from './ShortcutsDialog';
import { SettingsDialog } from './SettingsDialog';
import { AppearanceDialog } from './AppearanceDialog';
import type { RecentItem } from '../core/types';

/* 欢迎页（对标 PyCharm 新 UI）：左边是项目列表，右边是居中大标题与操作卡片。 */

function TplThumb({ tpl, active }: { tpl: Template; active: boolean }) {
  const line = active ? '#7cb1ff' : '#3f5f9e';
  const soft = active ? '#cfe0ff' : '#7285ad';
  if (tpl.id === 'class') return (
    <svg width="150" height="80" viewBox="0 0 150 80">
      <rect x="10" y="16" width="46" height="46" rx="5" fill="none" stroke={line} strokeWidth="1.6" />
      <text x="33" y="44" fontSize={11} fill={soft} textAnchor="middle" fontFamily="Consolas, monospace">class</text>
      <rect x="70" y="16" width="46" height="46" rx="5" fill="none" stroke={line} strokeWidth="1.6" strokeDasharray="3 3" />
      <text x="93" y="44" fontSize={11} fill={line} textAnchor="middle" fontFamily="Consolas, monospace">extends</text>
      <path d="M132 24v28M126 28l6-6 6 6" stroke={soft} strokeWidth="1.4" fill="none" />
    </svg>);
  if (tpl.id === 'starter') return (
    <svg width="150" height="80" viewBox="0 0 150 80">
      <rect x="24" y="14" width="102" height="52" rx="4" fill="none" stroke="#1b3260" strokeWidth="1.5" />
      <text x="34" y="32" fontSize={10} fill={soft} fontFamily="Consolas, monospace">ask "…"</text>
      <text x="34" y="46" fontSize={10} fill={line} fontFamily="Consolas, monospace">print(ans)</text>
      <text x="34" y="60" fontSize={10} fill={soft} fontFamily="Consolas, monospace">for i in range(3)</text>
    </svg>);
  if (tpl.id === 'advanced') return (
    <svg width="150" height="80" viewBox="0 0 150 80">
      <rect x="20" y="12" width="56" height="10" rx={2} fill={line} />
      <rect x="20" y="28" width="40" height="10" rx={2} fill={line} opacity={0.55} />
      <rect x="20" y="44" width="48" height="10" rx={2} fill={line} opacity={0.35} />
      <rect x="92" y="20" width="38" height="30" rx="4" fill="none" stroke={line} strokeWidth="1.6" />
      <line x1="92" y1="30" x2="130" y2="30" stroke={line} strokeWidth="1" opacity={0.6} />
      <line x1="92" y1="40" x2="130" y2="40" stroke={line} strokeWidth="1" opacity={0.6} />
    </svg>);
  return (
    <svg width="150" height="80" viewBox="0 0 150 80">
      <rect x="24" y="16" width="102" height="48" rx="4" fill="none" stroke="#1b3260" strokeWidth="1.5" />
      <text x="75" y="38" fontSize={13} fill={soft} textAnchor="middle" fontWeight={600}>main.cw</text>
      <line x1="42" y1="52" x2="108" y2="52" stroke="#1b3260" strokeWidth="2" />
      <line x1="42" y1="52" x2="82" y2="52" stroke={line} strokeWidth="2" />
    </svg>
  );
}

function NewProjectDialog({ preset, onClose, onOpenProject }: {
  preset: Template;
  onClose: () => void;
  onOpenProject: (dir: string) => void;
}) {
  const [tplId, setTplId] = useState(preset.id);
  const [name, setName] = useState(preset.name);
  const [loc, setLoc] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');

  useEffect(() => {
    const def = window.localStorage.getItem('float.newProjectDir');
    if (def) setLoc(def);
  }, []);

  const tpl = TEMPLATES.find((t) => t.id === tplId) ?? TEMPLATES[0];

  const browse = async (): Promise<void> => {
    const dir = await api.pickFolder('选择工程存放文件夹');
    if (dir) {
      setLoc(dir);
      window.localStorage.setItem('float.newProjectDir', dir);
    }
  };

  const create = async (): Promise<void> => {
    if (!loc) { setErr('请选择工程存放位置'); return; }
    if (!name.trim()) { setErr('请填写工程名称'); return; }
    setBusy(true);
    setErr('');
    try {
      const files: Record<string, string> = {};
      for (const [k, v] of Object.entries(tpl.files)) {
        files[k] = k === 'project.json' ? v.replace(/"name": "[^"]*"/, `"name": "${name.trim()}"`) : v;
      }
      const projDir = await api.projectNew(loc, name.trim(), files);
      onOpenProject(projDir);
    } catch (e) {
      setErr('创建失败：' + String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mask" onMouseDown={onClose}>
      <div className="dialog npd" onMouseDown={(e) => e.stopPropagation()}>
        <div className="dlg-head"><span>新建工程</span><button className="iconbtn" onClick={onClose}>✕</button></div>
        <div className="npd-tpls">
          {TEMPLATES.map((t) => (
            <div key={t.id} className={`npd-tpl ${t.id === tplId ? 'active' : ''}`} onClick={() => setTplId(t.id)}>
              <div className="npd-thumb"><TplThumb tpl={t} active={t.id === tplId} /></div>
              <div className="npd-tplname">{t.name}</div>
              <div className="npd-tpldesc">{t.desc}</div>
            </div>
          ))}
        </div>
        <div className="form-row">
          <label>工程名称</label>
          <input type="text" className="grow" value={name} onChange={(e) => setName(e.target.value)} placeholder="我的脚本" />
        </div>
        <div className="form-row">
          <label>存放位置</label>
          <input type="text" className="grow" value={loc} onChange={(e) => setLoc(e.target.value)} placeholder="选择文件夹……" />
          <button className="btn" onClick={() => void browse()}>浏览</button>
        </div>
        {err && <div style={{ color: 'var(--danger)', fontSize: 12.5 }}>{err}</div>}
        <div className="dlg-foot">
          <button className="btn" onClick={onClose}>取消</button>
          <button className="btn primary" disabled={busy} onClick={() => void create()}>{busy ? '正在创建……' : '创建'}</button>
        </div>
      </div>
    </div>
  );
}

/** 在欢迎页里跑一段教学代码（不依赖工程） */
async function runOnce(code: string): Promise<{ out: string[]; error: string | null }> {
  const out: string[] = [];
  let error: string | null = null;
  const file = '__tutorial__.cw';
  const sources = new Map<string, string>([[file, code]]);
  await new Promise<void>((done) => {
    let settled = false;
    const finish = (): void => { if (!settled) { settled = true; done(); } };
    const r = new Runner(file, {
      output: (l) => { if (l.kind === 'out') out.push(l.text); else if (l.kind === 'err') error = l.text; },
      onError: (e) => { error = e.message; },
      onState: () => {},
      onPaused: () => { window.setTimeout(() => r.resume(), 0); },
      onDone: finish
    }, '.', sources);
    try { r.start('run'); } catch (e) { error = String(e); finish(); }
    window.setTimeout(finish, 8000);
  });
  return { out, error };
}

/** 欢迎页里的学习中心：整页铺开（不是那个简陋的弹窗）。
 *  这里没有编辑器，所以不显示「在编辑器中打开」；运行结果直接显示在代码块下面。 */
function LearnPage({ onClose }: { onClose: () => void }) {
  return (
    <div className="learn-page">
      <div className="learn-page-head">
        <button className="btn" onClick={onClose}>← 返回欢迎页</button>
        <span className="tool-grow" />
        <span className="learn-page-hint">打开工程后按 Ctrl+Shift+H 随时回到这里</span>
      </div>
      <div className="learn-page-body">
        <TutorialPage
          onRun={async (code) => {
            const res = await runOnce(code);
            return res.error ? '错误：' + res.error : (res.out.join('\n') || '（没有输出）');
          }}
        />
      </div>
    </div>
  );
}


export function Welcome({ onOpenProject }: { onOpenProject: (dir: string) => void }) {
  const [recents, setRecents] = useState<RecentItem[]>([]);
  const [busy, setBusy] = useState('');
  const [newTpl, setNewTpl] = useState<Template | null>(null);
  const [showAppearance, setShowAppearance] = useState(false);
  const [learn, setLearn] = useState(false);
  const [shortcuts, setShortcuts] = useState(false);
  const [settings, setSettings] = useState(false);
  const [q, setQ] = useState('');
  const [err, setErr] = useState('');

  useEffect(() => {
    api.recentGet().then(setRecents).catch(() => setRecents([]));
  }, []);

  const filtered = useMemo(() => {
    const k = q.trim().toLowerCase();
    if (!k) return recents;
    return recents.filter((r) => r.name.toLowerCase().includes(k) || r.path.toLowerCase().includes(k));
  }, [recents, q]);

  const openExisting = async (): Promise<void> => {
    const dir = await api.pickFolder('选择工程文件夹（含 project.json）');
    if (!dir) return;
    onOpenProject(dir);
  };

  const importZip = async (): Promise<void> => {
    const zip = await api.pickSaveFile('选择 .cw 工程包', '', 'cw');
    if (!zip) return;
    setBusy('正在导入……');
    try {
      const destRoot = zip.replace(/[\\/]+$/, '').replace(/\.(cw|floatzip|float)$/i, '');
      const projDir = await api.projectImportZip(zip, destRoot);
      onOpenProject(projDir);
    } catch (e) {
      setErr('导入失败：' + String(e));
    } finally {
      setBusy('');
    }
  };

  const removeRecent = async (path: string): Promise<void> => {
    await api.recentRemove(path).catch(() => {});
    setRecents((l) => l.filter((r) => r.path !== path));
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      const tag = (e.target as HTMLElement)?.tagName;
      const mod = e.ctrlKey || e.metaKey;
      if (e.code === 'F1') { e.preventDefault(); setShortcuts(true); return; }
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (mod && e.code === 'KeyN') { e.preventDefault(); setNewTpl(TEMPLATES[1]); return; }
      if (mod && e.code === 'KeyO') { e.preventDefault(); if (e.shiftKey) void importZip(); else void openExisting(); return; }
      if (mod && e.altKey && e.code === 'KeyS') { e.preventDefault(); setSettings(true); return; }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [openExisting, importZip]);

  const menus: MenuGroup[] = [
    {
      label: '文件',
      items: [
        { label: '新建工程', hint: 'Ctrl+N', run: () => setNewTpl(TEMPLATES[1]) },
        { label: '打开工程', hint: 'Ctrl+O', run: () => void openExisting() },
        { label: '导入工程包', hint: 'Ctrl+Shift+O', run: () => void importZip() },
        { sep: true, label: 's1' },
        { label: '退出', hint: 'Alt+F4', run: () => { void import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().close()).catch(() => {}); } }
      ]
    },
    {
      label: '视图',
      items: [
        { label: '偏好设置…', hint: 'Ctrl+Alt+S', run: () => setSettings(true) },
        { label: '外观与背景…', run: () => setShowAppearance(true) }
      ]
    },
    {
      label: '帮助',
      items: [
        { label: '快捷键一览', hint: 'F1', run: () => setShortcuts(true) },
        { label: '学习中心（Float Script 教程）', hint: 'Ctrl+Shift+H', run: () => setLearn(true) },
        { label: '关于 Float IDE', run: () => setLearn(true) }
      ]
    }
  ];

  const cards = [
    { icon: <IconPlus size={18} />, title: '新建工程', desc: '从模板起步', run: () => setNewTpl(TEMPLATES[1]) },
    { icon: <IconFolder size={18} />, title: '打开工程', desc: '选择已有文件夹', run: () => void openExisting() },
    { icon: <IconImport size={18} />, title: '导入工程包', desc: '.float 一键搬入', run: () => void importZip() },
    { icon: <IconBook size={18} />, title: '学习中心', desc: '13 章 · 51 节 · 可运行示例', run: () => setLearn(true) },
    { icon: <IconImage size={18} />, title: '外观背景', desc: '换一张自己的图', run: () => setShowAppearance(true) },
    { icon: <IconSearch size={18} />, title: '最近工程', desc: `${recents.length} 个记录`, run: () => { if (recents[0]) onOpenProject(recents[0].path); else setNewTpl(TEMPLATES[0]); } }
  ];

  return (
    <div className="wc" style={{ position: 'relative' }}>
      <TitleBar
        title="Float IDE"
        subtitle="代码世界脚本工作室"
        right={<button className="tb-link" onClick={() => setShowAppearance(true)}><IconImage size={13} /> 外观</button>}
      />

      <MenuBar menus={menus} />
      <div className="wc-body">
        <aside className="wc-side">
          <div className="wc-side-head">项目</div>
          <div className="wc-acts">
            <button className="wc-act" onClick={() => setNewTpl(TEMPLATES[1])}><IconPlus size={15} /> 新建工程…</button>
            <button className="wc-act" onClick={() => void openExisting()}><IconFolder size={15} /> 打开…</button>
            <button className="wc-act" onClick={() => void importZip()}><IconImport size={15} /> 导入工程包…</button>
          </div>

          <div className="wc-search">
            <IconSearch size={13} />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="搜索项目" />
          </div>

          <div className="wc-recents">
            {filtered.length === 0 && (
              <div className="wc-empty">{q ? '没有匹配的项目' : '还没有打开过工程'}</div>
            )}
            {filtered.map((r) => (
              <div key={r.path} className="wc-recent" onClick={() => onOpenProject(r.path)} title={r.path}>
                <span className="wc-recent-icon"><IconFolder size={15} /></span>
                <span className="wc-recent-main">
                  <b>{r.name}</b>
                  <i>{r.path}</i>
                </span>
                <span
                  className="wc-recent-x"
                  title="从列表移除（不删文件）"
                  onClick={(e) => { e.stopPropagation(); void removeRecent(r.path); }}
                >
                  <IconClose size={11} />
                </span>
              </div>
            ))}
          </div>
        </aside>

        <main className="wc-main">
          <div className="wc-hero">
            <div className="wc-logo"><Logo size={84} /></div>
            <h1>欢迎使用 Float IDE</h1>
            <p className="wc-sub">给代码世界一个正经的 IDE</p>

            <div className="wc-cards">
              {cards.map((c) => (
                <button key={c.title} className="wc-card" onClick={c.run}>
                  <span className="wc-card-icon">{c.icon}</span>
                  <b>{c.title}</b>
                  <i>{c.desc}</i>
                </button>
              ))}
            </div>

            <div className="wc-tip">F5 运行 · F6 调试 · Ctrl+S 保存 · Ctrl+Shift+P 命令面板</div>
          </div>
          {busy && <div className="wc-busy">{busy}</div>}
          {err && (
            <div className="wc-busy err">
              {err}
              <button className="wc-err-x" onClick={() => setErr('')}>✕</button>
            </div>
          )}
        </main>
      </div>

      <div className="wc-foot">
        <span>v0.6.0 · 小方工作室 · MIT</span>
        <span className="grow" />
        <span className="muted">Float Script（CW Script 系）</span>
      </div>

      {newTpl && <NewProjectDialog preset={newTpl} onClose={() => setNewTpl(null)} onOpenProject={onOpenProject} />}
      {showAppearance && <AppearanceDialog onClose={() => setShowAppearance(false)} />}
      {learn && <LearnPage onClose={() => setLearn(false)} />}
      {shortcuts && <ShortcutsDialog onClose={() => setShortcuts(false)} />}
      {settings && <SettingsDialog onClose={() => setSettings(false)} onAppearance={() => setShowAppearance(true)} />}
    </div>
  );
}
