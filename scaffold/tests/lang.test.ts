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

async function main(): Promise<void> {
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

  await both('try / catch / finally', `
int done = 0
try
    throw "出错了"
    print("不会执行")
catch e
    print("抓到：" + e.message)
finally
    done = 1
print(done)
`, ['抓到：出错了', '1']);

  await both('错误能带行号', `
try
    int x = 1 - "a"
catch e
    print(e.line > 0)
`, ['true']);

  await both('函数里的错误能被上层捕获', `
function risky(n)
    if (n > 0)
        throw "太大了"
    return n
try
    risky(5)
catch err
    print("捕获：" + err.message)
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
      const src = readFileSync(new URL(`../../language/samples/${n}.cw`, import.meta.url), 'utf8');
      const r = await run(src, { answers: ['小明'] });
      eq(`示例 ${n} 无报错`, r.err, []);
      eq(`示例 ${n} 有输出`, r.out.length > 0, true);
    }
  }

  /* ── 新建工程模板也要能直接跑 ── */
  {
    const { TEMPLATES } = await import('../src/core/templates');
    for (const tpl of TEMPLATES) {
      const r = await run(tpl.files['main.cw'], { answers: ['小明'] });
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
