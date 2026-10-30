/* Float IDE 新建工程模板：空白 / 入门 / 进阶 / 类与对象。 */

export type Template = { id: string; name: string; desc: string; files: Record<string, string> };

const projectJson = (entry: string, name: string): string =>
  JSON.stringify({ name, entry, version: '0.2.0', kind: 'float' }, null, 2) + '\n';

const TPL_BLANK = `// main.cw —— 从这里开始写你的 Float 脚本
print("你好，Float！")
`;

const TPL_STARTER = `// ─ 入门示例：变量 / 条件 / 循环 ─

ask "你叫什么名字？"
print(join("你好，", ans))

// 变量声明右边可以写任意表达式
int a = 7
int b = 5
int sum = a + b
print(join("7 + 5 = ", sum))

// 条件：> < == != && || !
if (sum > 10)
    print("结果比 10 大")
else
    print("结果不超过 10")

// 循环变量 i 能直接用，次数也可以是变量
int times = 3
for i in range(times)
    print(join("第 ", i + 1, " 声问好"))

// 遍历列表
list fruits = ["苹果", "香蕉", "橘子"]
for f in fruits
    print("水果：" + f)

print("入门示例结束")
`;

const TPL_ADVANCED = `// ─ 进阶示例：函数 / 集合 / 字典 / 计时 ─

// 带参数与返回值的函数，可在表达式里直接调用
function priceWithTax(price, rate = 0.1)
    return round(price * (1 + rate))

print("100 元含税：" + priceWithTax(100))
print("100 元按 13%：" + priceWithTax(100, 0.13))

// 递归
function fib(n)
    if (n < 2)
        return n
    return fib(n - 1) + fib(n - 2)
print("fib(10) = " + fib(10))

// 列表：字面量 + 指令 + 下标
list nums = [3, 1, 2]
add 4 to nums
print("长度：" + len(nums))
print("排序前第一个：" + nums[0])
nums[0] = 9

int total = 0
for v in nums
    total += v
print("求和：" + total)

// 字典：键值对
dict scores = {"小明": 90, "小红": 95}
scores["小刚"] = 88
for who, score in scores
    print(join(who, "：", score))
print("平均分：" + (90 + 95 + 88) / 3)

// 计时：time reset 清零，time read 读取（单位秒）
time reset
wait 0.3
time read
print(join("耗时（秒）：", __last_time))

print("进阶示例结束")
`;

const TPL_CLASS = `// ─ 类与对象 ─

class Player
    str name = ""
    int hp = 100
    int mp = 30

    function init(name, hp)
        self.name = name
        self.hp = hp

    function hurt(damage)
        hp -= damage
        if (hp < 0)
            hp = 0
        return hp

    function alive()
        return hp > 0

    function info()
        return join(name, " HP ", hp, " MP ", mp)

class Mage extends Player
    int mana = 50

    function init(name)
        super(name, 80)

    function fireball(target)
        if (mana < 10)
            return "蓝不够了"
        mana -= 10
        target.hurt(25)
        return join(name, " 火球命中 ", target.name, "，对方剩 ", target.hp)

Player hero = new Player("战士", 120)
Mage wizard = new Mage("法师")

print(hero.info())
print(wizard.info())
print(wizard.fireball(hero))
print(hero.info())

// 对象可以放进列表统一处理
list team = [wizard, hero]
for p in team
    print(join(p.name, " 还活着吗：", p.alive()))
`;

function files(entry: string, name: string, main: string): Record<string, string> {
  return { 'project.json': projectJson(entry, name), [entry]: main };
}

export const TEMPLATES: Template[] = [
  { id: 'blank', name: '空白工程', desc: '只含 main.cw 与 project.json，一张白纸', files: files('main.cw', 'Float 工程', TPL_BLANK) },
  { id: 'starter', name: '入门示例', desc: '变量 / 条件 / 循环 / 列表，十行看懂 Float', files: files('main.cw', 'Float 入门', TPL_STARTER) },
  { id: 'advanced', name: '进阶示例', desc: '函数 / 集合 / 字典 / 计时，写法速查', files: files('main.cw', 'Float 进阶', TPL_ADVANCED) },
  { id: 'class', name: '类与对象', desc: '字段 / 方法 / 继承 / super，面向对象起步', files: files('main.cw', 'Float 类示例', TPL_CLASS) }
];
