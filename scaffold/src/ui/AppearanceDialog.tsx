import React, { useEffect, useRef, useState } from 'react';
import {
  applyAppearance, DEFAULT_APPEARANCE, imageFileToDataUrl, isLegacyBg,
  loadAppearance, saveAppearance, toBgUrl, type Appearance
} from './appearance';

/* 外观设置：换背景图、调浓度与模糊。改动即时生效并记住。
 * 选图走 <input type="file"> + FileReader 转 data URL，不再依赖 Tauri 的
 * asset://（那个协议没启用，选完图会一片空白）。 */

export function AppearanceDialog({ onClose, embedded }: { onClose: () => void; embedded?: boolean }) {
  const [a, setA] = useState<Appearance>(() => loadAppearance());
  /* 必须在挂载时读：下面的 useEffect 会立刻把新值写回 localStorage。 */
  const [legacy] = useState<boolean>(() => {
    try {
      const raw = window.localStorage.getItem('float.appearance');
      return raw ? isLegacyBg((JSON.parse(raw) as { bg?: unknown }).bg) : false;
    } catch { return false; }
  });
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => { applyAppearance(a); saveAppearance(a); }, [a]);

  const onFile = async (f: File | undefined): Promise<void> => {
    if (!f) return;
    setBusy(true);
    setErr('');
    try {
      const data = await imageFileToDataUrl(f);
      setA((prev) => ({ ...prev, bg: data }));
    } catch (e) {
      setErr('选图失败：' + String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
      if (fileRef.current) fileRef.current.value = '';
    }
  };

  return (
    <div className={embedded ? 'doc-embed settings-host' : 'mask'} onMouseDown={embedded ? undefined : onClose}>
      <div className={embedded ? 'doc-embed-inner settings-shell embedded appearance-shell' : 'dialog settings settings-shell appearance-shell'} onMouseDown={(e) => e.stopPropagation()}>
        <div className="dlg-head">
          <span>外观 · 自定义背景</span>
          <button className="iconbtn" onClick={onClose} title="关闭">✕</button>
        </div>

        <div className="dlg-body">
          <div
            className="bg-preview"
            style={a.bg ? { backgroundImage: `url("${toBgUrl(a.bg)}")` } : undefined}
          >
            {!a.bg && <span>还没有背景图</span>}
            <div className="bg-preview-veil" style={{ opacity: a.dim }} />
          </div>

          <input ref={fileRef} type="file" accept="image/png,image/jpeg,image/webp,image/bmp,image/gif"
            style={{ display: 'none' }} onChange={(e) => void onFile(e.target.files?.[0])} />

          <div className="dlg-row">
            <button className="btn primary" disabled={busy} onClick={() => fileRef.current?.click()}>
              {busy ? '处理中…' : '选择图片…'}
            </button>
            {a.bg && <button className="btn" onClick={() => setA({ ...a, bg: null })}>移除</button>}
            {a.bg && <span className="dlg-hint" style={{ margin: 0 }}>已设置背景图 · 图片会随设置一起保存</span>}
          </div>

          <label className="dlg-slider">
            <span>图片浓度</span>
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

          <p className="dlg-hint">背景会铺在整个 IDE 后面，面板保持半透明，字仍然清楚。图片会自动压到 1920 长边再存，不占多少空间。</p>
          {legacy && <p className="dlg-hint">检测到旧版保存的是图片路径，那种路径在桌面版里加载不到，已经清掉了——重新选一次图片就好。</p>}
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
