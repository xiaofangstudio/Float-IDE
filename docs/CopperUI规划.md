# Copper UI 规划（函数表 v0 草案）

> 小方工作室出品 ｜ 状态：**设计稿，未实现**
>
> Copper UI 是给 Copper 的样式 + 布局层。类比一句话：**Copper 之于 JS，Copper UI 之于 CSS**。
> 逻辑写 `.co`（Copper），外观写 `.cui`（Copper UI），结构与样式分离。

## 0. 已定案

| 决策 | 结论 |
|---|---|
| 语言形态 | **A 方案：真 CSS 风**——选择器 + 属性声明，不搞组件树内联样式 |
| 逻辑语言 | Copper（交互、事件、状态都写 `.co`） |
| 文件后缀 | `.cui` |
| 语法基调 | 与 Copper 同一套习惯：一行一句、分号可省、`/*` 注释到行尾、缩进或花括号成块 |
| 属性命名 | **英文正式名**（跟 Copper 新代码规范一致：`readfile` 而不是 `读文件`；中文别名最多做兼容层，不进补全不进文档） |
| v0 编译目标 | CSS + DOM（WebView2 宿主，Float IDE 预览面板直接渲染） |
| v1 编译目标 | Deft 样式系统（Skia 自绘，FangUI kernel），摆脱 WebView |
| 预览环境 | Float IDE 新增「预览标签页」：`.co` + `.cui` 实时渲染、改动秒刷 |

## 1. 语法速览

```cui
/* Copper UI 速览 —— 一行一条声明，分号可省 */

Window {
  width: 400
  height: 300
  background: s0
}

Button.primary {
  background: a1
  color: t0
  radius: 8
  padding: 8, 16
}

/* 伪类用冒号，跟 CSS 同款 */
Button.primary:hover {
  background: a2
}

/* 状态、子代、多条件 */
List > Item:nth(2) { background: s3 }
Button:disabled { color: t4 }
```

选择器速查：

| 写法 | 含义 | 对应 CSS |
|---|---|---|
| `Button` | 元素类型 | 标签/role 选择器 |
| `.primary` | 类名 | `.class` |
| `#submit` | 命名实例 | `#id` |
| `:hover` `:pressed` `:disabled` `:focus` `:checked` | 交互态伪类 | 同名/映射为 `:active` 等 |
| `A > B` `A B` | 直接子代 / 任意后代 | 同款 |
| `:nth(2)` `:first` `:last` `:even` `:odd` | 位置伪类 | `:nth-child(...)` |
| `,` | 多选择器并列 | 同款 |

## 2. 属性总表

状态标记：✅ v0（转 CSS 就能实现）｜🔜 v1（Deft 落地时做）｜💤 远期。
「映射」列 = v0 编译出的 CSS 属性；无映射 = v0 暂不编译。

### 2.1 尺寸与盒模型

| 属性 | 取值 | 说明 | 映射 | 状态 |
|---|---|---|---|---|
| `width` `height` | 数值 \| `fill` \| `fit` | `fill` = 撑满父级，`fit` = 包住内容 | `width` / `height`（fill→`100%`，fit→`fit-content`） | ✅ |
| `min-width` `max-width` `min-height` `max-height` | 数值 | 尺寸钳制 | 同名 | ✅ |
| `margin` | 数值 \| `上, 右, 下, 左` | 外边距，1~4 值简写 | `margin` | ✅ |
| `padding` | 数值 \| `上, 右, 下, 左` | 内边距，1~4 值简写 | `padding` | ✅ |
| `border` | `宽度, 样式, 颜色` | 如 `1, solid, b2` | `border` | ✅ |
| `border-top/right/bottom/left` | 同上 | 单边 | 同名 | ✅ |
| `radius` | 数值 \| `四角` | 圆角；`宽/高` 可得胶囊 | `border-radius` | ✅ |
| `box-sizing` | `border` \| `content` | 默认 `border`（比 CSS 更合理的默认） | `box-sizing` | ✅ |

### 2.2 布局

| 属性 | 取值 | 说明 | 映射 | 状态 |
|---|---|---|---|---|
| `layout` | `flow` \| `hstack` \| `vstack` \| `stack` \| `grid` | 默认 `flow`；hstack/vstack 即 flex row/column；stack 为层叠 | `display` | ✅ |
| `direction` | `row` \| `column` \| `row-reverse` \| `column-reverse` | 仅 flow 下有意义 | `flex-direction` | ✅ |
| `justify` | `start` `end` `center` `between` `around` | 主轴对齐 | `justify-content` | ✅ |
| `align` | `start` `end` `center` `stretch` | 交叉轴对齐 | `align-items` | ✅ |
| `self-align` | 同上 | 单个子项覆盖 | `align-self` | ✅ |
| `gap` | 数值 \| `行, 列` | 间距 | `gap` | ✅ |
| `wrap` | `no` \| `yes` | 换行 | `flex-wrap` | ✅ |
| `grow` `shrink` | 数值 | 占比/收缩 | `flex-grow/shrink` | ✅ |
| `basis` | 数量 \| `auto` | 基准尺寸 | `flex-basis` | ✅ |
| `position` | `static` \| `relative` \| `absolute` \| `fixed` | 定位 | 同名 | ✅ |
| `inset` | 数值 \| `上, 右, 下, 左` | 偏移（top/right/bottom/left 合体） | 四属性展开 | ✅ |
| `z-index` | 整数 | 层级 | 同名 | ✅ |
| `overflow` | `visible` \| `hidden` \| `scroll` \| `auto` | 溢出；scroll 即可滚动 | `overflow` | ✅ |
| `columns` | `n` \| `宽, n` | grid 的行列定义 | `grid-template` | 🔜 |
| `span` | `跨行, 跨列` | grid 单元合并 | `grid-area` | 🔜 |

