import React, { useRef } from 'react';

/* 可拖动分栏：拿到拖动增量后由调用方改尺寸，指针离开也不丢拖拽。 */

type Props = {
  dir: 'x' | 'y';
  onDrag: (delta: number) => void;
  title?: string;
};

export function Splitter({ dir, onDrag, title }: Props) {
  const elRef = useRef<HTMLDivElement>(null);
  const handler = useRef(onDrag);
  handler.current = onDrag;

  const onDown = (e: React.MouseEvent): void => {
    e.preventDefault();
    e.stopPropagation();
    let last = dir === 'x' ? e.clientX : e.clientY;
    const el = elRef.current;
    el?.classList.add('dragging');
    document.body.classList.add('resizing');
    document.body.style.cursor = dir === 'x' ? 'col-resize' : 'row-resize';

    const move = (ev: MouseEvent): void => {
      const pos = dir === 'x' ? ev.clientX : ev.clientY;
      const delta = pos - last;
      last = pos;
      if (delta !== 0) handler.current(delta);
    };
    const up = (): void => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      el?.classList.remove('dragging');
      document.body.classList.remove('resizing');
      document.body.style.cursor = '';
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  };

  return <div ref={elRef} className={`split split-${dir}`} onMouseDown={onDown} title={title} />;
}
