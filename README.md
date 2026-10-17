> 小方工作室 · 出品 ｜ Float IDE ｜ CW Script 桌面编辑器 ｜ MIT

# Float IDE

**CW Script（代码世界）的桌面 IDE。** 把网页版的 `CW Script IDE` 做成桌面软件：语言与运行模型保持原样，界面重做，高亮、大纲、诊断与调试建立在真正的词法、语法、语义分析之上，而不是正则匹配。

## 一、它是什么

- **语言是 CW Script。** 本仓库的 `language/catalog/core.json` 把「CW Script 去掉舞台相关指令」后的核心子集记作 **Float Script**，并明确标注：**语法与 CW Script 保持一致，未做改写**。扩展名仍是 `.cw`。
- **形态是桌面 IDE。** 交互原型是 `reference/original/CW Script IDE.html`——一个单文件 HTML 的完整 IDE，已包含资源管理器、语法高亮、大纲、自动补全、断点与单步调试、变量监视、控制台 / 时间线、模拟提交、导出 NEX。
- **工具链在 `tools/`。** Eclipse IDE for DSL Developers 2026-09（含 Xtext 2.44、LSP4E、tm4e）与 JDK 21，用于实现语言服务与桌面 IDE。**`tools/` 是本地工具链，不入库。**

## 二、语言速查

### 文件与排版

| 项 | 规则 |
| --- | --- |
| 名称 | Float Script（CW Script 的核心子集） |
| 扩展名 / 编码 / 换行 | `.cw` / UTF-8 / LF |
| 行注释 | `// 文本` |
| 整行跳过 | `-- 任意文本`（不是注释节点，直接跳过） |
| 分号 | 行尾 `;` 可选，匹配后忽略 |
| 花括号 | `{ }` 可选，块是否存在只由缩进决定 |
| 缩进 | 子语句必须比父语句更深；缩进回退到父级或更浅即结束当前块 |
| 空行 | 忽略，不生成 AST 节点 |
| 分支合并 | 只有 `} elif (条件) {` / `} else {` 与上一行同行书写，才合并为 `if_elif_else` |
| 类型 | `int` `float` `str` `bool` `list` `dict`（没有 `any`，也没有 `none`） |

### 控制

```cw
repeat(10)
for i in range(10)
for i = 1 to 10
forever
main loop
while (条件)
if (条件)
elif (条件)
else
break
out                 // 跳出当前块，等价 break
balo                // 跳过本轮，等价 continue
if out(条件)        // 条件成立时跳出当前块
wait 0.5
wait(表达式)
wait until(条件)
stop all
stop now
stop script
```

- `repeat`、`for i in range` 的计数都从 0 开始，只接受数字字面量。
- 单独的 `if` 不写条件时按 `true` 处理。
- `for_range` 的循环计数**没有**写入 `sb.vars['i']`，循环体内不要依赖变量 `i`。

### 变量

```cw
int count = 0
float ratio = 0.5
str name = "小方"
bool ready = true

name = 表达式          // 右值走完整表达式求值
count++
count--
count += 1             // 复合赋值原版只接受数字字面量
count -= 1
count *= 2
count /= 2
count %= 2
```

声明语句的初始化值只走字面量解析：数字、字符串、`true` / `false`、`[]`、`{}`、变量名、`ans`，**不会求值表达式**。需要表达式时，先声明，再用 `name = 表达式` 赋值。

### 函数

```cw
function hello()
function bool isReady()
function repo load()
fn bye()
Function foo()
f bar()
```

- `bool` / `repo` 是函数修饰符，不是装饰器。
- 带参定义写作 `function greet(name)`；`f(param) {` 定义的是固定名为 `_f` 的函数，且只支持一个参数。
- 文本解析层目前只打通**无参调用** `name()`，带参调用尚未打通。

### 输入输出与运行时

```cw
print("文本")
print(表达式)
ask "你叫什么名字？"
str name = ans
time read
time reset
```

### 列表

```cw
list nums
add 3 to nums
del nums 0
insert 1 to nums at 0
replace nums 0 with 9
read nums 1
len nums
listin(1, nums)
```

列表指令的下标只接受数字字面量。`read` 的结果写入 `__last_read`，`len` 写入 `__last_len`，`listin` 写入 `__last_cmp`。

### 字典

```cw
dict scores
dict scores key "小明" = 90
dict scores read "小明"
dict scores del "小明"
dictallkey(scores)
dictallval(scores)
dictin(scores, "小明")
```

`dict read` / `dictallkey` / `dictallval` 的结果写入 `__last_read`，`dictin` 写入 `__last_cmp`。

### 数学、比较、逻辑、字符串

