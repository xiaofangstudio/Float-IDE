import React, { useEffect, useState } from 'react';

/* 内联 SVG 图标集：全站图标统一出处，禁止 emoji。 */

type P = { size?: number; style?: React.CSSProperties };

const s = (size?: number) => ({ width: size ?? 16, height: size ?? 16, fill: 'none', stroke: 'currentColor', strokeWidth: 1.7, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const });

export const IconPlay = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M7 4.8v14.4l12-7.2z" fill="currentColor" stroke="none" /></svg>
);
export const IconPause = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M7 4.5v15M17 4.5v15" strokeWidth={2.6} /></svg>
);
export const IconPrev = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M18 5.5v13L8.5 12z" fill="currentColor" stroke="none" /><path d="M6 5v14" /></svg>
);
export const IconNext = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M6 5.5v13L15.5 12z" fill="currentColor" stroke="none" /><path d="M18 5v14" /></svg>
);
export const IconStepBack = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M14 6l-6 6 6 6" /></svg>
);
export const IconStepFwd = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M10 6l6 6-6 6" /></svg>
);
export const IconLoop = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M17 3l3 3-3 3" /><path d="M20 6H8a4 4 0 0 0-4 4v1" /><path d="M7 21l-3-3 3-3" /><path d="M4 18h12a4 4 0 0 0 4-4v-1" /></svg>
);
export const IconExport = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 3v12" /><path d="M7 10l5 5 5-5" /><path d="M4 19h16" /></svg>
);
export const IconFolder = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M3 6a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
);
export const IconFile = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M6 3h8l4 4v14H6z" /><path d="M14 3v4h4" /></svg>
);
export const IconPlus = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 5v14M5 12h14" /></svg>
);
export const IconClose = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" /></svg>
);
export const IconImport = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 15V3" /><path d="M7 10l5 5 5-5" /><path d="M4 19h16" /></svg>
);
export const IconSave = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M5 3h11l3 3v15H5z" /><path d="M8 3v5h7V3" /><path d="M8 21v-7h8v7" /></svg>
);
export const IconTrash = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M4 7h16" /><path d="M9 7V4h6v3" /><path d="M6 7l1 13h10l1-13" /></svg>
);
export const IconMcp = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="7" rx="2" /><rect x="3" y="13" width="18" height="7" rx="2" /><path d="M7 7.5h.01M7 16.5h.01" /></svg>
);
export const IconBook = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M4 5a2 2 0 0 1 2-2h13v18H6a2 2 0 0 1-2-2z" /><path d="M4 17a2 2 0 0 1 2-2h13" /></svg>
);
export const IconInfo = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></svg>
);
export const IconRefresh = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M20 12a8 8 0 1 1-2.5-5.8" /><path d="M20 4v5h-5" /></svg>
);
export const IconTerminal = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M7 9l3 3-3 3" /><path d="M12 15h5" /></svg>
);
export const IconBranch = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><circle cx="6" cy="5" r="2.2" /><circle cx="6" cy="19" r="2.2" /><circle cx="18" cy="9" r="2.2" /><path d="M6 7.2v9.6" /><path d="M18 11.2c0 3-2.5 4.3-5.5 4.3H9" /></svg>
);
export const IconCloudUp = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M6.5 19a4.5 4.5 0 0 1-.4-8.98A6 6 0 0 1 17.8 9.1 4.2 4.2 0 0 1 17.5 19z" /><path d="M12 16V9" /><path d="M9 12l3-3 3 3" /></svg>
);
export const IconCloudDown = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M6.5 19a4.5 4.5 0 0 1-.4-8.98A6 6 0 0 1 17.8 9.1 4.2 4.2 0 0 1 17.5 19z" /><path d="M12 9v7" /><path d="M9 13l3 3 3-3" /></svg>
);
export const IconHistory = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M4 12a8 8 0 1 1 2.3 5.6" /><path d="M4 13v-4h4" /><path d="M12 8v4.5l3 2" /></svg>
);
export const IconEdit = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17z" /><path d="M14.5 6.5l3 3" /></svg>
);
export const IconCheck = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M4.5 12.5l5 5L19.5 7" /></svg>
);

