import React, { useEffect, useState } from 'react';
import { Loading } from './ui/Loading';
import { Welcome } from './ui/Welcome';
import { Workspace } from './workspace/Workspace';
import { applyAppearance, loadAppearance } from './ui/appearance';
import { applySettings, loadSettings, watchSystemTheme } from './ui/settings';
import { WindowResizers } from './ui/WindowResizers';
import { PromptHost } from './ui/prompt';

type Page = { kind: 'loading' } | { kind: 'welcome' } | { kind: 'ws'; dir: string };

/* 应用外壳：背景图铺在最底层，面板半透明浮在上面。 */
export default function App() {
  const [page, setPage] = useState<Page>({ kind: 'loading' });

  useEffect(() => {
    applyAppearance(loadAppearance());
    applySettings(loadSettings());
    return watchSystemTheme(() => loadSettings().theme);
  }, []);

  return (
    <div className="app-shell">
      <div className="app-bg" />
      <div className="app-veil" />
      <WindowResizers />
      <div className="app-stage">
        {page.kind === 'loading' && <Loading onDone={() => setPage({ kind: 'welcome' })} />}
        {page.kind === 'welcome' && <Welcome onOpenProject={(dir) => setPage({ kind: 'ws', dir })} />}
        {page.kind === 'ws' && <Workspace projectDir={page.dir} onExit={() => setPage({ kind: 'welcome' })} />}
      </div>
      {/* 输入/确认对话框：替代 WebView 里不可用的 prompt / confirm */}
      <PromptHost />
    </div>
  );
}
