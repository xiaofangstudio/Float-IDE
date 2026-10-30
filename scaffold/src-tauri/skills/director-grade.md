# 导演级 2D 动画（director-grade）

拉开「真动画」与「廉价动图」的从来不是工具，而是**导演纪律**。
本技能把纪律编成强制门禁：spec 先行、角色锁定、配色有弧线、切镜有动机、音画同表。
在 Tempus 中：每一帧都是 frame 的纯函数，纪律可以直接机器化执行。

## 五道门禁（缺一不可，顺序不可换）

### 门禁一：当导演——立分镜硬规矩

写代码之前先产出 `spec.json`（放工程根目录，随手可改）。硬规矩：

1. **每镜必须有东西在动**：事件、因果、或至少一个铺垫在兑现；没有「氛围镜头」。
2. **画面默认不出现文字**：要说的画出来；唯一例外是歌词视频的逐字卡点与极少量大字拟声。
3. **角色设计全程锁定**：rig 函数一旦定稿，任何镜头不得变形——只能换姿态、换表情，不能换结构。
   用「转身画关键帧」而不是 3D 投影。
4. **配色有演进弧线**：从开场到结尾有一个统一的色彩旅程（如暖→冷、灰→饱和），
   而不是每镜随手配色。
5. **每次切镜都要有动机**：硬切跟动作、擦除跟世界规则、 iris 跟主体；无动机的切 = 返工。

spec.json 最小结构：

```json
{
  "title": "示例短片",
  "fps": 30, "width": 1920, "height": 1080, "durationInFrames": 450,
  "palette": { "start": ["#2b2d42", "#8d99ae"], "end": ["#ef476f", "#ffd166"] },
  "characters": [
    { "name": "小鸟", "rig": "drawBird(ctx, frame, pose, x, y)", "locked": true }
  ],
  "shots": [
    { "id": "s1", "from": 0, "to": 90, "action": "小鸟破壳，蛋壳弹飞",
      "cut_to_next": "动作跟随：蛋壳飞出画时硬切" }
  ],
  "beatMap": { "bpm": 96, "events": [ { "frame": 0, "what": "开场拍" } ] }
}
```

### 门禁二：当制片——写风格手册

写进 spec.json：调色板（含弧线）、角色规格（锁定）、背景风格（禁止纯色平涂当背景：
草地/水面/天空要有底色 + 纹理 + 笔触变化 + 前后景深四层）、动效语言（缓动与挤压拉伸规则）。

### 门禁三：定节拍表

画面和配乐共用同一张 BPM 网格：
- 每个动作落在节拍上，切镜落在小节上；
- mark() 把节拍登记到时间轴，与音乐对齐检查；
- 配乐可程序化合成（WebAudio 按节拍表生成）或外部素材，但**网格必须同源**。

### 门禁四：校验（机器强制）

对照检查 spec.json，任何一条不过禁止写码：

```
[ ] 每个 shot 的 to > from，且 action 非空
[ ] characters 每项 locked = true
[ ] palette.start 与 palette.end 不同（有弧线）
[ ] 每个 shots[i].cut_to_next 非空（有切镜动机）
[ ] beatMap.bpm 存在且 events 覆盖开场与结尾
```

### 门禁五：写码 → 逐帧确认 → 成片

Tempus 实现要点：

```tsx
// 单 <canvas> + renderAt(frame) 纯函数；组件体内禁止 setState / 随机 / 实时时钟
function Main() {
  const frame = useFrame();
  const { width, height } = useVideoConfig();
  const ref = React.useRef<HTMLCanvasElement>(null);
  React.useEffect(() => { renderAt(frame, ref.current?.getContext('2d'), width, height); });
  return <canvas ref={ref} width={width} height={height} style={{ width: '100%', height: '100%' }} />;
}
```

- `renderAt(frame, ...)` 是唯一画帧入口：姿态、形变、配色、运镜全部由 frame 推导；
- 角色用函数化 rig：`drawBird(ctx, frame, poseIndex, x, y)`，多镜头复用；
- 需要稳定随机时用确定性哈希（`hash(i)` 风格），禁止 Math.random；
- 邻镜之间用**连续形变**（同一形状持续变形）而不是独立图形生硬切换——这是「读出是连续动画」的关键；
- 分段确认：先用时间轴 mark + 抽帧看关键帧，再整片导出。

## 常见坑

- 图层顺序：门口/窗口/屏幕类构图，先画内部再画回外框，别让超出开口的内容露出来；
- 退化几何（NaN 点）会让 Canvas 整帧不画，先守卫；
- 大幅 zoom 下描边要随缩放补偿，否则细节镜头全部失边；
- 成本 = 填充与描边次数：宁可少而大，不要多而碎。

## 交付检查

- [ ] spec.json 先于代码，且五条校验全过？
- [ ] 同 frame 同画面（确定性）？
- [ ] 每镜有动作、无无关文字、角色锁定、配色有弧线、切镜有动机？
- [ ] 画面与配乐挂同一张节拍表？
- [ ] 关键帧逐个看过、无空帧无大块空白？
- [ ] MP4 复制到工程根目录交付？
