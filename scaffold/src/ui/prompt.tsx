import React, { useCallback, useEffect, useRef, useState } from 'react';

/* 自绘的输入 / 确认对话框。
 *
 * 为什么不用 window.prompt / confirm：Tauri v2 的 WebView2 里这两个是**未实现**的，
 * 调用会静默返回 null / false —— 表现就是「点了新建什么都没发生」。
 * 所以所有需要用户输入的地方一律走这里。
 */

type TextReq = {
  kind: 'text';
  title: string;
  value: string;
  placeholder?: string;
  hint?: string;
  /** 校验：返回字符串表示错了，返回 null 表示通过 */
  validate?: (v: string) => string | null;
  resolve: (v: string | null) => void;
};

type ConfirmReq = {
  kind: 'confirm';
  title: string;
  hint?: string;
  okText: string;
  danger: boolean;
  resolve: (v: boolean) => void;
};

type Req = TextReq | ConfirmReq;

let current: Req | null = null;
const listeners = new Set<(r: Req | null) => void>();

function emit(): void {
  for (const l of listeners) l(current);
}

export function askText(
  title: string,
  opts: { value?: string; placeholder?: string; hint?: string; validate?: (v: string) => string | null } = {}
): Promise<string | null> {
  return new Promise((resolve) => {
    current = {
      kind: 'text',
      title,
      value: opts.value ?? '',
      placeholder: opts.placeholder,
      hint: opts.hint,
      validate: opts.validate,
      resolve: (v) => { current = null; emit(); resolve(v); }
    };
    emit();
  });
}

export function askConfirm(
  title: string,
  opts: { hint?: string; okText?: string; danger?: boolean } = {}
): Promise<boolean> {
  return new Promise((resolve) => {
    current = {
      kind: 'confirm',
      title,
      hint: opts.hint,
      okText: opts.okText ?? '确定',
      danger: opts.danger ?? false,
      resolve: (v) => { current = null; emit(); resolve(v); }
    };
    emit();
  });
}

/** 挂在 App 根部一次即可 */
export function PromptHost(): React.ReactElement | null {
  const [req, setReq] = useState<Req | null>(current);
  const [text, setText] = useState('');
  const [err, setErr] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const on = (r: Req | null): void => {
      setReq(r);
      setErr('');
      if (r && r.kind === 'text') setText(r.value);
    };
    listeners.add(on);
    return () => { listeners.delete(on); };
  }, []);

  useEffect(() => {
    if (req) window.setTimeout(() => inputRef.current?.select(), 0);
  }, [req]);

  const ok = useCallback((): void => {
    if (!req) return;
    if (req.kind === 'confirm') { req.resolve(true); return; }
    const v = text.trim();
    const bad = req.validate?.(v) ?? (v ? null : '不能为空');
    if (bad) { setErr(bad); return; }
    req.resolve(v);
  }, [req, text]);

  const cancel = useCallback((): void => {
    if (!req) return;
    if (req.kind === 'confirm') req.resolve(false);
    else req.resolve(null);
  }, [req]);

  if (!req) return null;

  return (
    <div className="modal-mask" onMouseDown={cancel}>
      <div className="prompt-box" onMouseDown={(e) => e.stopPropagation()}>
        <div className="prompt-title">{req.title}</div>
        {req.kind === 'text' ? (
          <>
            <input
              ref={inputRef}
              autoFocus
              className="prompt-input"
              value={text}
              placeholder={req.placeholder}
              onChange={(e) => { setText(e.target.value); setErr(''); }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') { e.preventDefault(); ok(); }
                if (e.key === 'Escape') cancel();
              }}
            />
            {(err || req.hint) && <div className={`prompt-hint ${err ? 'err' : ''}`}>{err || req.hint}</div>}
          </>
        ) : (
          req.hint && <div className="prompt-hint">{req.hint}</div>
        )}
        <div className="prompt-actions">
          <button className="btn" onClick={cancel}>取消</button>
          <button className={`btn ${req.kind === 'confirm' && req.danger ? 'danger' : 'primary'}`} onClick={ok}>
            {req.kind === 'confirm' ? req.okText : '确定'}
          </button>
        </div>
      </div>
    </div>
  );
}
