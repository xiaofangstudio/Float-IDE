# 液态玻璃（Liquid Glass）

Web 端液态玻璃质感：折射、色散边缘、高光流动。核心三件套：
`backdrop-filter`（模糊折射）+ SVG 位移贴图（真实折射形变）+ 内阴影高光（玻璃厚度）。
以下代码可直接粘贴进 Tempus 组件或任意网页。

## 基础玻璃卡

```css
.liquid-glass {
  position: relative;
  border-radius: 24px;
  background: rgba(255, 255, 255, 0.06);
  backdrop-filter: blur(14px) saturate(1.6) brightness(1.08);
  -webkit-backdrop-filter: blur(14px) saturate(1.6) brightness(1.08);
  box-shadow:
    inset 0 1px 1px rgba(255, 255, 255, 0.55),   /* 顶部高光：玻璃厚度 */
    inset 0 -1px 1px rgba(255, 255, 255, 0.18),  /* 底部回光 */
    inset 1px 0 1px rgba(255, 255, 255, 0.25),
    0 18px 50px rgba(0, 0, 0, 0.35);             /* 投影 */
  border: 1px solid rgba(255, 255, 255, 0.14);
}
```

## 真折射：SVG 位移贴图

backdrop-filter 的 blur 只有毛玻璃；想要「透镜感」需要 displacement：
先定义 feTurbulence + feDisplacementMap 的 SVG 滤镜，再在 CSS 里引用。

```html
<svg width="0" height="0" style="position:absolute">
  <filter id="lens" x="-20%" y="-20%" width="140%" height="140%">
    <feTurbulence type="fractalNoise" baseFrequency="0.008 0.012"
                  numOctaves="2" seed="7" result="noise"/>
    <feGaussianBlur in="noise" stdDeviation="2" result="soft"/>
    <feDisplacementMap in="SourceGraphic" in2="soft" scale="46"
                       xChannelSelector="R" yChannelSelector="G"/>
  </filter>
</svg>
```

```css
.liquid-glass-lens {
  backdrop-filter: url(#lens) blur(6px) saturate(1.5) brightness(1.1);
  -webkit-backdrop-filter: blur(6px) saturate(1.5); /* 不支持 url() 时退化为毛玻璃 */
}
```

## 流动高光（纯函数，可在 Tempus 里逐帧驱动）

```tsx
import { useFrame, useVideoConfig, interpolate, AbsoluteFill } from 'tempus';

function GlassCard() {
  const frame = useFrame();
  const { durationInFrames } = useVideoConfig();
  // 高光带从左扫到右，收尾淡出
  const sweep = interpolate(frame, [10, durationInFrames - 15], [-30, 130]);
  return (
    <AbsoluteFill style={{ background: 'linear-gradient(135deg,#1a1040,#0b1020)',
      alignItems: 'center', justifyContent: 'center' }}>
      <div style={{
        width: 620, height: 360, borderRadius: 32,
        background: 'rgba(255,255,255,0.06)',
        backdropFilter: 'blur(14px) saturate(1.6) brightness(1.08)',
        boxShadow: 'inset 0 1px 1px rgba(255,255,255,0.55), 0 18px 50px rgba(0,0,0,0.35)',
        position: 'relative', overflow: 'hidden'
      }}>
        <div style={{
          position: 'absolute', top: 0, bottom: 0, width: 220,
          left: `${sweep}%`,
          background: 'linear-gradient(105deg, transparent, rgba(255,255,255,0.22), transparent)',
          transform: 'skewX(-18deg)'
        }} />
      </div>
    </AbsoluteFill>
  );
}
```

## 参数经验表

| 参数 | 作用 | 建议值 |
|---|---|---|
| blur | 毛玻璃强度 | 8–16px，太大显脏 |
| saturate | 提饱和，玻璃「活」 | 1.4–1.8 |
| feDisplacementMap scale | 折射强度 | 30–60，强透镜感 |
| baseFrequency | 噪声频率 | 0.004–0.015，越低越像大曲率 |
| inset 高光 | 玻璃厚度感 | 上亮下暗，1px 级别 |
| 背景透明度 | 玻璃体色 | 0.04–0.10 |

## 注意

- Chrome/Edge 121+ 支持 backdrop-filter: url(#filter)；不支持的浏览器自动退化为普通毛玻璃（已内置 fallback）。
- 液态玻璃层叠不超过两层，否则 GPU 合成开销剧增。
- 深色背景上玻璃要「亮边暗体」；浅色背景上反之。
