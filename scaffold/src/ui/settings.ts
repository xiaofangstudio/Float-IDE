/* 偏好设置：主题（浅色 / 深色 / 跟随系统）、字号、缩进、换行、自动保存、默认运行方式。
 * 存在 localStorage，改动即时生效，并通过事件通知编辑器。 */

export type ThemeMode = 'light' | 'dark' | 'system';

export type Settings = {
  theme: ThemeMode;
  fontSize: number;
  tabSize: number;
  wordWrap: boolean;
  autoSave: boolean;
  runMode: 'compile' | 'debug';
  /** 紧凑模式：行高、内边距更小，一屏塞更多信息（真 IDE 默认更紧凑） */
  compact: boolean;
  /** 界面动画：面板淡入、按钮悬停位移等 */
  animations: boolean;
  /** 补全弹窗右侧的文档说明面板 */
  suggestDocs: boolean;
};

const KEY = 'float.settings';
export const SETTINGS_EVENT = 'float:settings-changed';

export const DEFAULT_SETTINGS: Settings = {
  theme: 'light',
  fontSize: 14,
  tabSize: 4,
  wordWrap: false,
  autoSave: true,
  runMode: 'compile',
  compact: false,
  animations: true,
  suggestDocs: true
};

export function loadSettings(): Settings {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const s = JSON.parse(raw) as Partial<Settings>;
    return {
      theme: s.theme === 'dark' || s.theme === 'system' ? s.theme : 'light',
      fontSize: typeof s.fontSize === 'number' ? Math.min(28, Math.max(10, s.fontSize)) : DEFAULT_SETTINGS.fontSize,
      tabSize: typeof s.tabSize === 'number' ? Math.min(8, Math.max(2, s.tabSize)) : DEFAULT_SETTINGS.tabSize,
      wordWrap: typeof s.wordWrap === 'boolean' ? s.wordWrap : DEFAULT_SETTINGS.wordWrap,
      autoSave: typeof s.autoSave === 'boolean' ? s.autoSave : DEFAULT_SETTINGS.autoSave,
      runMode: s.runMode === 'debug' ? 'debug' : 'compile',
      compact: typeof s.compact === 'boolean' ? s.compact : DEFAULT_SETTINGS.compact,
      animations: typeof s.animations === 'boolean' ? s.animations : DEFAULT_SETTINGS.animations,
      suggestDocs: typeof s.suggestDocs === 'boolean' ? s.suggestDocs : DEFAULT_SETTINGS.suggestDocs
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(s: Settings): void {
  try { window.localStorage.setItem(KEY, JSON.stringify(s)); } catch { /* 忽略 */ }
  window.dispatchEvent(new CustomEvent<Settings>(SETTINGS_EVENT, { detail: s }));
}

export function resolvedTheme(mode: ThemeMode): 'light' | 'dark' {
  if (mode === 'system') {
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return mode;
}

export function applySettings(s: Settings): void {
  const theme = resolvedTheme(s.theme);
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
  /* 视觉偏好也走 data 属性，CSS 里用 [data-compact='1'] 覆盖间距即可，
     不用在组件里到处传 prop。 */
  root.dataset.compact = s.compact ? '1' : '0';
  root.dataset.motion = s.animations ? '1' : '0';
  root.dataset.suggestDocs = s.suggestDocs ? '1' : '0';
  window.dispatchEvent(new CustomEvent<Settings>(SETTINGS_EVENT, { detail: s }));
}

/** 跟随系统时，系统切换要跟着变 */
export function watchSystemTheme(getMode: () => ThemeMode): () => void {
  const mq = window.matchMedia?.('(prefers-color-scheme: dark)');
  if (!mq) return () => {};
  const onChange = (): void => {
    if (getMode() === 'system') document.documentElement.dataset.theme = mq.matches ? 'dark' : 'light';
  };
  mq.addEventListener('change', onChange);
  return () => mq.removeEventListener('change', onChange);
}