### 2.3 文本

| 属性 | 取值 | 说明 | 映射 | 状态 |
|---|---|---|---|---|
| `text` | 字符串 | 仅 Text 元素；支持 `{表达式}` 插值 | `textContent` | ✅ |
| `font` | 字体名 \| `字体名, 备选...` | 字族 | `font-family` | ✅ |
| `size` | 数值 | 字号（默认 14） | `font-size` | ✅ |
| `weight` | `light` `regular` `medium` `semibold` `bold` | 字重，告别 400/700 数字 | `font-weight` | ✅ |
| `color` | 颜色 \| 令牌 | 文字色 | `color` | ✅ |
| `line-height` | 数值（倍数） | 行高，1.5 即 1.5 倍 | `line-height` | ✅ |
| `text-align` | `start` `center` `end` `justify` | 对齐 | `text-align` | ✅ |
| `letter-spacing` | 数值 | 字距 | 同名 | ✅ |
| `overflow-text` | `clip` \| `ellipsis` | 溢出省略号 | `text-overflow` | ✅ |
| `lines` | 整数 | 多行截断（-webkit-line-clamp 封装） | line-clamp | ✅ |
| `decoration` | `none` `underline` `strike` | 下划线/删除线 | `text-decoration` | ✅ |

### 2.4 背景与视觉

| 属性 | 取值 | 说明 | 映射 | 状态 |
|---|---|---|---|---|
| `background` | 颜色 \| 渐变 \| `image(路径)` | 底色/底图 | `background` | ✅ |
| `image-fit` | `fill` `fit` `cover` `crop` | 图的填充方式 | `object-fit`/`background-size` | ✅ |
| `opacity` | 0~1 | 不透明度 | `opacity` | ✅ |
| `shadow` | `x, y, 模糊, 颜色` \| `none` | 投影 | `box-shadow` | ✅ |
| `glow` | `模糊, 颜色` | 外发光（无偏移 shadow 的糖） | `box-shadow` 展开 | ✅ |
| `blur` | 数值 | 自身模糊 | `filter: blur()` | 🔜 |
| `frost` | 数值 | 背景磨砂（Float 的磨砂面板同款） | `backdrop-filter` | 🔜 |
| `clip` | `默认` \| `内容` | 子元素是否被圆角裁剪 | `overflow: hidden` | ✅ |

### 2.5 变换

| 属性 | 取值 | 说明 | 映射 | 状态 |
|---|---|---|---|---|
| `translate` | `x, y` | 平移，支持百分比 | `translate` | ✅ |
| `scale` | 数值 \| `x, y` | 缩放 | `scale` | ✅ |
| `rotate` | 角度 | 旋转，`45deg` 或 `0.25turn` | `rotate` | ✅ |
| `origin` | `center` `top-left`... | 变换原点 | `transform-origin` | ✅ |

### 2.6 动效

| 属性 | 取值 | 说明 | 映射 | 状态 |
|---|---|---|---|---|
| `transition` | `属性 时长 缓动, ...` | 过渡；`all 0.2` 即全属性 0.2s | `transition` | ✅ |
| `duration` `easing` | 数值 / 缓动名 | 独立写法（糖） | 展开 | ✅ |
| `animation` | `关键帧名 时长 缓动 次数` | 循环动画 | `animation` | ✅ |
| `@frames 名 { from {…} to {…} }` | 关键帧块 | 0%/100% 也可写百分比 | `@keyframes` | ✅ |
| `spring` | `刚度, 阻尼` | 弹簧缓动（Deft 侧原生，v0 转 cubic-bezier 近似） | cubic-bezier | 🔜 |
| `ease` | `linear` `ease` `in` `out` `in-out` | 预设缓动 | 同名 | ✅ |

## 3. 取值函数表

值里可以调函数（编译期求值为主）：

| 函数 | 返回 | 说明 | 状态 |
|---|---|---|---|
| `rgb(r, g, b)` / `rgba(r, g, b, a)` | 颜色 | 0~255 | ✅ |
| `hex("#RRGGBB")` | 颜色 | 也直接写字面量 `#fff` | ✅ |
| `mix(c1, c2, t)` | 颜色 | 两色插值，t=0~1 | ✅ |
| `lighten(c, t)` / `darken(c, t)` | 颜色 | 明度增减 | ✅ |
| `alpha(c, a)` | 颜色 | 改不透明度 | ✅ |
| `linear-gradient(方向, 色1, 色2, ...)` | 渐变 | 方向：`to top` / `45deg` | ✅ |
| `radial-gradient(色1, 色2, ...)` | 渐变 | 径向 | ✅ |
| `calc(表达式)` | 数量 | `calc(fill - 16)`、`calc(50% + 8)` | ✅ |
| `token(名)` | 任意令牌 | 显式取主题令牌，等价于裸写令牌名 | ✅ |
| `spring(刚度, 阻尼)` | 缓动 | 弹簧参数 | 🔜 |

