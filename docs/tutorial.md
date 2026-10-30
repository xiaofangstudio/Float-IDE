# Float Script 教程

> 面向零基础到进阶，一路写到类和继承。
> 配套 IDE：Float IDE（Windows 桌面，F5 运行、F6 调试）。
> 语言规范以 `language/catalog/core.json` 为准，本文是它的教学版。

---

## 目录

1. [这是什么](#1-这是什么)
2. [第一个程序](#2-第一个程序)
3. [语法骨架：一行一句，缩进成块](#3-语法骨架一行一句缩进成块)
4. [变量与类型](#4-变量与类型)
5. [表达式与运算符](#5-表达式与运算符)
6. [输出与输入](#6-输出与输入)
7. [条件分支](#7-条件分支)
8. [循环](#8-循环)
9. [函数](#9-函数)
10. [列表](#10-列表)
11. [字典](#11-字典)
12. [类与对象](#12-类与对象)
13. [特殊变量 __last_*](#13-特殊变量-__last_)
14. [模块：import](#14-模块import)
15. [调试：断点与单步](#15-调试断点与单步)
16. [为什么它比 Python 快](#16-为什么它比-python-快)
17. [看懂报错](#17-看懂报错)
18. [完整例子：控制台小游戏](#18-完整例子控制台小游戏)
19. [速查表](#19-速查表)
20. [常见问题](#20-常见问题)
21. [异常、方法与多返回值（0.4 新增）](#21-异常方法与多返回值04-新增)

---

## 1. 这是什么

Float Script（简称 Float，原名 CW Script / 代码世界）是一门**行式中文脚本语言**：

- **一行一句**：回车就是新语句，不用写分号；
- **缩进成块**：缩进比上一行深，就属于上一行的块；
- **中文友好**：变量名可以写中文，指令是英文单词但都有中文文档；
- **即时运行**：按 F5 就跑，没有"编译要等半天"的环节；
- **有类**：函数、列表、字典、类与继承一样不少。

它来自「代码世界」的脚本语言，但去掉了舞台/角色相关的指令，补上了函数参数、类、表达式，变成一门可以正经写程序的通用语言。

---

## 2. 第一个程序

新建工程 → 选「入门示例」→ F5。

```text
print("你好，Float！")
```

控制台会输出：

```text
你好，Float！
```

三件事记住：

| 写法 | 含义 |
| --- | --- |
| `print(值)` | 把值输出到控制台 |
| `"文本"` | 字符串，单双引号都行 |
| 换行 | 一条语句结束 |

---

## 3. 语法骨架：一行一句，缩进成块

```text
// 这是注释，整行不执行
-- 这也是注释（行首两个减号，整行跳过）

if (true)
    print("缩进 4 格，属于上面的 if")
    print("还是 if 里面")

print("回到最外层")
```

规则只有四条：

1. **一行一条语句**，行尾分号可写可不写（写了会被忽略）。
2. **缩进决定归属**：比上一行深 → 进入它的块；退回去 → 块结束。
3. **花括号可选**：`if (x) {` 和 `if (x)` 等价，只为看着习惯。
4. **空行随便加**，不影响执行。

> 缩进推荐 4 个空格。Tab 会按 4 个空格算，但别混着用。

---

## 4. 变量与类型

```text
int age = 18
float price = 9.9
str name = "小方"
bool ready = true

print(age)
print(price)
print(name)
print(ready)
```

六种类型名：`int` `float` `str` `bool` `list` `dict`。

**类型只是标注，不做强制检查**——写 `int x = 1.5` 不会报错，运行时它就是 1.5。这样写是为了让读代码的人一眼看出意图。

声明也可以省掉类型：

```text
score = 100          // 直接赋值也能创建变量
list nums = [1, 2]   // 列表字面量
dict hero = {"hp": 100}
```

初始化值可以是**任意表达式**：

```text
int a = 7
int b = 5
int sum = a + b * 2        // 17，先乘后加
str tag = "Lv." + (a - 2)  // "Lv.5"
```

变量名允许中文：

```text
int 血量 = 100
血量 -= 30
print(血量)
```

> 关键词（`if` / `for` / `out` / `class` …）不能当变量名。如果非要叫 `out`，
> 单独一行的 `out` 是"跳出循环"，`out += 1` 才会被当成变量。

---

## 5. 表达式与运算符

### 算术

```text
print(7 + 5)      // 12
print(7 - 5)      // 2
print(7 * 5)      // 35
print(7 / 2)      // 3.5
print(7 % 2)      // 1（取余）
print(2 ** 10)    // 1024（乘方）
```

`+` 遇到字符串就是拼接：

```text
print("第 " + 3 + " 关")   // 第 3 关
```

### 比较与逻辑

```text
print(5 > 3)              // true
print(5 == 5)             // true
print(5 != 4)             // true
print(5 >= 5 && 2 < 3)    // true
print(!true)              // false
```

优先级从低到高：`||` → `&&` → `|` → `^` → `&` → `== !=` → `< > <= >=` → `+ -` → `* / %` → `**`。

拿不准就加括号，括号永远最优先。

> 注意：条件里用 `&&` `||` `!`，**不要**写 `and` `or` `not`——那是内建函数名，不是运算符。

---

## 6. 输出与输入

```text
print("分数：", 95, "分")   // 多个值用空格隔开
```

```text
ask "你叫什么名字？"
print("你好，" + ans)
```

`ask` 会暂停脚本、在控制台弹出输入框，回答存进 `ans`，回车继续。

---

## 7. 条件分支

```text
int hp = 30

if (hp > 60)
    print("状态良好")
elif (hp > 20)
    print("受伤了")
else
    print("危险")
```

三种写法都认：

```text
if (x > 0) {           // 带花括号
    print("正")
} elif (x == 0) {
    print("零")
} else {
    print("负")
}

if x > 0               // 不写括号也行
    print("正")
```

`if` 后面不写条件 = 永远成立（等价 `if (true)`），主要给"占位"用。

---

## 8. 循环

### 数次数：`range`

```text
for i in range(5)        // 0 1 2 3 4
    print(i)

int n = 3
for i in range(n)        // 次数可以是变量、算式、函数调用
    print(i)

for i in range(2, 5)     // 2 3 4（含头不含尾）
    print(i)

for i in range(10, 0, -2)  // 10 8 6 4 2（步长）
    print(i)
```

循环变量随便取名，**循环体里能直接用**：

```text
for 序号 in range(3)
    print("第 " + (序号 + 1) + " 次")
```

嵌套循环各用各的变量：

```text
for i in range(2)
    for j in range(2)
        print(i + "," + j)
```

### 从 a 数到 b：`for i = a to b`

含两端：

```text
for i = 1 to 5        // 1 2 3 4 5
    print(i)

for i = 10 to 1 step -3   // 10 7 4 1
    print(i)
```

### 遍历集合

```text
list fruits = ["苹果", "香蕉", "橘子"]
for f in fruits
    print(f)

for ch in "abc"
    print(ch)

dict scores = {"小明": 90, "小红": 95}
for who in scores          // 字典默认给键
    print(who)

for who, score in scores   // 双变量拿键和值
    print(who + "：" + score)
```

### 条件循环

```text
int n = 0
while (n < 3)
    n++
    print(n)
```

### 重复 n 次

```text
repeat(3)
    print("再来一次")
```

### 死循环与跳出

```text
int count = 0
forever
    count++
    print(count)
    if (count >= 5)
        out          // 跳出循环
```

| 语句 | 作用 |
| --- | --- |
| `break` / `out` | 跳出整个循环 |
| `balo` / `continue` | 跳过本轮，进入下一轮 |
| `if out(条件)` | 条件成立就跳出（一行的写法） |
| `stop` | 直接停止整个脚本 |

`forever` 必须有跳出条件，否则 IDE 会在 2000 万步时掐断并报错——不是卡死。

### 等待

```text
wait 0.5              // 等半秒
wait(1)

wait until (ready)    // 条件为假时一直重复块内语句
    ready = check()
```

---

## 9. 函数

### 定义与调用

```text
function greet(name)
    return "你好，" + name

print(greet("小明"))
```

- 参数可以有多个：`function add(a, b)`
- 默认值：`function greet(name, greeting = "你好")`
- 没有 `return` 时返回 `null`
- 函数可以先调用后定义（顶层函数在运行前统一注册）

```text
print(early())        // 先调用

function early()
    return "没问题"
```

### 递归

```text
function fact(n)
    if (n <= 1)
        return 1
    return n * fact(n - 1)

print(fact(5))   // 120
```

### 默认值与参数的引用

默认值可以引用前面的参数：

```text
function area(w, h = w)
    return w * h

print(area(5))      // 25
print(area(3, 4))   // 12
```

### 作用域

函数内声明的变量是**局部变量**，函数外看不见：

```text
int x = 1

function shadow()
    int x = 100
    return x

print(shadow())   // 100
print(x)          // 1
```

但要改全局变量，直接赋值即可：

```text
int counter = 0
function bump()
    counter += 1

bump()
bump()
print(counter)   // 2
```

---

## 10. 列表

```text
list nums = [3, 1, 2]
print(nums)          // [3, 1, 2]
print(nums[0])       // 3
print(nums[-1])      // 2（负数从末尾数）

nums[0] = 9
print(nums)          // [9, 1, 2]
```

追加与增删：

```text
add 4 to nums            // 末尾追加
insert 0 to nums at 0    // 指定位置插入
replace nums 0 with 7    // 改某个位置
del nums 2               // 删某个位置
print(len(nums))         // 长度
```

遍历、求和：

```text
int total = 0
for v in nums
    total += v
print(total)
```

`len` 也是表达式函数，可以直接用：

```text
print(len(nums) * 2)
print(nums[0] + nums[1])
```

---

## 11. 字典

```text
dict scores = {"小明": 90, "小红": 95}
print(scores["小明"])    // 90
print(scores.小明)       // 90（点号写法也行）

scores["小刚"] = 88
scores.小刚 = 88
```

词形指令写法（和代码世界一致）：

```text
dict scores key "小刚" = 88
dict scores read "小红"
print(__last_read)
dict scores del "小明"
```

常用函数：

```text
print(dictallkey(scores))        // 全部键
print(dictallval(scores))        // 全部值
print(dictin(scores, "小红"))    // 键是否存在
print(len(scores))               // 键的数量
```

遍历：

```text
for who, score in scores
    print(who + "：" + score)
```

---

## 12. 类与对象

类把"数据 + 操作数据的函数"打包在一起。

### 定义一个类

```text
class Hero
    str name = ""
    int hp = 100

    function init(name, hp)      // 构造方法
        self.name = name
        self.hp = hp

    function hurt(damage)
        hp -= damage              // 字段可以直接裸名读写
        if (hp < 0)
            hp = 0
        return hp

    function info()
        return self.name + " 剩余 " + hp

Hero h = new Hero("阿方", 120)
print(h.info())        // 阿方 剩余 120
h.hurt(30)
print(h.hp)            // 90
```

要点：

| 概念 | 写法 |
| --- | --- |
| 字段 | 类体里 `int hp = 100`，默认值可以是表达式 |
| 构造 | `function init(参数)`，`new 类名(参数)` 时自动调用 |
| 方法 | 类体里的 `function` |
| 自己 | `self` / `this`；字段可以直接写名字 |
| 调用 | `obj.方法(参数)`、`obj.字段` |
| 改字段 | `obj.hp = 50`、`obj.hp -= 10` |

方法之间可以直接互相调用：

```text
class Counter
    int n = 0

    function inc()
        n += 1

    function incTwice()
        inc()          // 不用写 self.inc()
        inc()
        return n
```

### 继承

```text
class Animal
    str name = ""

    function init(name)
        self.name = name

    function speak()
        return "……"

class Dog extends Animal
    str breed = "土狗"

    function init(name, breed)
        super(name)              // 调父类的 init
        self.breed = breed

    function speak()             // 覆盖父类方法
        return name + "（" + breed + "）：汪！"

Dog d = new Dog("大黄", "中华田园犬")
print(d.speak())        // 大黄（中华田园犬）：汪！
```

- 子类没写 `init` 时直接沿用父类的；
- `super(参数)` 只能用在子类方法里；
- 父类的方法、字段子类都能用。

### 对象放进列表

```text
list team = [new Dog("大黄"), new Animal("路人")]
for p in team
    print(p.speak())
```

---

## 13. 特殊变量 `__last_*`

从「代码世界」继承下来的老习惯：**单独成行**写一条指令，它的结果会顺手存进一个特殊变量。

```text
int a = 7
int b = 5
add(a, b)              // 单独一行 → 结果写进 __last_calc
print(__last_calc)     // 12
```

| 变量 | 谁写进去的 |
| --- | --- |
| `ans` | `ask` 的回答 |
| `__last_calc` | `add` `sub` `mul` `div` `mod` `pow` `rand` `sqrt` `abs` `floor` `ceil` `round` |
| `__last_cmp` | `lt` `gt` `eq` `and` `or` `not` `listin` `dictin` `contain` |
| `__last_read` | `read` / `dict … read` / `dictallkey` / `dictallval` |
| `__last_len` | `len` |
| `__last_str` | `join` `char` `strlen` |
| `__last_time` | `time read` |

新写法更推荐直接取值：

```text
print(add(7, 5))            // 12
print(len(nums))            // 直接当表达式用
```

老写法照样能跑，不用改。

---

## 14. 模块：`import`

```text
import "lib.cw"      // 按文件
import "tools"       // 按文件夹（找 tools/main.cw）
```

被导入文件里的函数、类、全局变量都会带进来。

`lib.cw`：

```text
function twice(x)
    return x * 2
```

`main.cw`：

```text
import "lib.cw"
print(twice(21))    // 42
```

> 导入是"运行到这一行才执行"，循环导入只会执行一次。

---

## 15. 调试：断点与单步

| 操作 | 快捷键 | 说明 |
| --- | --- | --- |
| 运行 | F5 | 编译执行，最快 |
| 调试 | F6 | 解释执行，断点与单步生效 |
| 继续 | F8 | 跑到下一个断点 |
| 单步跳过 | F10 | 执行下一条，不进入函数 |
| 单步进入 | F11 | 钻进函数里 |
| 单步跳出 | Shift+F11 | 从当前函数里出来 |
| 停止 | Shift+F5 | 立刻终止 |

- **打断点**：点编辑器行号左边的空白，出现红点；也可以把光标放到某行按 `F9`。
- 完整快捷键（含 `Ctrl+W` 关文件、`Alt+1/2/3` 切面板、`Ctrl+±` 缩放字号）按 `F1` 查看。
- 暂停时，右栏「变量」会列出当前所有变量（含对象字段），当前行有黄色箭头。
- 调试默认停在第一条语句，之后按 F8 继续。

> 为什么调试比运行慢？调试走的是解释器（每行都能停），运行走的是编译器（翻译成 JS 交给 V8）。两套引擎语义一致，结果相同。

---

## 16. 为什么它比 Python 快

Float 不解释 AST——按 F5 时先把整棵树**编译成 JavaScript**，再交给 V8 跑。这一步只花零点几毫秒，所以体感是"按下就跑"，但跑起来是接近原生的速度。

实测（Windows / Node 22 / CPython 3.13，同一台机器，预热后取稳定值）：

| 用例 | Float（编译） | CPython 3.13 | 倍率 |
| --- | --- | --- | --- |
| `fib(30)` 递归 | 21 ms | 114 ms | **快 5.4 倍** |
| `for` 累加 500 万次 | 15 ms | 133 ms | **快 8.8 倍** |
| 字符串拼接 5 万次 | 2.5 ms | 2.9 ms | 快 1.2 倍 |

复现：`npm run bench:lang` 与 `python tests/bench.py`。

---

## 17. 看懂报错

两类错误，都在控制台里给中文说明。

### 语法错误（运行前就拦下）

```text
for i in range(
    print(i)
```

```text
发现 1 处语法错误，先改好再运行：
  main.cw 第 1 行第 16 列：这里应该是一个值，实际是行尾
```

编辑器对应位置会画红色波浪线，悬停能看原因。

### 运行时错误（带行号与调用栈）

```text
function half(x)
    return x / 0

print(half(10))
```

```text
出错（main.cw 第 2 行）：除数不能为 0
    half() 在 main.cw 第 1 行
```

常见错误：

| 提示 | 原因 |
| --- | --- |
| 变量「x」没有定义 | 拼写错了，或者还没赋值 |
| 类 X 没有方法「y」 | 方法名写错，或忘了继承 |
| 下标 3 超出范围（长度 3） | 下标从 0 开始，最大是长度减 1 |
| 比较大小需要两个数字或两个字符串 | 拿数字和 `null` 比了 |
| 「+」需要数字 | 字符串和数字混用减法 |

---

## 18. 完整例子：控制台小游戏

```text
// ─ 怪物与勇者：一个能跑的小游戏 ─

class Fighter
    str name = ""
    int hp = 100
    int atk = 10

    function init(name, hp, atk)
        self.name = name
        self.hp = hp
        self.atk = atk

    function alive()
        return hp > 0

    function hurt(damage)
        hp -= damage
        if (hp < 0)
            hp = 0
        return hp

    function attack(target)
        int damage = atk + rand(0, 4)
        target.hurt(damage)
        return name + " 打中 " + target.name + "，" + damage + " 点伤害"

class Boss extends Fighter
    int rage = 0

    function attack(target)
        rage += 1
        int damage = atk + rage * 2
        target.hurt(damage)
        return name + " 暴怒一击，" + damage + " 点伤害"

Fighter hero = new Fighter("勇者", 90, 14)
Boss boss = new Boss("石巨人", 160, 12)

print("战斗开始！")
int round = 0

while (hero.alive() && boss.alive())
    round++
    print("── 第 " + round + " 回合 ──")
    print(hero.attack(boss))
    if (boss.alive())
        print(boss.attack(hero))
    print(hero.name + "：" + hero.hp + "  |  " + boss.name + "：" + boss.hp)

if (hero.alive())
    print("你赢了！用了 " + round + " 回合")
else
    print("你倒下了……")
```

试试改这些地方：

1. 给 `Fighter` 加个 `defend()` 方法，减伤一半；
2. 加一个 `heal` 道具，用 `ask` 让玩家选择攻击还是回血；
3. 把 `rand(0, 4)` 改成 `rand(0, 10)`，看看难度变化。

---

## 19. 速查表

### 语句

| 写法 | 说明 |
| --- | --- |
| `int x = 1` | 声明（类型可省） |
| `x = 1` / `x += 1` / `x++` | 赋值 / 复合赋值 / 自增 |
| `print(值, …)` | 输出 |
| `ask "问题"` | 提问，答案在 `ans` |
| `if (条件)` / `elif` / `else` | 分支 |
| `for i in range(n)` | 0 到 n-1 |
| `for i in range(a, b)` / `range(a, b, step)` | 区间 / 带步长 |
| `for i = 1 to 10` | 含两端 |
| `for x in 列表/字符串/字典` | 遍历 |
| `for k, v in 字典` | 键值遍历 |
| `while (条件)` / `repeat(n)` / `forever` | 循环 |
| `break` / `out` / `balo` / `continue` | 跳出 / 跳过 |
| `wait 0.5` / `wait until (条件)` | 等待 |
| `function f(a, b = 1)` | 定义函数 |
| `return 值` | 返回 |
| `class A` / `class B extends A` | 定义类 / 继承 |
| `new A(参数)` | 创建实例 |
| `import "lib.cw"` | 导入 |

### 内建函数

| 分类 | 函数 |
| --- | --- |
| 数学 | `add sub mul div mod pow sqrt abs floor ceil round sin cos tan asin acos atan atan2 log exp min max rand random` |
| 比较 | `lt gt eq listin dictin contain` |
| 逻辑 | `and or not` |
| 字符串 | `join char strlen upper lower trim split indexof substr` |
| 集合 | `len range dictallkey dictallval` |
| 转换 | `int float str bool` |
| 类型 | `type` |

### 列表 / 字典指令

| 写法 | 说明 |
| --- | --- |
| `list xs = [1, 2]` | 声明列表 |
| `add 值 to xs` | 追加 |
| `insert 值 to xs at 位置` | 插入 |
| `replace xs 位置 with 值` | 替换 |
| `del xs 位置` | 删除 |
| `len xs` | 长度（写 `__last_len`） |
| `dict d = {"k": 1}` | 声明字典 |
| `dict d key 键 = 值` | 写入 |
| `dict d read 键` | 读取（写 `__last_read`） |

---

## 20. 常见问题

**Q：为什么 `if` 的条件要加括号？**
不加也行（`if x > 0` 可以），加括号只是更清楚、和别的语言更像。

**Q：`int` 真的会把小数截断吗？**
不会。类型只是标注，不做转换。要截断请用 `floor(x)` 或 `int(x)`。

**Q：函数能返回多个值吗？**
用列表：`return [a, b]`，接收方 `list r = f()`，再 `r[0]`。

**Q：循环里改了 `i` 会怎样？**
`for` 的循环变量每轮重新绑定，改它不影响下一轮的计数。

**Q：为什么同名变量在函数里改了，外面没变？**
函数内 `int x = …` 是局部变量。要改外面的，别重新声明，直接 `x = …`。

**Q：程序跑飞了怎么办？**
Shift+F5 停止。死循环也会被执行器自动掐断（2000 万步），不会卡死界面。

**Q：能写图形/窗口程序吗？**
Float 核心语言只做控制台程序（原版代码世界的舞台指令不在范围内）。它的定位是：练语法、写算法、做文本小游戏。

---

*小方工作室 · Float Script 教程 · 写完记得按 F5 试试*

---

## 21. 异常、方法与多返回值（0.4 新增）

### 21.1 抛出与捕获

```text
try
    throw "出错了"
    print("这行不会执行")
catch e
    print("抓到：" + e.message)
finally
    print("不管成功失败都会执行")
```

- `throw 值`：抛出一个错误。抛字符串最简单，也可以抛字典 `{"message": "…"}`。
- `catch e`：捕获错误，`e` 是个字典，有三个键：

| 键 | 含义 |
| --- | --- |
| `e.message` | 错误说明 |
| `e.line` | 出错行号（0 表示未知） |
| `e.file` | 出错文件 |

- `finally` 可选，无论成功、出错还是提前 `return` 都会执行。
- 不写 `catch` 只写 `finally` 也合法：错误会继续往上抛，但清理代码一定跑。
- 函数里抛的错误会一路往上冒，直到被某一层 `catch` 接住；没人接就打印到控制台并终止脚本。

```text
function divide(a, b)
    if (b == 0)
        throw "除数不能为 0"
    return a / b

try
    print(divide(10, 2))
    print(divide(1, 0))
catch err
    print("出问题：" + err.message)
```

### 21.2 方法调用（链式）

字符串、列表、字典都能像对象一样调方法，可以串起来写。

**字符串**

```text
print("  hello  ".trim().upper())     // HELLO
print("a,b,c".split(","))             // ["a", "b", "c"]
print("abc".replace("b", "B"))        // aBc
print("hello".find("ll"))             // 2（找不到返回 -1）
print("x".repeat(3))                  // xxx
print("abc".starts_with("ab"))        // true
print("42".to_int() + 1)              // 43
```

可用：`upper lower trim split replace find contains starts_with ends_with substr repeat chars to_int to_float len`

**列表**

```text
list xs = [3, 1, 2]
xs.push(4)              // 追加，返回新长度
xs.sort()               // 原地排序
print(xs.join("-"))     // 1-2-3-4
print(xs.pop())         // 4（取出并删除末尾）
xs.reverse()
print(xs.slice(0, 2))   // [3, 2]
print(xs.contains(2))   // true
print(xs.sum())         // 6
```

可用：`push pop insert remove index_of contains join sort reverse slice sum clear len`

**字典**

```text
dict d = {"a": 1, "b": 2}
print(d.keys())         // ["a", "b"]
print(d.values())       // [1, 2]
print(d.has("a"))       // true
print(d.get("z", 0))    // 0（没这个键时给默认值）
d.remove("b")
print(d.len())          // 1
```

可用：`keys values items has remove get set len`

### 21.3 字符串插值

字符串里用 `{…}` 直接嵌变量或表达式，比一路 `+` 清爽：

```text
int score = 95
str name = "小明"

print("得分：{score}")
print("{name} 下一关要打 {score + 10} 分")
print("长度：{len(name)}，大写：{name.upper()}")
```

- 花括号里可以写任何表达式：变量、运算、函数调用、方法调用。
- 想输出真的花括号，用 `\{` 转义：`print("\{不是插值\}")`。

### 21.4 多个返回值

`return a, b` 会把几个值打包成列表返回，接收端可以一次解构：

```text
function minmax(xs)
    int lo = xs[0]
    int hi = xs[0]
    for v in xs
        if (v < lo)
            lo = v
        if (v > hi)
            hi = v
    return lo, hi

int low, int high = minmax([5, 3, 9, 1])
print(low)    // 1
print(high)   // 9
```

解构也可以不带类型：`low, high = minmax(data)`。
右边的值少于左边要的个数时会报错，不会静默给 `null`。
