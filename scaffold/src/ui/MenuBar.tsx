import React, { useEffect, useRef, useState } from 'react';

/* 顶部菜单栏（PyCharm 式）：点开下拉，鼠标划过切换，点外面收起。 */

export type MenuItem = {
  label: string;
  hint?: string;
  run?: () => void;
  sep?: boolean;
  disabled?: boolean;
};

export type MenuGroup = {
  label: string;
  items: MenuItem[];
};

export function MenuBar({ menus, right }: { menus: MenuGroup[]; right?: React.ReactNode }) {
  const [open, setOpen] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent): void => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(null);
    };
    const onEsc = (e: KeyboardEvent): void => { if (e.key === 'Escape') setOpen(null); };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onEsc);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onEsc);
    };
  }, [open]);

  return (
    <div className="menubar" ref={rootRef}>
      {menus.map((g) => (
        <div className="menu" key={g.label}>
          <button
            className={`menu-btn ${open === g.label ? 'on' : ''}`}
            onClick={() => setOpen((o) => (o === g.label ? null : g.label))}
            onMouseEnter={() => { if (open) setOpen(g.label); }}
          >
            {g.label}
          </button>
          {open === g.label && (
            <div className="menu-pop">
              {g.items.map((it, i) => it.sep
                ? <div className="menu-sep" key={`s${i}`} />
                : (
                  <button
                    key={it.label}
                    className="menu-item"
                    disabled={it.disabled}
                    onClick={() => { setOpen(null); it.run?.(); }}
                  >
                    <span>{it.label}</span>
                    {it.hint && <em>{it.hint}</em>}
                  </button>
                ))}
            </div>
          )}
        </div>
      ))}
      <span className="menu-grow" />
      {right}
    </div>
  );
}