## 4. 主题令牌

裸写令牌名即可取值（`background: s1`），语义与 Float IDE 主题系统对齐，深浅主题自动换挡：

| 令牌 | 含义 |
|---|---|
| `s0` `s1` `s2` `s3` `s4` | 表面阶梯：窗口底 → 面板 → 编辑区 → 下沉 → 悬停 |
| `b1` `b2` `b3` | 边框：极淡分割 → 常规 → 强调 |
| `t0` `t1` `t2` `t3` `t4` | 文字：反白 → 主 → 次 → 弱 → 极弱 |
| `a1` `a2` `a3` `a4` | 主色：主 → 悬停 → 淡底 → 选中底 |
| `ok` `err` `warn` `info` | 语义色：墨绿 / 砖红 / 琥珀褐 / 蓝 |

主题文件 `theme.cui` 定义令牌值，`@theme 深色 { ... }` 覆盖块换肤。

## 5. Copper 侧交互 API（函数表）

`.co` 里 `import ui` 拿到这套（v0 在 Float IDE 预览宿主里实现，与 `ui` 控制台库合并命名空间）：

| 函数 / 方法 | 签名 | 说明 | 状态 |
|---|---|---|---|
| `ui.find` | `ui.find("#submit")` → Element | 按选择器找元素，取第一个 | ✅ |
| `ui.find_all` | `ui.find_all(".item")` → list | 按选择器找全部 | ✅ |
| `el.on` | `el.on("tap", fn)` | 事件：`tap` `press` `hover` `input` `change` | ✅ |
| `el.class_add` | `el.class_add("active")` | 加类（样式随之切换） | ✅ |
| `el.class_remove` / `el.class_toggle` | 同形 | 去类 / 开关类 | ✅ |
| `el.has_class` | `el.has_class("active")` → bool | 查类 | ✅ |
| `el.get` | `el.get("width")` → 值 | 读当前生效样式（含级联结果） | ✅ |
| `el.set` | `el.set("background", "a3")` | 内联改样式（最高优先级） | ✅ |
| `el.text` | `el.text = "新文案"`（可读可写） | 文本内容，支持 `{表达式}` 插值 | ✅ |
| `el.show` / `el.hide` | `el.hide()` | 显隐（占位保留用 `el.set("visibility","hidden")`） | ✅ |
| `el.create` | `ui.create("Button", class: "x")` → Element | 动态建元素 | ✅ |
| `el.append` / `el.remove` | `parent.append(el)` | 挂载 / 移除 | ✅ |
| `el.animate` | `el.animate("shake", 0.3)` | 临时播一段动画 | 🔜 |
| `ui.window` | `ui.window.width = 800` | 窗口级操作 | 🔜 |
| `ui.tick` | `ui.tick(fn)` | 每帧回调（游戏/进度） | 🔜 |
| `ui.render` | `ui.render()` | 手动触发重绘（默认自动） | 🔜 |

## 6. 元件类型表

选择器里能用的元素类型（v0 与预览宿主内置元素对齐）：

| 类型 | 说明 |
|---|---|
| `Window` | 窗口根 |
| `Text` | 文本 |
| `Button` | 按钮 |
| `Input` | 单行输入 |
| `TextArea` | 多行输入 |
| `Image` | 图片 |
| `Box` | 通用容器（div） |
| `List` | 列表容器 |
| `Item` | 列表项 |
| `Checkbox` / `Radio` | 选择控件 |
| `Slider` / `Progress` | 滑杆 / 进度 |
| `Scroll` | 滚动容器 |

## 7. 分期清单

- **v0（Float IDE 内闭环）**
  - [ ] 本 spec 定稿：属性集冻结为上表 ✅ 项
  - [ ] `.cui` 词法/语法（复用 Copper 解析习惯），进 float-lang
  - [ ] 编译器：`.cui` → CSS（Rust 侧新模块，挨着 aot.rs）
  - [ ] Float IDE 预览标签页：`.co` + `.cui` 双文件渲染、热刷新
  - [ ] `ui` 库桥接：find/on/class_add/text 等 Element API
  - [ ] Monaco 支持：高亮、补全（属性+令牌+函数）、悬停文档
- **v1（Deft 落地）**
  - [ ] Copper VM 嵌入 Deft kernel（Rust 内核现成，AOT 可直编）
  - [ ] `.cui` → Deft 样式系统（Skia）；`spring` 缓动原生实现
  - [ ] `grid` / `blur` / `frost` 补齐
- **v2（生态）**
  - [ ] 组件库（按钮/卡片/弹层用 Copper UI 自举）
  - [ ] 教程进 Float 学习中心；示例工程加「Copper UI」模板
