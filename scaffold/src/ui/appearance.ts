/* 外观设置：自定义背景图 + 透明度 / 模糊，存在 localStorage。 */

import { convertFileSrc } from '@tauri-apps/api/core';

export type Appearance = {
  bg: string | null;
  opacity: number;
  blur: number;
  dim: number;
};

const KEY = 'float.appearance';

export const DEFAULT_APPEARANCE: Appearance = { bg: null, opacity: 0.55, blur: 0, dim: 0.55 };

export function loadAppearance(): Appearance {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_APPEARANCE };
    const a = JSON.parse(raw) as Partial<Appearance>;
    return {
      bg: typeof a.bg === 'string' ? a.bg : null,
      opacity: typeof a.opacity === 'number' ? a.opacity : DEFAULT_APPEARANCE.opacity,
      blur: typeof a.blur === 'number' ? a.blur : DEFAULT_APPEARANCE.blur,
      dim: typeof a.dim === 'number' ? a.dim : DEFAULT_APPEARANCE.dim
    };
  } catch {
    return { ...DEFAULT_APPEARANCE };
  }
}

export function saveAppearance(a: Appearance): void {
  try { window.localStorage.setItem(KEY, JSON.stringify(a)); } catch { /* 忽略 */ }
}

export function applyAppearance(a: Appearance): void {
  const root = document.documentElement;
  if (a.bg) {
    let url = a.bg;
    try { url = convertFileSrc(a.bg); } catch { /* 浏览器预览 */ }
    root.style.setProperty('--user-bg', `url("${url.replace(/"/g, '%22').replace(/\\/g, '/')}")`);
    root.style.setProperty('--user-bg-opacity', String(a.opacity));
    root.style.setProperty('--user-bg-blur', `${a.blur}px`);
    root.style.setProperty('--user-bg-dim', String(a.dim));
    root.classList.add('has-user-bg');
  } else {
    root.style.setProperty('--user-bg', 'none');
    root.style.removeProperty('--user-bg-opacity');
    root.style.removeProperty('--user-bg-blur');
    root.style.removeProperty('--user-bg-dim');
    root.classList.remove('has-user-bg');
  }
}
