import React, { useEffect, useState } from 'react';
import { Logo } from './icons';

const TIPS = [
  '正在准备语言引擎……',
  '正在装载编辑器……',
  '马上就好……'
];

/* 启动页：进度用定时器推进（不依赖 requestAnimationFrame，
 * 窗口在后台被节流时也能正常走完），最少 900ms、最多 1400ms。 */
export function Loading({ onDone }: { onDone: () => void }) {
  const [p, setP] = useState(0);

  useEffect(() => {
    const t0 = performance.now();
    const timer = window.setInterval(() => {
      const el = performance.now() - t0;
      if (el >= 1400) {
        window.clearInterval(timer);
        setP(1);
        onDone();
        return;
      }
      setP(el / 1400);
    }, 24);
    return () => window.clearInterval(timer);
  }, [onDone]);

  const tip = TIPS[Math.min(TIPS.length - 1, Math.floor(p * TIPS.length))];

  return (
    <div className="loading-wrap">
      <div className="loading-logo"><Logo size={96} /></div>
      <div style={{ fontSize: 22, fontWeight: 800, letterSpacing: 3 }}>Float IDE</div>
      <div style={{ fontSize: 11, letterSpacing: 2, color: 'var(--txt3)', marginTop: 2 }}>代码世界脚本工作室</div>
      <div className="loading-bar"><div style={{ width: `${p * 100}%` }} /></div>
      <div className="loading-tip">{tip}</div>
    </div>
  );
}
