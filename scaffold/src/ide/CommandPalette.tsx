import React, { useEffect, useMemo, useRef, useState } from 'react';

/* 命令面板（Ctrl+Shift+P）：全部工作台动作的统一入口。 */

export type Command = {
  id: string;
  title: string;
  hint?: string;
  run: () => void;
};

export function CommandPalette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [q, setQ] = useState('');
  const [idx, setIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return commands;
    return commands.filter((c) => (c.title + (c.hint ?? '')).toLowerCase().includes(s));
  }, [q, commands]);

  useEffect(() => { setIdx(0); }, [q]);
  useEffect(() => { inputRef.current?.focus(); }, []);

  useEffect(() => {
    const el = listRef.current?.children[idx] as HTMLElement | undefined;
    el?.scrollIntoView({ block: 'nearest' });
  }, [idx]);

  const exec = (c: Command): void => { onClose(); window.setTimeout(() => c.run(), 0); };

  return (
    <div className="modal-mask palette-mask" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="palette-input"
          placeholder="输入命令名或关键字……"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') onClose();
            else if (e.key === 'ArrowDown') { e.preventDefault(); setIdx((i) => Math.min(i + 1, filtered.length - 1)); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); setIdx((i) => Math.max(i - 1, 0)); }
            else if (e.key === 'Enter' && filtered[idx]) exec(filtered[idx]);
          }}
        />
        <div className="palette-list" ref={listRef}>
          {filtered.length === 0 && <div className="palette-item muted">没有匹配的命令</div>}
          {filtered.map((c, i) => (
            <div
              key={c.id}
              className={`palette-item ${i === idx ? 'active' : ''}`}
              onClick={() => exec(c)}
              onMouseEnter={() => setIdx(i)}
            >
              <span>{c.title}</span>
              {c.hint && <span className="palette-hint">{c.hint}</span>}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
