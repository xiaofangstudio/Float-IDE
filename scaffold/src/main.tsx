import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './styles.css';

/* 兜底：渲染层一旦崩了，绝不允许只留一片黑屏。
 * 把错误原文直接画在页面上 —— 用户能看到「什么错」，
 * 我们也不用再靠猜。 */
function showFatal(title: string, detail: string): void {
  const root = document.getElementById('root');
  if (!root) return;
  const esc = (s: string): string =>
    s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] ?? c);
  root.innerHTML = `
    <div style="padding:32px;font:14px/1.7 'Microsoft YaHei',system-ui;color:#1a1a1a;background:#fff;height:100vh;overflow:auto">
      <div style="font-size:18px;font-weight:700;margin-bottom:6px">${esc(title)}</div>
      <div style="color:#666;margin-bottom:16px">界面出错了。下面这段原文请直接发给开发者；也可以先点下面的按钮试试恢复。</div>
      <pre style="white-space:pre-wrap;background:#f5f6f8;border:1px solid #e2e5ea;border-radius:6px;padding:12px;font-size:12.5px">${esc(detail)}</pre>
      <button onclick="location.reload()" style="margin-top:14px;padding:6px 14px;border:1px solid #c4d0e0;border-radius:6px;background:#fff;cursor:pointer;font:inherit">重新加载界面</button>
    </div>`;
}

window.addEventListener('error', (e) => {
  showFatal('界面出错了', `${e.message}\n${e.filename || ''}:${e.lineno || 0}`);
});
window.addEventListener('unhandledrejection', (e) => {
  /* Promise 拒绝说明界面本体还活着（事件循环都在跑），只是某个异步操作失败——
   * 把整页盖成错误只会把好界面变成死页（连 ✕ 都点不了）。打到控制台即可，
   * 真正的渲染崩溃由 React 错误边界和上面的 error 兜底负责。 */
  const r = e.reason as { stack?: string; message?: string } | undefined;
  console.error('[Float] 未处理的 Promise 拒绝：', r?.stack || r?.message || String(e.reason));
});

const el = document.getElementById('root');
if (!el) {
  document.body.innerHTML = '<pre style="padding:24px">找不到 #root 节点，index.html 不完整</pre>';
} else {
  try {
    createRoot(el).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>
    );
  } catch (e) {
    showFatal('界面没起来', e instanceof Error ? `${e.message}\n${e.stack ?? ''}` : String(e));
  }
}