| 类别 | 指令 | 结果变量 |
| --- | --- | --- |
| 数学 | `add(a,b)` `sub` `mul` `div` `mod` `pow` `rand` `sqrt` `abs` `floor` `ceil` `round` | `__last_calc` |
| 比较 | `lt(a,b)` `eq(a,b)` `gt(a,b)` | `__last_cmp` |
| 逻辑 | `and(a,b)` `or(a,b)` `not(a)` | `__last_cmp` |
| 字符串 | `join(a,b)` `char(s,i)` `strlen(s)` | `__last_str` |
| 包含 | `contain(s,sub)` | `__last_cmp` |

### 特殊变量

| 变量 | 写入者 |
| --- | --- |
| `ans` | `ask` 的回答 |
| `__last_calc` | 全部 `math_*` 指令 |
| `__last_cmp` | `cmp_*` / `logic_*` / `listin` / `dictin` / `contain` |
| `__last_read` | `list read` / `dict read` / `dictallkey` / `dictallval` |
| `__last_len` | `len` |
| `__last_str` | `join` / `char` / `strlen` |
| `__last_time` | `time read` |
| `__last_ans` | `ans` |

运算指令**不返回值**，结果写进这些隐式变量。

### 表达式

表达式由手写的 Pratt Parser（`safeEvalExpr`）求值，支持整数、小数、单双引号字符串、`true` / `false`。

| 优先级 | 运算符 |
| --- | --- |
| 10 | `\|\|` |
| 15 | `&&` |
| 20 | `\|` |
| 25 | `^` |
| 30 | `&` |
| 35 | `==` `!=` `===` `!==` |
| 40 | `<` `>` `<=` `>=` |
| 45 | `<<` `>>` |
| 50 | `+` `-` |
| 60 | `*` `/` `%` |
| 70 | `**`（右结合） |

一元运算符：`-` `!` `~`；下标：`列表[下标]`；可调用：`Math.*`、`sin` `cos` `tan` `asin` `acos` `atan` `atan2` `sqrt` `abs` `floor` `ceil` `round` `log` `exp` `pow` `min` `max` `random` `join` `char` `strlen` `contain` `len`。

**表达式层不支持：**

- `and` / `or` / `not` 关键字（会被当成标识符）
- 把 `lt` / `eq` / `gt` / `add` / `sub` / `mul` / `div` / `mod` / `pow` 等指令名当函数调用
- 成员方法调用，例如 `name.append(...)`

条件表达式请使用 `> < >= <= == != && || !`。

### 模块与注释

```cw
import "file.ext"     // 路径含 . 时按文件导入
import "folder"       // 路径不含 . 时按文件夹导入

// 普通注释
// ─ 组开始 ─          供大纲折叠
// ─ 组结束 ─
-- 整行跳过
```

未识别的文本会生成 `unknown` 节点，并给出近似指令提示。

## 三、样例

仓库内四份样例就是语言现状的最好说明：

| 文件 | 内容 |
| --- | --- |
| `language/samples/01_hello.cw` | `ask` / `ans` / `print` / `repeat` |
| `language/samples/02_control.cw` | `repeat` / `for i in range` / `if_elif_else` / `while` / `out` / `balo` |
| `language/samples/03_collections.cw` | 列表与字典，以及 `__last_read` / `__last_len` / `__last_cmp` |
| `language/samples/04_functions.cw` | `function` / `fn` / `function bool` / 无参调用 |

```cw
// 01 你好 —— Float Script

ask "你叫什么名字？"
str name = ans
print("你好，")
print(name)

int count = 0
repeat(3) {
    count++
    print(count)
}
```

## 四、仓库结构

| 路径 | 内容 |
| --- | --- |
| `language/catalog/core.json` | 语言核心目录：排版规则、类型、特殊变量、全部 AST 构造、表达式优先级与限制 |
| `language/catalog/stage.json` | 舞台扩展归档：移动、外观、画笔、造型、克隆、鼠标、碰撞、事件、联机、变量面板 |
| `language/samples/` | 四份 `.cw` 样例 |
| `reference/original/` | 「代码世界」原始页面与 `CW Script IDE.html`，只读参考 |
| `tools/` | Eclipse DSL 与 JDK 21 本地工具链，不入库 |

`stage.json` 的定位是：舞台、角色、画笔、外观、碰撞、克隆、鼠标与联机指令已从核心剔除，Float 核心不加载该扩展，未来以可实时更新的扩展包提供。

## 五、原型里已有的 IDE 能力

`reference/original/CW Script IDE.html` 是一个可直接打开的完整原型：

- 资源管理器、大纲、变量监视
- 自研语法高亮与行号栏
- 自动补全、断点、条件断点
- 运行 / 调试 / 继续 / 单步跳过 / 单步进入 / 单步跳出
- 控制台与时间线
- 「提交」按钮与「导出为 NEX」
- 菜单：文件、编辑、查看、运行、终端、帮助

## 六、边界

- 语言核心不加载舞台扩展；舞台能力归入 `stage` 扩展。
- 当前文本解析层只支持无参函数调用。
- 声明语句的初始化值不求值表达式。
- `and` / `or` / `not` 不是关键字。

## 七、许可

MIT，小方工作室出品。

---

*小方工作室 · Float IDE*
