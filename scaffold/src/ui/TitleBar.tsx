import React, { useEffect, useState } from 'react';

/* 自绘标题栏（窗口去掉了系统边框）：拖拽区 + 窗口按钮。
 * 注意：Tauri 的 window 模块必须在真正调用时再加载，
 * 顶层静态引入会在浏览器预览环境里直接抛错（__TAURI_INTERNALS__ 还没注入）。 */

type Props = {
  title: string;
  subtitle?: string;
  center?: React.ReactNode;
  right?: React.ReactNode;
};

type Win = Awaited<ReturnType<typeof loadWindow>>;

const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as unknown as Record<string, unknown>);

async function loadWindow(): Promise<{
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  isMaximized: () => Promise<boolean>;
  close: () => Promise<void>;
  onResized: (cb: () => void) => Promise<() => void>;
}> {
  const mod = await import('@tauri-apps/api/window');
  return mod.getCurrentWindow() as unknown as Win;
}

async function withWindow(fn: (w: Win) => Promise<void>): Promise<void> {
  if (!isTauri()) return;
  try {
    await fn(await loadWindow());
  } catch (e) {
    console.warn('[Float] 窗口操作失败：', e);
  }
}

export function TitleBar({ title, subtitle, center, right }: Props) {
  const [maxed, setMaxed] = useState(false);
  const tauri = isTauri();

  useEffect(() => {
    if (!tauri) return;
    let un: (() => void) | null = null;
    let alive = true;
    void (async () => {
      try {
        const w = await loadWindow();
        if (!alive) return;
        void w.isMaximized().then(setMaxed).catch(() => {});
        const fn = await w.onResized(() => { void w.isMaximized().then(setMaxed).catch(() => {}); });
        un = fn;
      } catch { /* 预览环境 */ }
    })();
    return () => { alive = false; un?.(); };
  }, [tauri]);

  const call = (fn: 'min' | 'max' | 'close'): void => {
    void withWindow(async (w) => {
      if (fn === 'min') await w.minimize();
      else if (fn === 'max') { await w.toggleMaximize(); setMaxed(await w.isMaximized()); }
      else await w.close();
    });
  };

  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="tb-left" data-tauri-drag-region>
        <span className="tb-title" data-tauri-drag-region>{title}</span>
        {subtitle && (<><span className="tb-sep" data-tauri-drag-region>/</span><span className="tb-sub" data-tauri-drag-region>{subtitle}</span></>)}
      </div>

      <div className="tb-center">{center}</div>

      <div className="tb-right" onMouseDown={(e) => e.stopPropagation()}>
        {right}
        {tauri && (
          <div className="tb-win">
            <button className="tb-winbtn" title="最小化" onClick={() => call('min')}>
              <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2 5h6" stroke="currentColor" strokeWidth="1.1" /></svg>
            </button>
            <button className="tb-winbtn" title={maxed ? '还原' : '最大化'} onClick={() => call('max')}>
              {maxed
                ? <svg width="10" height="10" viewBox="0 0 10 10"><rect x="2.2" y="3.2" width="4.4" height="4.4" fill="none" stroke="currentColor" strokeWidth="1" /><path d="M3.6 3.2V2.2h4.2v4.2H6.8" fill="none" stroke="currentColor" strokeWidth="1" /></svg>
                : <svg width="10" height="10" viewBox="0 0 10 10"><rect x="2.2" y="2.2" width="5.6" height="5.6" fill="none" stroke="currentColor" strokeWidth="1" /></svg>}
            </button>
            <button className="tb-winbtn close" title="关闭" onClick={() => call('close')}>
              <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2.4 2.4l5.2 5.2M7.6 2.4L2.4 7.6" stroke="currentColor" strokeWidth="1.1" /></svg>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