export const IconStop = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor" stroke="none" /></svg>
);
export const IconStepOver = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 4v9" /><path d="M8 10l4 4 4-4" /><circle cx="12" cy="19" r="1.8" fill="currentColor" stroke="none" /></svg>
);
export const IconStepInto = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 4v8" /><path d="M8 9l4 4 4-4" /><path d="M7 19h10" /></svg>
);
export const IconStepOut = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 13V5" /><path d="M8 8l4-4 4 4" /><path d="M7 20h10" /></svg>
);
export const IconDebug = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M8 5.5l9 6.5-9 6.5z" fill="currentColor" stroke="none" /><circle cx="17.5" cy="5.5" r="2" /><path d="M17.5 7.5v3" /></svg>
);
export const IconBp = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><circle cx="12" cy="12" r="5" fill="currentColor" stroke="none" /></svg>
);

export function Logo({ size = 84 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 96 96" fill="none">
      <rect x="6" y="6" width="84" height="84" rx="22" fill="#ffffff" stroke="#c5d4ea" strokeWidth="2" />
      {/* 水面波纹 + 浮标：Float */}
      <path d="M18 62c6-5 12-5 18 0s12 5 18 0 12-5 18 0 12 5 18 0" stroke="#1f8fe8" strokeWidth="3" strokeLinecap="round" />
      <path d="M22 72c5-4 11-4 16 0s11 4 16 0 11-4 16 0" stroke="#6fc0ff" strokeWidth="2.4" strokeLinecap="round" opacity=".8" />
      <path d="M48 18v34" stroke="#16273d" strokeWidth="5" strokeLinecap="round" />
      <path d="M48 22l14 10-14 10z" fill="#1f8fe8" />
      <circle cx="48" cy="18" r="4.5" fill="#16273d" />
    </svg>
  );
}
export const IconImage = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2.5" /><circle cx="8.5" cy="9.5" r="1.6" /><path d="M4 17l4.5-4.5 3.5 3.5 3-3L20 17" /></svg>
);
export const IconSearch = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></svg>
);
export const IconSettings = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><circle cx="12" cy="12" r="3.2" /><path d="M12 3.5v2.2M12 18.3v2.2M4.6 7.8l1.9 1.1M17.5 15.1l1.9 1.1M4.6 16.2l1.9-1.1M17.5 8.9l1.9-1.1" /></svg>
);
export const IconBug = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="8" y="7" width="8" height="11" rx="4" /><path d="M8 11H4M20 11h-4M8 15H5M19 15h-3M9 7.5L7.5 5M15 7.5L16.5 5" /></svg>
);
export const IconPlug = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M9 3v5M15 3v5" /><path d="M6 8h12v3a6 6 0 0 1-6 6 6 6 0 0 1-6-6V8z" /><path d="M12 17v4" /></svg>
);
export const IconServer = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="7" rx="2" /><rect x="3" y="13" width="18" height="7" rx="2" /><path d="M7 7.5h.01M7 16.5h.01" /></svg>
);
export const IconKeyboard = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><rect x="2.5" y="6.5" width="19" height="11" rx="2" /><path d="M6 10h.01M9.5 10h.01M13 10h.01M16.5 10h.01M6 13.5h.01M9.5 13.5h.01M13 13.5h.01M16.5 13.5h.01M8 16.5h8" /></svg>
);
export const IconLayers = ({ size, style }: P) => (
  <svg {...s(size)} style={style} viewBox="0 0 24 24"><path d="M12 3.5l8 4.5-8 4.5-8-4.5z" /><path d="M4 12l8 4.5 8-4.5" /><path d="M4 16.5L12 21l8-4.5" /></svg>
);
