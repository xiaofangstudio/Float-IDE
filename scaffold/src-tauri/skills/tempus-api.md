# Tempus 运行时 API

Tempus 的每一段动画都是「时间的纯函数」：给定帧号，画面唯一确定。
这是逐帧导出可复现、可并行的根基。写代码前先读本文。

## 注册合成（入口文件 src/index.tsx）

```tsx
import React from 'react';
import { registerComposition, Main } from './scenes/main';

registerComposition({
  id: 'main',            // 合成标识（导出文件名、MCP render_video 用）
  width: 1920,
  height: 1080,
  fps: 30,
  durationInFrames: 150, // 时长（帧）；150 帧 @30fps = 5 秒
  component: Main
});
```

可注册多个合成，界面右侧「工程」面板可切换。

## 读时间：useFrame / useVideoConfig

```tsx
import { useFrame, useVideoConfig } from 'tempus';

function Main() {
  const frame = useFrame();               // 当前帧号（整数）
  const { fps, width, height, durationInFrames } = useVideoConfig();
  // 一切动画都从 frame 推导，禁止 setState / Math.random / Date.now
}
```

## 插值：interpolate

```tsx
interpolate(frame, [0, 30], [0, 1])                       // 0..30 帧线性 0→1，越界自动 clamp
interpolate(frame, [0, 30], [100, 900], { easing: Easing.outCubic })
```

## 弹簧：spring

```tsx
spring({ frame: frame - 10, fps, config: { damping: 10 } })   // 第 10 帧起弹簧 0→1
spring({ frame, fps, from: 0, to: 720 })                       // 弹到 720
spring({ frame, fps, durationInFrames: 30 })                   // 压缩到 30 帧内完成
```

## 序列：Sequence

```tsx
<Sequence from={60} durationInFrames={90}>   // 第 60 帧进场，占 90 帧
  <Intro />                                  // 内部 useFrame 从 0 重新计数
</Sequence>
```

## 缓动函数（Easing）

linear / inQuad / outQuad / inOutQuad / inCubic / outCubic / inOutCubic /
outExpo / inOutExpo / outBack / outElastic

## 标记：mark（时间轴会显示）

```tsx
mark(90, '副歌');     // 组件体内每帧执行是安全的，相同标记只登记一次
```

## 确定性渲染纪律（违反 = 导出画面错乱）

1. 每帧画面只是 frame 的函数。
2. 禁止：Math.random、Date.now、requestAnimationFrame 自驱动状态、跨帧可变量。
3. 需要随机感用确定性哈希：`const r = Math.abs(Math.sin(seed * 12.9898) * 43758.5453) % 1`。
4. 布尔开关（如是否已出现）用 `frame >= t` 推导，不要存状态。
5. React 状态只用于组织结构，不用于动画时间。

## 内置依赖与限制

- 可用依赖：react、react-dom、tempus（运行时），以及工程内的相对路径文件（.tsx/.ts/.jsx/.js/.css）。
- 其余 npm 包暂未内置；css 直接 `import './x.css'` 会注入页面。
- 导出：无头浏览器逐帧截图 + ffmpeg 编码 H.264 MP4；音频轨在路线图中。
