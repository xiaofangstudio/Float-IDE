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
 *  .cw 就当文件，其余当文件夹——双击 .cw 打开走的就是这条。 */
export function resolveTarget(target: string): { dir: string; openFile?: string } {
  const norm = target.replace(/\\/g, '/').replace(/\/+$/, '');
  if (/\.cw$/i.test(norm)) {
    const cut = norm.lastIndexOf('/');
    return { dir: cut > 0 ? norm.slice(0, cut) : norm, openFile: norm };
  }
  return { dir: norm };
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
    void (async () => {
      try {
        const p = await api.startupPath();
        if (alive && p) setStartup(p);
      } catch { /* 预览环境忽略 */ }
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
        {page.kind === 'loading' && (startupChecked || !isTauri()) && <Loading onDone={onLoadingDone} />}
        {page.kind === 'welcome' && (
          <Welcome
            onOpenProject={(dir) => setPage({ kind: 'ws', dir })}
            onOpenFile={(file) => openTarget(file)}
          />
        )}
        {page.kind === 'ws' && (
          <Workspace
            projectDir={page.dir}
            initialFile={page.openFile}
            onOpenTarget={openTarget}
            onExit={() => setPage({ kind: 'welcome' })}
          />
        )}
      </div>
      {/* 输入/确认对话框：替代 WebView 里不可用的 prompt / confirm */}
      <PromptHost />
    </div>
  );
}
