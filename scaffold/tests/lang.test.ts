/* Float Script 语言内核测试：解析、表达式、循环、函数、类、指令兼容。
 * 运行：npm run test:lang
 */

import { run, debugTrace } from './harness';
import { parseProgram } from '../src/lang/parse';
import { analyze } from '../src/lang/analyze';

let pass = 0;
const failures: string[] = [];

function eq(name: string, actual: unknown, expect: unknown): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expect);
  if (a === e) { pass++; return; }
  failures.push(`${name}\n  期望 ${e}\n  实际 ${a}`);
}

async function t(name: string, src: string, expect: string[], opts: { answers?: string[] } = {}): Promise<void> {
  const r = await run(src, opts);
  eq(name, r.out, expect);
  if (r.err.length) eq(`${name}（不该报错）`, r.err, []);
}

async function rep_(name: string, src: string, expect: string[]): Promise<void> {
  const r = await run(src, {});
  eq(name, r.rep, expect);
}

async function main(): Promise<void> {
  /* ── Math：三角 / 对数 / 整数 / 判断 / 常量 ── */
  await t('三角与反三角', `
print(near(sin(pi() / 2), 1))
print(near(cos(pi()), -1))
print(near(atan2(1, 1), pi() / 4))
print(near(asin(1), pi() / 2))
`, ['true', 'true', 'true', 'true']);
  await t('对数与指数', `print(log2(8))\nprint(log10(1000))\nprint(near(expm1(0), 0))`, ['3', '3', 'true']);
  await t('整数数学', `
print(gcd(12, 18))
print(lcm(4, 6))
print(factorial(5))
print(comb(5, 2))
print(perm(5, 2))
print(isqrt(17))
print(trunc(-2.7))
print(sign(-9))
`, ['6', '12', '120', '10', '20', '4', '-2', '-1']);
  await t('数值判断与常量', `
print(isfinite(1.0))
print(isnan(nan()))
print(isinf(inf()))
print(near(degrees(pi()), 180))
`, ['true', 'true', 'true', 'true']);
  await t('hypot 多维', `print(hypot(3, 4))\nprint(hypot(2, 3, 6))`, ['5', '7']);

  /* ── OS：桌面版能力（浏览器里给明确提示） ── */
  await t('pathjoin 是纯运算哪都能用', `print(pathjoin("a", "b", "c.txt"))`, ['a/b/c.txt']);
  {
    const r = await run(`try\n    makedir("x")\ncatch e\n    print("接住：" + e)\nend\n`);
    eq('浏览器里系统函数给明确提示', r.out.filter((s) => s.startsWith('接住')).length >= 1, true);
  }

  /* ── 两个减号是整行注释（后来又恢复了） ── */
  {
    const r = await run(`-- 这行是注释\n`);
    eq('两个减号是注释', r.err.length === 0, true);
  }

  /* ── alias / #define（类 #define 的文本替换） ── */
  await t('alias 能替换名字', `alias 两次 = 2\nprint(两次)`, ['2']);
  await t('#define 能替换名字', `#define 两次 2\nprint(两次)`, ['2']);
  await t('alias 也能是英文名', `alias PI = 3.14\nprint(PI)`, ['3.14']);
  await t('替换成一整条语句', `alias 打招呼 = print("你好")\n打招呼`, ['你好']);
  await t('替换进函数体', `alias 双倍 = x * 2\nfn f(x)\n    return 双倍\n\nprint(f(21))`, ['42']);
  await t('只换整词', `alias 值 = 1\nint 变量值 = 9\nprint(值, 变量值)`, ['1 9']);
  await t('后面覆盖前面', `alias a = 1\nprint(a)\nalias a = 2\nprint(a)`, ['1', '2']);
  await t('多轮展开', `alias a = b\nalias b = 9\nprint(a)`, ['9']);
  await t('中文换写法仍然兼容', `换 两次 成 2\nprint(两次)`, ['2']);
  {
    const r = await run(`alias a = b\nalias b = a\nprint(a)\n`);
    eq('别名成环会报错', r.err.some((e) => e.includes('环') || e.includes('别名')), true);
  }

  /* ── 调试输出（report=蓝 / warn=黄 / error=红，纯给开发者看，不影响程序） ── */
  {
    const r = await run(`debug report "普通消息"\ndebug warn "注意"\ndebug error "炸了"\nprint("还在跑")\n`);
    eq('调试三档输出', r.dbg, ['[报告] 普通消息', '[警告] 注意', '[报错] 炸了']);
    eq('调试不进普通输出也不中断', r.out, ['还在跑']);
    eq('调试不报错', r.err, []);
  }
  {
    const r = await run(`int a = 7\ndebug(warn, a, "个")\n`);
    eq('调试函数式写法+多参数', r.dbg, ['[警告] 7 个']);
  }
  {
    const r = await run(`调试 报错 "中文写法也认"\n`);
    eq('调试中文写法兼容', r.dbg, ['[报错] 中文写法也认']);
  }

  /* ── 时间 / 文件 / 网络：英文名，桌面版能力 ── */
  {
    const r = await run(`int t = now()\nprint(t > 1700000000000)\nprint(type(now()))\n`);
    eq('now 是毫秒级 int', r.out, ['true', 'int']);
    eq('now 不报错', r.err, []);
  }
  {
    // 浏览器（Node 测试环境）里文件/网络给明确的"桌面版才支持"，不许崩溃或静默
    const r = await run(`try\n    writefile("x.txt", "内容")\ncatch e\n    print("接住：" + e)\nend\n`);
    eq('浏览器里文件函数给明确提示', r.out.filter((s) => s.startsWith('接住')).length >= 1, true);
  }

  /* ── 报告通道 ── */
  await t('报告不进控制台', `报告 "只是报告"\nprint("这句才进控制台")\n`, ['这句才进控制台']);
  await rep_('报告写进报告面板', `报告 "甲", 1\n报告 "乙"\n`, ['甲 1', '乙']);
  await rep_('报告不带参数会列当前变量', `int 金币 = 5\n报告\n`, [
    '当前变量：\n  金币 = 5（int）'
  ]);

  /* ── 表达式 ── */
  await t('表达式优先级', `
int a = 2
int b = 3
int c = a + b * 2
print(c)
print((a + b) * 2)
print(a ** 3)
print(10 % 3)
`, ['8', '10', '8', '1']);

  await t('字符串与转义', `
str s = "第一行\\n第二行"
print(s)
print("a" + 1 + 2)
print(strlen("你好"))
`, ['第一行\n第二行', 'a12', '2']);

  await t('比较与逻辑', `
int a = 5
print(a > 3 && a < 10)
print(a == 5 || false)
print(!(a > 3))
`, ['true', 'true', 'false']);

  /* ── 循环 ── */
  await t('for 用变量控制次数', `
int n = 4
int sum = 0
for i in range(n)
    sum += i
print(sum)
`, ['6']);

  await t('for 循环变量可见', `
for i in range(3)
    print(i)
`, ['0', '1', '2']);

  await t('嵌套 for 各用各的变量', `
for i in range(2)
    for j in range(2)
        print(join(i, "-", j))
`, ['0-0', '0-1', '1-0', '1-1']);

  await t('for i = 1 to 3 含端点', `
for i = 1 to 3
    print(i)
`, ['1', '2', '3']);

  await t('for 遍历列表', `
list xs = [10, 20, 30]
int s = 0
for v in xs
    s += v
print(s)
`, ['60']);

  await t('for 遍历字典键值', `
dict d = {"a": 1, "b": 2}
for k, v in d
    print(join(k, "=", v))
`, ['a=1', 'b=2']);

  await t('while + break / continue', `
int i = 0
while (i < 6)
    i++
    if (i == 2)
        balo
    print(i)
    if (i == 4)
        out
`, ['1', '3', '4']);

  await t('repeat 与 if out', `
int n = 0
repeat(10)
    n++
    if out(n == 3)
print(n)
`, ['3']);

  /* ── 函数 ── */
  await t('函数多参数与返回值', `
function add2(a, b)
    return a + b
print(add2(3, 4))
print(add2(1, 2) * 10)
`, ['7', '30']);

  await t('递归阶乘', `
function fact(n)
    if (n <= 1)
        return 1
    return n * fact(n - 1)
print(fact(5))
`, ['120']);

  await t('参数默认值', `
function greet(name = "世界")
    return join("你好，", name)
print(greet())
print(greet("小明"))
`, ['你好，世界', '你好，小明']);

  await t('函数内局部变量不污染全局', `
int x = 1
function bump()
    int x = 100
    return x
print(bump())
print(x)
`, ['100', '1']);

  /* ── 类 ── */
  await t('类：字段 / init / 方法 / 成员访问', `
class Person
    str name = ""
    int age = 0
    function init(n, a)
        name = n
        age = a
    function greet()
        return join("我是", name, "，", age, "岁")
    function birthday()
        age += 1
        return age

Person p = new Person("小明", 18)
print(p.greet())
print(p.birthday())
print(p.age)
p.age = 30
print(p.age)
`, ['我是小明，18岁', '19', '19', '30']);

  await t('类：方法里调自己的另一个方法', `
class Counter
    int n = 0
    function inc()
        n += 1
    function twice()
        inc()
        inc()
        return n
Counter c = new Counter()
print(c.twice())
`, ['2']);

  await t('类：继承与 super', `
class Animal
    str name = ""
    function init(n)
        name = n
    function speak()
        return "……"
class Dog extends Animal
    function init(n)
        super(n)
    function speak()
        return join(name, "：汪")
Dog d = new Dog("大黄")
print(d.speak())
`, ['大黄：汪']);

  await t('类：对象放进列表再遍历', `
class P
    int hp = 10
    function hurt(d)
        hp -= d
        return hp
list team
add new P() to team
add new P() to team
team[0].hurt(3)
int total = 0
for m in team
    total += m.hp
print(total)
`, ['17']);

  await t('类：字段默认值可以是表达式', `
class Monster
    int hp = 10 * 5
    str tag = join("怪", "-", 1)
Monster m = new Monster()
print(m.hp)
print(m.tag)
`, ['50', '怪-1']);

  /* ── 集合 ── */
  await t('列表指令与下标', `
list xs
add 1 to xs
add 2 to xs
read xs 1
print(__last_read)
len xs
print(__last_len)
xs[0] = 9
print(xs[0])
print(xs[-1])
`, ['2', '2', '9', '2']);

  await t('字典指令与成员', `
dict d
dict d key "小明" = 90
dict d read "小明"
print(__last_read)
print(d["小明"])
print(d.小明)
print(dictin(d, "小红"))
`, ['90', '90', '90', 'false']);

  await t('内建函数在表达式里', `
list xs = [3, 1, 2]
print(len(xs))
print(listin(2, xs))
print(max(1, 9, 4))
print(upper("abc"))
print(split("a,b", ","))
`, ['3', 'true', '9', 'ABC', '["a", "b"]']);

  /* ── 老写法兼容 ── */
  await t('指令行写特殊变量', `
int a = 7
int b = 5
add(a, b)
print(__last_calc)
join("7 + 5 = ", __last_calc)
print(__last_str)
`, ['12', '7 + 5 = 12']);

  await t('if / elif / else（同行与独立行）', `
int n = 5
if (n > 10) {
    print("大")
} elif (n == 5) {
    print("等于 5")
} else {
    print("小")
}
if (n > 1)
    print("大于 1")
else
    print("不大于 1")
`, ['等于 5', '大于 1']);

  await t('import 语义（同文件函数先注册）', `
print(hello())
function hello()
    return "hi"
`, ['hi']);



  /* ── 0.4 新语法：异常 / 方法 / 多返回值（两种执行模式都要过） ── */
  const both = async (name: string, src: string, expect: string[]): Promise<void> => {
    const a = await run(src);
    eq(`${name}（编译）`, a.out, expect);
    eq(`${name}（编译不该报错）`, a.err, []);
    const b = await run(src, { mode: 'debug' });
    eq(`${name}（解释）`, b.out, expect);
    eq(`${name}（解释不该报错）`, b.err, []);
  };

  /* ── 注释两种写法 ── */
  await both('两个减号是整行注释', `
-- 这是注释
int a = 1
-- 又一行注释
print(a)
`, ['1']);
  await both('缩进的注释也算注释', `
if (true)
    -- 块里的注释
    print("ok")
`, ['ok']);
  await both('后缀自减照旧', `
int i = 3
i--
print(i)
`, ['2']);
  await both('行尾注释用两个减号', `
int b = 2 -- 行尾说明
print(b)
print("http://a.b")
`, ['2', 'http://a.b']);
  await both('两个斜杠是整除', `
print(7 // 2)
print(-7 // 2)
print(7.5 // 2)
`, ['3', '-4', '3']);

  /* ── 三元表达式与整除 ── */
  await both('三元基础', `
int a = 3
print(a > 0 ? "正" : "负")
print(a > 10 ? "大" : "小")
`, ['正', '小']);
  await both('三元右结合（嵌套）', `
int n = 0
print(n < 0 ? "负" : n == 0 ? "零" : "正")
`, ['零']);
  await both('三元只算选中的那一支', `
fn 炸()
    throw "不该被求值"
    return 1

print(true ? "安全" : 炸())
`, ['安全']);
  await both('整除向下取整', `
print(idiv(7, 2))
print(idiv(-7, 2))
print(idiv(7.5, 2))
`, ['3', '-4', '3']);

  await both('try / catch / finally', `
int done = 0
try
    throw "出错了"
    print("不会执行")
catch e
    print("抓到：" + e)
finally
    done = 1
print(done)
`, ['抓到：出错了', '1']);

  await both('错误能带行号', `
try
    int x = 1 - "a"
catch e
    print(e.line > 0)
    print(e.行 > 0)
    print(len(e.消息) > 0)
`, ['true', 'true', 'true']);

  await both('函数里的错误能被上层捕获', `
function risky(n)
    if (n > 0)
        throw "太大了"
    return n
try
    risky(5)
catch err
    print("捕获：" + err)
print("继续跑")
`, ['捕获：太大了', '继续跑']);

  await both('字符串方法链', `
str s = "  Float Script  "
print(s.trim().upper())
print("a,b,c".split(",").len())
print("abc".replace("b", "B"))
print("hello".find("ll"))
print("x".repeat(3))
`, ['FLOAT SCRIPT', '3', 'aBc', '2', 'xxx']);

  await both('列表方法', `
list xs = [3, 1, 2]
xs.push(4)
print(xs.len())
xs.sort()
print(xs.join("-"))
print(xs.pop())
xs.reverse()
print(xs)
print(xs.slice(0, 2))
print(xs.contains(2))
`, ['4', '1-2-3-4', '4', '[3, 2, 1]', '[3, 2]', 'true']);

  await both('字典方法', `
dict d = {"a": 1, "b": 2}
print(d.keys())
print(d.values())
print(d.has("a"))
print(d.get("z", 0))
`, ['["a", "b"]', '[1, 2]', 'true', '0']);

  await both('多返回值与解构', `
function minmax(xs)
    int lo = xs[0]
    int hi = xs[0]
    for v in xs
        if (v < lo)
            lo = v
        if (v > hi)
            hi = v
    return lo, hi

list data = [5, 3, 9, 1]
int low, int high = minmax(data)
print(low)
print(high)
`, ['1', '9']);


  /* ── 0.6：字符串插值与静态检查 ── */
  await both('字符串插值', `
int score = 95
str name = "小明"
list xs = [1, 2]
print("得分：{score}，名字：{name}")
print("下一名：{score + 1}")
print("列表长度：{len(xs)}")
print("转义：\\{不是插值}")
`, ['得分：95，名字：小明', '下一名：96', '列表长度：2', '转义：{不是插值}']);

  await both('插值里能放函数调用与方法', `
function tag(n)
    return "T" + n
print("标签：{tag(7)} / {\"ab\".upper()}")
`, ['标签：T7 / AB']);

  {
    const a = analyze('main.cw', `int score = 1
print(scor)
`);
    eq('未定义变量会提示', a.diagnostics.some((d) => d.code === 'undefined-var'), true);

    const b = analyze('main.cw', `int a = 1
print(b)
`);
    const d = b.diagnostics.find((x) => x.code === 'undefined-var');
    eq('提示里给出近似名字', d?.message.includes('a') ?? false, true);

    const c = analyze('main.cw', 'function f()\n    return 1\nprint(f())\n');
    eq('函数调用不误报', c.diagnostics.filter((x) => x.code === 'undefined-fn').length, 0);

    const e = analyze('main.cw', 'class A\n    int hp = 1\n    function m()\n        return hp\n');
    eq('类方法里引用字段不误报', e.diagnostics.filter((x) => x.code === 'undefined-var').length, 0);
  }

  /* ── 关键字与变量名、教程示例 ── */
  await t('out 当变量名也能用', `
str out = ""
for i in range(3)
    out += "x"
print(out)
`, ['xxx']);

  await t('教程里的战斗示例能跑', `
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
        int damage = atk + 2
        target.hurt(damage)
        return name + " 打中 " + target.name

class Boss extends Fighter
    function attack(target)
        int damage = atk + 8
        target.hurt(damage)
        return name + " 暴怒一击"

Fighter hero = new Fighter("勇者", 90, 14)
Boss boss = new Boss("石巨人", 40, 12)
int round = 0
while (hero.alive() && boss.alive())
    round++
    print(hero.attack(boss))
    if (boss.alive())
        print(boss.attack(hero))
print("回合：" + round)
`, ['勇者 打中 石巨人', '石巨人 暴怒一击', '勇者 打中 石巨人', '石巨人 暴怒一击', '勇者 打中 石巨人', '回合：3']);

  /* ── 老示例脚本仍能跑（向后兼容） ── */
  {
    const { readFileSync } = await import('node:fs');
    const names = ['01_hello', '02_control', '03_collections', '04_functions', '05_class'];
    for (const n of names) {
      const src = readFileSync(new URL(`../../language/samples/${n}.co`, import.meta.url), 'utf8');
      const r = await run(src, { answers: ['小明'] });
      eq(`示例 ${n} 无报错`, r.err, []);
      eq(`示例 ${n} 有输出`, r.out.length > 0, true);
    }
  }

  /* ── 新建工程模板也要能直接跑 ── */
  {
    const { TEMPLATES } = await import('../src/core/templates');
    for (const tpl of TEMPLATES) {
      const r = await run(tpl.files['main.co'], { answers: ['小明'] });
      eq(`模板「${tpl.name}」无报错`, r.err, []);
      eq(`模板「${tpl.name}」有输出`, r.out.length > 0, true);
    }
  }

  /* ── 调试：停在第一条、单步、断点 ── */
  {
    const src = 'int a = 1\nprint(a)\nprint(a + 1)\n';
    const d1 = await debugTrace(src, ['step', 'step', 'step']);
    eq('调试停在第一条语句', d1.lines.slice(0, 1), [1]);
    eq('调试单步逐行推进', d1.lines.slice(0, 3), [1, 2, 3]);

    const d2 = await debugTrace(src, ['continue'], { breakpoints: [2] });
    eq('断点停在指定行（先停首条再停断点）', d2.lines, [1, 2]);

    const d3 = await debugTrace('function f(x)\n    return x * 2\nprint(f(3))\n', ['into', 'into', 'continue']);
    eq('单步进入函数体内', d3.lines.includes(2), true);
  }

  /* ── 诊断 ── */
  const bad = parseProgram('main.cw', 'for i in range(\n');
  eq('语法错误有诊断', bad.diagnostics.length > 0, true);

  const bad2 = parseProgram('main.cw', 'class A\n    int x = 1\nclass B extends Missing\n    int y = 2\n');
  eq('正常解析 class', bad2.diagnostics.filter((d) => d.severity === 'error').length, 0);

  const err = await run(`
class A
    int x = 1
A a = new A()
print(a.nope)
`);
  eq('访问不存在字段报错', err.err.length > 0, true);

  const err2 = await run('int x = 1 - "a"');
  eq('类型错误有提示', err2.err[0]?.includes('需要数字'), true);


  /* ── 健壮性回归（这几个以前会让 IDE 崩，现在必须优雅处理）── */
  const bare = await run('int n = 7\nprint n\n');
  eq('print 允许不带括号', bare.out[0], '7');

  const bareMul = await run('list a = [1, 2]\nprint a\n');
  eq('print 裸写列表', bareMul.out[0], '[1, 2]');

  const cyc = await run('list a = [1]\nadd a to a\nprint(a)\n');
  eq('自引用列表不崩', cyc.err.length, 0);

  const cyc2 = await run('list a = [1]\nadd a to a\nlist b = a\nprint("ok")\n');
  eq('自引用列表可拷贝', cyc2.out[0], 'ok');

  const deep = await run('function f(n)\n    return f(n + 1)\n\nprint(f(1))\n');
  eq('无限递归给出人话提示', deep.err[0]?.includes('递归太深'), true);

  const noCond = await run('if\n    print(1)\n');
  eq('if 缺条件会被指出', noCond.err.length > 0, true);

  const topRet = await run('return 1\n');
  eq('顶层 return 会被指出', topRet.err.length > 0, true);

  const longSrc = (() => {
    let t = '';
    for (let i = 0; i < 400; i++) t += `function fn${i}(a)\n    return a + ${i}\n`;
    for (let i = 0; i < 1500; i++) t += `int v${i} = ${i}\n`;
    t += 'print(fn399(1))\nprint(v1499)\n';
    return t;
  })();
  const long = await run(longSrc);
  eq('长代码（400 函数 + 1500 变量）执行正确', long.out[0] === '400' && long.out[1] === '1499', true);

  /* ── 汇编式算术（add / sub / mul / div / mod，空格分隔，任意多个数）── */
  await t('空变量：只声明不赋值，按类型给中性初值', 'int c\nprint c', ['0']);
  await t('空变量能直接累加', 'int 总数\nfor i in range(4)\n    总数 += i\nprint 总数', ['6']);
  await t('汇编式 add', 'int c\nint a = 2\nint b = 3\nc = add a b\nprint c', ['5']);
  await t('汇编式 多操作数左结合', 'print add 1 2 3 4', ['10']);
  await t('汇编式 sub / mul / div / mod',
    'print sub 10 3\nprint mul 2 3 4\nprint div 100 5 2\nprint mod 10 3',
    ['7', '24', '10', '1']);
  await t('汇编式和运算符混写', 'int a = 2\nint b = 3\nprint add a b + 10', ['15']);
  await t('带括号的老写法也还在', 'print add(2, 3)', ['5']);

  /* ── Copper 改名：.co 主扩展名 + .cw 旧扩展名兼容 ── */
  {
    const { Runner } = await import('../src/lang/interp');
    const runFiles = (sources: Map<string, string>, entry: string): Promise<{ out: string[]; err: string | null }> =>
      new Promise((done) => {
        (globalThis as any).window = (globalThis as any).window ?? {};
        const host = (globalThis as any).window;
        host.__floatSources = sources;
        const out: string[] = [];
        let err: string | null = null;
        let settled = false;
        const finish = (): void => { if (!settled) { settled = true; done({ out, err }); } };
        const r = new Runner(entry, {
          output: (l) => { if (l.kind === 'out') out.push(l.text); else if (l.kind === 'err') err = l.text; },
          onError: (e) => { err = e.message; },
          onDone: finish,
          onState: () => {},
          onPaused: () => {}
        });
        try { r.start('run'); } catch (e) { err = String(e); finish(); }
        setTimeout(finish, 20000);
      });
    // 1) 新扩展名 .co：显式带后缀
    {
      const sources = new Map<string, string>([
        ['main.co', 'import "lib.co"\nprint greet("Copper")'],
        ['lib.co', 'function greet(who)\n    return "你好，" + who']
      ]);
      const r = await runFiles(sources, 'main.co');
      eq('.co 显式 import 能跑', r.out, ['你好，Copper']);
      eq('.co 显式 import 无报错', r.err, null);
    }
    // 2) 省略后缀时优先 .co
    {
      const sources = new Map<string, string>([
        ['main.co', 'import "lib"\nprint twice(11)'],
        ['lib.co', 'function twice(x)\n    return x * 2'],
        ['lib.cw', 'function twice(x)\n    return " WRONG "']
      ]);
      const r = await runFiles(sources, 'main.co');
      eq('省略后缀优先认 .co', r.out, ['22']);
    }
    // 3) 旧 .cw 工程原样能跑（用户现存工程不搬家）
    {
      const sources = new Map<string, string>([
        ['main.cw', 'import "lib.cw"\nprint greet("旧工程")'],
        ['lib.cw', 'function greet(who)\n    return "还在跑：" + who']
      ]);
      const r = await runFiles(sources, 'main.cw');
      eq('.cw 旧工程能跑', r.out, ['还在跑：旧工程']);
      eq('.cw 旧工程无报错', r.err, null);
    }
  }

  /* ── 补全用的符号表：变量必须能被收集到（以前只有写了类型关键字的才算） ── */
  {
    const src = [
      'fang = 1',
      'list xs',
      'dict d',
      'x = "hi"',
      'fn 打招呼(name)',
      '    return name',
      'try',
      '    throw "e"',
      'catch err',
      '    print err',
      'for i in range(3)',
      '    print i'
    ].join('\n');
    const a = analyze('main.cw', src);
    const names = a.vars.map((v) => v.name);
    for (const want of ['fang', 'xs', 'd', 'x', 'err', 'i', 'name']) {
      eq(`符号表里有 ${want}`, names.includes(want), true);
    }
    eq('符号表里没有重复的变量', names.length, new Set(names).size);
    eq('类型推断：list / dict / str / int', [
      a.vars.find((v) => v.name === 'xs')?.type,
      a.vars.find((v) => v.name === 'd')?.type,
      a.vars.find((v) => v.name === 'x')?.type,
      a.vars.find((v) => v.name === 'fang')?.type
    ], ['list', 'dict', 'str', 'int']);
    eq('函数进了符号表', a.funcs.map((f) => f.name), ['打招呼']);
  }

  /* ── 彩蛋：第 27 行报错，吉祥物廿七（三脚橘猫）瘸在这行 ── */
  {
    const pad = (n: number): string => Array.from({ length: n }, (_, i) => `print ${i + 1}`).join('\n');
    const at27 = await run(pad(26) + '\nprint 1 / 0');
    eq('第 27 行报错带「喵～」彩蛋', String(at27.err[0] ?? '').startsWith('喵～第 27 行瘸了…'), true);
    const at26 = await run(pad(25) + '\nprint 1 / 0');
    eq('第 26 行不触发彩蛋', String(at26.err[0] ?? '').startsWith('出错'), true);
    const at28 = await run(pad(27) + '\nprint 1 / 0');
    eq('第 28 行不触发彩蛋', String(at28.err[0] ?? '').startsWith('出错'), true);
    // 彩蛋里带着原因，不是只有一句喵
    eq('彩蛋后面跟真实原因', String(at27.err[0] ?? '').includes('除数不能为 0'), true);
  }

  /* ── 运行时错误行号必须准（编译通道曾经把运算符错误一律报成第 1 行） ── */
  {
    const r2 = await run('print 1\nprint 2 / 0');
    eq('编译通道：第 2 行的错报第 2 行', String(r2.err[0] ?? '').includes('第 2 行'), true);
    const r3 = await run('print 1\nprint 2\nthrow "坏了"');
    eq('编译通道：throw 报所在行', String(r3.err[0] ?? '').includes('第 3 行'), true);
  }

  /* ── 结果 ── */
  console.log(`\n通过 ${pass} 项，失败 ${failures.length} 项`);
  if (failures.length) {
    for (const f of failures) console.log('✗ ' + f);
    process.exitCode = 1;
  } else {
    console.log('全部通过');
  }
}

void main();
