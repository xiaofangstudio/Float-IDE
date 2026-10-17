> 小方工作室 · 出品 ｜ Float IDE ｜ Float Script 桌面编辑器 ｜ MIT

# Float IDE

**Float Script 的桌面编辑器。** Float Script（简称 Float）是小方工作室自研的编程语言，从「代码世界」的 CW Script 演化而来：保留简单直接的运行模型，把语法整理得更接近 Python。

## 一句话立论

> IDE 的高亮、折叠、大纲、跳转、引用、重命名、诊断，全部建立在真正的词法分析、语法分析与语义分析之上，不是正则匹配。

正则做不到的事：

| 能力 | 正则方案 | 基于 AST 的方案 |
| --- | --- | --- |
| 高亮 | 关键字表逐行匹配，字符串与注释里的关键字会被误染色 | 先分词，字符串与注释天然不会被当成关键字 |
| 跳转定义 | 只能全局文本搜索，同名变量与字符串字面量都会命中 | 从标识符节点反查符号表，只认真正的定义与引用 |
| 重命名 | 全局替换会误伤字符串、注释与局部同名变量 | 改在语法树上再回写源码，作用域外不受影响 |
| 错误定位 | 只能提示「这一行有怪字符」 | 给出期望符号、实际问题与精确行列 |

## 语言速览

| 项 | 内容 |
| --- | --- |
| 名称 | Float Script |
| 版本 | 0.1.0 |
| 扩展名 | `.cw` |
| 编码 / 换行 | UTF-8 / LF |
| 缩进 | 4 个空格；子语句缩进必须严格大于父语句 |
| 注释 | `#`（旧代码的 `//` 仍可解析） |
| 类型 | `int` `float` `str` `bool` `list` `dict` `any`，可省略，仅作校验 |
| 标识符 | 沿用 CW Script 规则，支持中文 |
| 语句结束 | 换行；分号可选，仅为兼容旧代码 |

```cw
# Float Script 0.1 — 你好，浮点

name: str = input("你叫什么名字？")
print("你好，" + name)

count: int = 0
for i in range(3):
    count += 1
    print("第 " + str(count) + " 次")
```

### 关键字

`if` `elif` `else` `while` `for` `in` `range` `break` `continue` `return` `def` `import` `del` `and` `or` `not` `true` `false` `none`

### 内建

`len` `str` `int` `float` `bool` `sqrt` `abs` `floor` `ceil` `round` `random` `randint` `range` `print` `input` `sleep` `wait_until` `exit`

列表、字典、字符串各有常用方法，例如 `list.append()`、`dict.keys()`、`str.split()`。

## 相对 CW Script 的变化

- 语法 Python 化：`def` 定义函数、`for ... in range()` 循环、`#` 注释
- 新增 `continue`、一元负号、列表与字典字面量、成员方法调用、默认参数
- `+=` 等复合赋值的右侧可以是任意表达式
- `function bool` / `function repo` 改为装饰器 `@bool` / `@repo`
- 舞台、角色、画笔、外观、碰撞、克隆、鼠标、联机指令从核心剥离，归档为 `stage` 扩展，未来以扩展包提供
- 旧写法保留在 `legacy` 迁移层，只用于旧代码兼容，不进入新代码

## 仓库结构

| 路径 | 内容 |
| --- | --- |
| `language/catalog/core.json` | 语言核心目录：类型、关键字、优先级、AST 构造、内建函数、容器方法 |
| `language/catalog/stage.json` | 舞台扩展（已归档，核心不加载） |
| `language/catalog/legacy.json` | CW Script 兼容层与迁移对照 |
| `language/samples/` | 四份 `.cw` 样例：你好、控制流、集合、函数 |
| `reference/original/` | 「代码世界」原始页面与 CW Script IDE，只读参考 |
| `tools/` | Eclipse DSL + JDK 21 本地工具链，不入库 |

## 路线

1. 冻结语言规范
2. 词法 / 语法 / 语义分析
3. 编辑器能力：高亮、折叠、大纲、跳转定义、查找引用、重命名、诊断
4. 打包与分发

## 边界

- 不带任何 AI 功能
- 不做账号体系，不登录、不联网校验
- 永不自动更新
- 全中文界面
- 完全免费

## 许可

MIT，非营利，小方工作室出品。

---

*小方工作室 · Float IDE*
