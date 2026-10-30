import React, { useEffect, useState } from 'react';

/* 无边框窗口的 8 向缩放热区：Tauri 的 startResizeDragging 负责真正的拖拽缩放。 */

const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as unknown as Record<string, unknown>);

type Dir = 'North' | 'South' | 'East' | 'West' | 'NorthEast' | 'NorthWest' | 'SouthEast' | 'SouthWest';

export function WindowResizers() {
  const [tauri, setTauri] = useState(false);
  useEffect(() => setTauri(isTauri()), []);
  if (!tauri) return null;

  const start = async (dir: Dir): Promise<void> => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().startResizeDragging(dir as never);
    } catch { /* 忽略 */ }
  };

  const edge = (cls: string, dir: Dir) => (
    <div className={`wres ${cls}`} onMouseDown={() => void start(dir)} />
  );

  return (
    <div className="wres-layer" aria-hidden>
      {edge('n', 'North')}
      {edge('s', 'South')}
      {edge('e', 'East')}
      {edge('w', 'West')}
      {edge('ne', 'NorthEast')}
      {edge('nw', 'NorthWest')}
      {edge('se', 'SouthEast')}
      {edge('sw', 'SouthWest')}
    </div>
  );
}
