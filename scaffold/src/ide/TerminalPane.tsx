import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { api, listen } from '../core/tauri';
import { IconClose, IconPlus } from '../ui/icons';

/* 真终端：前端 xterm.js ↔ 后端 ConPTY（powershell）。
 * 输出走 pty_write，输出经 pty-output 事件回流（base64 → 字节）。 */

type Tab = { id: string; title: string };

const termTheme = {
  background: '#081427',
  foreground: '#dbe7ff',
  cursor: '#7cb1ff',
  cursorAccent: '#081427',
  selectionBackground: '#2c4a86',
  black: '#0c1a35',
  red: '#ff7b7b',
  green: '#55d493',
  yellow: '#ffd166',
  blue: '#6ea8ff',
  magenta: '#d9a0f0',
  cyan: '#8fd3ff',
  white: '#dbe7ff',
  brightBlack: '#7285ad',
  brightWhite: '#ffffff'
};

function b64ToBytes(data: string): Uint8Array {
  const bin = atob(data);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

export function TerminalPane({ projectDir }: { projectDir: string }) {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const termsRef = useRef<Map<string, { term: Terminal; fit: FitAddon; host: HTMLElement }>>(new Map());
  const activeRef = useRef<string | null>(null);
  activeRef.current = activeId;
  const seqRef = useRef(0);

  const fitActive = (): void => {
    const a = activeRef.current;
    if (!a) return;
    const t = termsRef.current.get(a);
    if (!t) return;
    try { t.fit.fit(); } catch { /* 容器尚未布局 */ }
    void api.ptyResize(a, t.term.cols, t.term.rows).catch(() => {});
  };

  /* 全局输出路由 */
  useEffect(() => {
    const un = listen<{ id: string; data: string }>('pty-output', (e) => {
      const p = e.payload;
      const t = termsRef.current.get(p.id);
      if (t) t.term.write(b64ToBytes(p.data));
    });
    const un2 = listen<{ id: string }>('pty-exit', (e) => {
      const t = termsRef.current.get(e.payload.id);
      if (t) {
        t.term.write('\r\n\x1b[90m[进程已退出]\x1b[0m\r\n');
      }
    });
    return () => { un.then((f) => f()); un2.then((f) => f()); };
  }, []);

  const createTerm = useCallback(async (): Promise<void> => {
    if (!hostRef.current) return;
    seqRef.current += 1;
    const id = 't' + seqRef.current;
    const host = document.createElement('div');
    host.style.cssText = 'position:absolute;inset:0;padding:4px 6px;display:none;';
    hostRef.current.appendChild(host);

    const term = new Terminal({
      fontFamily: 'Consolas, "Courier New", monospace',
      fontSize: 13,
      theme: termTheme,
      cursorBlink: true,
      allowProposedApi: true
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    term.onData((d) => void api.ptyWrite(id, d).catch(() => {}));
    termsRef.current.set(id, { term, fit, host });
    setTabs((t) => [...t, { id, title: `终端 ${seqRef.current}` }]);
    setActiveId(id);
    activeRef.current = id;

    try {
      term.element && (term.element.style.height = '100%');
      fit.fit();
      await api.ptyCreate(id, projectDir, term.rows, term.cols);
    } catch (e) {
      term.write('\x1b[31m终端启动失败：' + String(e) + '\x1b[0m\r\n');
    }
  }, [projectDir]);

  /* 首次挂载开一个终端 */
  useEffect(() => { void createTerm(); /* eslint-disable-next-line */ }, []);

  /* 切换显示 */
  useEffect(() => {
    for (const [id, t] of termsRef.current) {
      t.host.style.display = id === activeId ? 'block' : 'none';
    }
    fitActive();
  }, [activeId]);

  /* 尺寸变化 */
  useEffect(() => {
    const ro = new ResizeObserver(() => fitActive());
    if (hostRef.current) ro.observe(hostRef.current);
    return () => ro.disconnect();
  }, []);

  const closeTab = (id: string): void => {
    void api.ptyClose(id).catch(() => {});
    const t = termsRef.current.get(id);
    if (t) { t.term.dispose(); t.host.remove(); termsRef.current.delete(id); }
    setTabs((list) => {
      const next = list.filter((x) => x.id !== id);
      if (activeRef.current === id) {
        const n = next[next.length - 1]?.id ?? null;
        activeRef.current = n;
        setTimeout(() => setActiveId(n), 0);
      }
      return next;
    });
  };

  /* 卸载时关掉全部终端进程 */
  useEffect(() => {
    return () => {
      for (const [id] of termsRef.current) void api.ptyClose(id).catch(() => {});
      termsRef.current.clear();
    };
  }, []);

  return (
    <div className="term-dock">
      <div className="term-tabs">
        <span className="term-label">终端</span>
        {tabs.map((t) => (
          <div key={t.id} className={`term-tab ${activeId === t.id ? 'active' : ''}`} onClick={() => setActiveId(t.id)}>
            {t.title}
            <span className="term-close" onClick={(e) => { e.stopPropagation(); closeTab(t.id); }}><IconClose size={10} /></span>
          </div>
        ))}
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="新建终端" onClick={() => void createTerm()}>
          <IconPlus size={12} />
        </button>
        <span style={{ flex: 1 }} />
        <span className="muted" style={{ fontSize: 11, marginRight: 8 }}>Ctrl+` 收起</span>
      </div>
      <div className="term-host" ref={hostRef} />
    </div>
  );
}
