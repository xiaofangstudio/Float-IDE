import React, { useCallback, useEffect, useState } from 'react';
import { Loading } from './ui/Loading';
import { Welcome } from './ui/Welcome';
import { Workspace } from './workspace/Workspace';
import { applyAppearance, loadAppearance } from './ui/appearance';
import { applySettings, loadSettings, watchSystemTheme } from './ui/settings';
import { WindowResizers } from './ui/WindowResizers';
import { PromptHost } from './ui/prompt';
import { api } from './core/tauri';

type Page = { kind: 'loading' } | { kind: 'welcome' } | { kind: 'ws'; dir: string; openFile?: string };

const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as unknown as Record<string, unknown>);

/** 把一个路径拆成"工程目录 + 要打开的文件"。
 *  .co / .cw / .json / .txt / .md / .in 就当文件（.cw 是旧扩展名，保留兼容），其余当文件夹。 */
export function resolveTarget(target: string): { dir: string; openFile?: string } {
  const norm = target.replace(/\\/g, '/').replace(/\/+$/, '');
  if (/\.(co|cw|json|txt|md|in)$/i.test(norm)) {
    const cut = norm.lastIndexOf('/');
    return { dir: cut > 0 ? norm.slice(0, cut) : norm, openFile: norm };
  }
  return { dir: norm };
}


/** 工程页崩溃兜底：以前 Workspace 一炸整个页面白掉/弹回欢迎页，谁也不知道为什么。
 *  现在把错误亮出来，给"回到欢迎页"的出口——报错能截图，问题就能修。 */
class WorkspaceBoundary extends React.Component<
  { resetKey: string; onExit: () => void; children: React.ReactNode },
  { err: Error | null }
> {
  state = { err: null as Error | null };
  static getDerivedStateFromError(err: Error): { err: Error } {
    return { err };
  }
  componentDidUpdate(prev: { resetKey: string }): void {
    // 换工程 = 全新一次，旧错误清掉
    if (prev.resetKey !== this.props.resetKey && this.state.err) this.setState({ err: null });
  }
  render(): React.ReactNode {
    if (this.state.err) {
      return (
        <div style={{ padding: 32, maxWidth: 760, margin: '0 auto', fontFamily: 'monospace' }}>
          <h2 style={{ marginBottom: 12 }}>工程页出错了（这不是脚本的锅，是 IDE 内部的问题）</h2>
          <pre style={{
            whiteSpace: 'pre-wrap', background: 'var(--bg2, #f5f5f5)', color: 'var(--fg, #222)',
            padding: 16, borderRadius: 6, fontSize: 12, lineHeight: 1.6, userSelect: 'text'
          }}>{String(this.state.err.stack ?? this.state.err.message ?? this.state.err)}</pre>
          <button
            className="btn primary"
            style={{ marginTop: 16 }}
            onClick={() => this.props.onExit()}
          >回到欢迎页</button>
        </div>
      );
    }
    return this.props.children;
  }
}

/* 应用外壳：背景图铺在最底层，面板半透明浮在上面。 */
export default function App() {
  const [page, setPage] = useState<Page>({ kind: 'loading' });
  const [startup, setStartup] = useState<string | null>(null);
  const [startupChecked, setStartupChecked] = useState(false);

  useEffect(() => {
    applyAppearance(loadAppearance());
    applySettings(loadSettings());
    return watchSystemTheme(() => loadSettings().theme);
  }, []);

  /* 双击 .cw 启动时，Windows 会把路径作为参数传进来。
   * 以前这里完全没读，所以双击脚本只会掉到欢迎页。 */
  useEffect(() => {
    let alive = true;
    if (!isTauri()) {
      setStartupChecked(true);
      return;
    }
    // 兜底：不管查询成功、失败、还是卡住，最多 800ms 就放行 ——
    // 界面绝不允许因为一个启动参数而停在空白页
    const guard = window.setTimeout(() => { if (alive) setStartupChecked(true); }, 800);
    void (async () => {
      try {
        const p = await api.startupPath();
        if (alive && p) setStartup(p);
      } catch { /* 预览环境忽略 */ }
      window.clearTimeout(guard);
      if (alive) setStartupChecked(true);
    })();
    return () => { alive = false; };
  }, []);

  const openTarget = useCallback((target: string) => {
    const { dir, openFile } = resolveTarget(target);
    setPage({ kind: 'ws', dir, openFile });
  }, []);

  const onLoadingDone = useCallback(() => {
    if (startup) openTarget(startup);
    else setPage({ kind: 'welcome' });
  }, [startup, openTarget]);

  return (
    <div className="app-shell">
      <div className="app-bg" />
      <div className="app-veil" />
      <WindowResizers />
      <div className="app-stage">
        {page.kind === 'loading' && <Loading onDone={onLoadingDone} />}
        {page.kind === 'welcome' && (
          <Welcome
            onOpenProject={(dir) => setPage({ kind: 'ws', dir })}
            onOpenFile={(file) => openTarget(file)}
          />
        )}
        {page.kind === 'ws' && (
          <WorkspaceBoundary
            resetKey={page.dir + '|' + (page.openFile ?? '')}
            onExit={() => setPage({ kind: 'welcome' })}
          >
            <Workspace
              projectDir={page.dir}
              initialFile={page.openFile}
              onOpenTarget={openTarget}
              onExit={() => setPage({ kind: 'welcome' })}
            />
          </WorkspaceBoundary>
        )}
      </div>
      {/* 输入/确认对话框：替代 WebView 里不可用的 prompt / confirm */}
      <PromptHost />
    </div>
  );
}
