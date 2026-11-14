/* 外观设置：自定义背景图 + 浓度 / 模糊 / 压暗，存在 localStorage。
 *
 * 背景图存的是 data URL，不是磁盘路径。原因：Tauri 没开 assetProtocol，
 * convertFileSrc 出来的 asset:// 在 WebView2 里加载不到，而裸 C:\ 路径
 * 更不是合法 URL —— 以前选完图「看着没生效」就是栽在这里。
 * 存成自包含 data URL 后，真 IDE 和浏览器预览走同一条路，图片被挪走也不怕。 */

import { convertFileSrc } from '@tauri-apps/api/core';

export type Appearance = {
  /** 图片 data URL；老版本存过的磁盘路径仍兼容（见 toBgUrl） */
  bg: string | null;
  /** 图片浓度（CSS opacity），1 = 完全不透明 */
  opacity: number;
  blur: number;
  /** 白纱浓度，越大背景越淡 */
  dim: number;
};

const KEY = 'float.appearance';

/* 默认要「一眼看得见」：图片满浓度、白纱只压 35%。
   以前图片 55% + 白纱 55%，两层叠完只剩两成多，用户以为功能坏了。 */
export const DEFAULT_APPEARANCE: Appearance = { bg: null, opacity: 1, blur: 0, dim: 0.35 };

/** 老版本把图片存成 C:\... 磁盘路径，在 WebView2 里根本加载不了。
 *  读出来直接丢弃，并把浓度/压暗恢复到默认，免得"显示已设置但一片白"。 */
export function isLegacyBg(bg: unknown): boolean {
  return typeof bg === 'string' && bg.length > 0 && !/^(data:|blob:|https?:\/\/|asset:)/i.test(bg);
}

export function loadAppearance(): Appearance {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_APPEARANCE };
    const a = JSON.parse(raw) as Partial<Appearance>;
    const clamp = (v: unknown, lo: number, hi: number, d: number): number =>
      typeof v === 'number' && Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : d;
    const legacy = isLegacyBg(a.bg);
    return {
      bg: legacy ? null : (typeof a.bg === 'string' && a.bg ? a.bg : null),
      opacity: legacy ? DEFAULT_APPEARANCE.opacity : clamp(a.opacity, 0.1, 1, DEFAULT_APPEARANCE.opacity),
      blur: clamp(a.blur, 0, 24, DEFAULT_APPEARANCE.blur),
      dim: legacy ? DEFAULT_APPEARANCE.dim : clamp(a.dim, 0, 0.9, DEFAULT_APPEARANCE.dim)
    };
  } catch {
    return { ...DEFAULT_APPEARANCE };
  }
}

export function saveAppearance(a: Appearance): void {
  try { window.localStorage.setItem(KEY, JSON.stringify(a)); } catch { /* 忽略 */ }
}

/** 把存下来的 bg 变成 CSS 能直接用的 URL。data / blob / http 原样用；
 *  老版本留下的 C:\... 路径仍试着走 convertFileSrc，加载不了也不会抛。 */
export function toBgUrl(src: string): string {
  if (/^(data:|blob:|https?:\/\/|asset:)/i.test(src)) return src;
  try { return convertFileSrc(src); } catch { return src; }
}

/** 选中的图片 → 压缩后的 data URL：最长边收到 1920，JPEG 质量 0.86，
 *  超大就再降一档。避免几 MB 的 base64 把 localStorage 撑爆。 */
export async function imageFileToDataUrl(file: File): Promise<string> {
  const objectUrl = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const el = new Image();
      el.onload = () => resolve(el);
      el.onerror = () => reject(new Error('这个图片读不出来，换一张试试'));
      el.src = objectUrl;
    });
    const max = 1920;
    const scale = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
    const w = Math.max(1, Math.round(img.naturalWidth * scale));
    const h = Math.max(1, Math.round(img.naturalHeight * scale));
    const canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('浏览器不支持图片处理');
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, w, h);
    ctx.drawImage(img, 0, 0, w, h);
    let out = canvas.toDataURL('image/jpeg', 0.86);
    if (out.length > 3_000_000) out = canvas.toDataURL('image/jpeg', 0.7);
    return out;
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}

export function applyAppearance(a: Appearance): void {
  const root = document.documentElement;
  if (a.bg) {
    const url = toBgUrl(a.bg);
    root.style.setProperty('--user-bg', `url("${url.replace(/"/g, '%22').replace(/\\/g, '/')}")`);
    root.style.setProperty('--user-bg-opacity', String(a.opacity));
    root.style.setProperty('--user-bg-blur', `${a.blur}px`);
    root.style.setProperty('--user-bg-dim', String(a.dim));
    /* 磨砂程度：背景图模糊 + 面板毛玻璃共用这一个值（styles.css 的 .has-user-bg 块） */
    root.style.setProperty('--frost', `${a.blur}px`);
    root.style.setProperty('--panel-op', String(Math.round((1 - a.dim * 0.6) * 100)));
    root.classList.add('has-user-bg');
  } else {
    root.style.setProperty('--user-bg', 'none');
    root.style.removeProperty('--user-bg-opacity');
    root.style.removeProperty('--user-bg-blur');
    root.style.removeProperty('--user-bg-dim');
    root.style.removeProperty('--frost');
    root.style.removeProperty('--panel-op');
    root.classList.remove('has-user-bg');
  }
}
