import React, { useEffect, useState } from 'react';
import { api } from '../core/tauri';
import { applyAppearance, DEFAULT_APPEARANCE, loadAppearance, saveAppearance, type Appearance } from './appearance';

/* 外观设置：换背景图、调透明度与模糊。改动即时生效并记住。 */

export function AppearanceDialog({ onClose, embedded }: { onClose: () => void; embedded?: boolean }) {
  const [a, setA] = useState<Appearance>(() => loadAppearance());
  const [err, setErr] = useState('');

  useEffect(() => { applyAppearance(a); saveAppearance(a); }, [a]);

  const pick = async (): Promise<void> => {
    try {
      const p = await api.pickImage('选择背景图片');
      if (p) setA((prev) => ({ ...prev, bg: p }));
    } catch (e) {
      setErr('选图失败：' + String(e));
    }
  };

  return (
    <div className={embedded ? 'doc-embed' : 'mask'} onMouseDown={embedded ? undefined : onClose}>
      <div className={embedded ? 'doc-embed-inner' : 'dialog appear'} onMouseDown={(e) => e.stopPropagation()}>
        <div className="dlg-head">
          <span>外观 · 自定义背景</span>
          <button className="iconbtn" onClick={onClose} title="关闭">✕</button>
        </div>

        <div className="dlg-body">
          <div
            className="bg-preview"
            style={a.bg ? { backgroundImage: `url("${a.bg.replace(/\\/g, '/')}")` } : undefined}
          >
            {!a.bg && <span>还没有背景图</span>}
            <div className="bg-preview-veil" style={{ opacity: a.dim }} />
          </div>

          <div className="dlg-row">
            <button className="btn primary" onClick={() => void pick()}>选择图片…</button>
            {a.bg && <button className="btn" onClick={() => setA({ ...a, bg: null })}>移除</button>}
            {a.bg && <span className="dlg-path" title={a.bg}>{a.bg}</span>}
          </div>

          <label className="dlg-slider">
            <span>透明度</span>
            <input type="range" min={10} max={100} value={Math.round(a.opacity * 100)}
              onChange={(e) => setA({ ...a, opacity: Number(e.target.value) / 100 })} />
            <b>{Math.round(a.opacity * 100)}%</b>
          </label>

          <label className="dlg-slider">
            <span>模糊</span>
            <input type="range" min={0} max={24} value={a.blur}
              onChange={(e) => setA({ ...a, blur: Number(e.target.value) })} />
            <b>{a.blur}px</b>
          </label>

          <label className="dlg-slider">
            <span>压暗</span>
            <input type="range" min={0} max={90} value={Math.round(a.dim * 100)}
              onChange={(e) => setA({ ...a, dim: Number(e.target.value) / 100 })} />
            <b>{Math.round(a.dim * 100)}%</b>
          </label>

          <p className="dlg-hint">背景会铺在整个 IDE 后面，面板保持半透明，字仍然清楚。建议用 1920×1080 以上的图。</p>
          {err && <div className="dlg-err">{err}</div>}
        </div>

        <div className="dlg-foot">
          <button className="btn" onClick={() => setA({ ...DEFAULT_APPEARANCE })}>恢复默认</button>
          <button className="btn primary" onClick={onClose}>完成</button>
        </div>
      </div>
    </div>
  );
}
