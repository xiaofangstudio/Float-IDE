/* Float Script 教程内容。
 *
 * 结构化的数据而不是一坨 Markdown——这样应用里能渲染成带目录的页面、
 * 代码块能一键「在编辑器中打开」和「直接运行」，同一份内容还能导出去。
 *
 * 写作原则：每节只讲一件事；代码必须先能跑通；输出示例必须是真的输出。
 */

export type CodeSample = {
  /** 一段可运行（或可独立阅读）的代码 */
  code: string;
  /** 真实输出；没有就不显示 */
  out?: string;
  /** 这段在讲什么 */
  note?: string;
  /** 需要用户先填输入才有输出 */
  ask?: boolean;
};

export type Section = {
  id: string;
  title: string;
  /** 一句话导语 */
  lead?: string;
  paras?: string[];
  codes?: CodeSample[];
  /** 小提示 / 坑 */
  tips?: { kind: 'tip' | 'warn' | 'note'; text: string }[];
};

export type Chapter = {
  id: string;
  title: string;
  /** 章节图标字符 */
  icon: string;
  summary: string;
  sections: Section[];
};

export const CHAPTERS: Chapter[] = [
  /* ───────────────────────── 1 ───────────────────────── */
  {
    id: 'start',
    title: '开始之前',
    icon: '▶',
    summary: 'Float Script 是什么、怎么跑起来、和 Python 有什么关系',
    sections: [
      {
        id: 'what',
        title: '这是什么语言',
        lead: 'Float Script（CW Script 系）是一门行式脚本语言：一行一句、缩进成块、中英文都认。',
        paras: [
          '它有 Python 的读感，但走的是「先翻译成 JS 再跑」的路线：按 F5 时，整份 AST 会被编译成 JavaScript 交给 V8 执行。所以循环、递归、字符串拼接这些热路径，实测比 CPython 快 5～9 倍。',
          '同时它保留了「开箱即用」：没有编译等待，按下就跑；随时可以切到解释执行（F6）打断点、单步、看变量。'
        ],
        tips: [
          { kind: 'note', text: '两种执行方式的分工：F5 = 编译执行（快，日常用）；F6 = 解释执行（慢一点，但能断点调试）。同一份代码两边结果一致。' }
        ]
      },
      {
        id: 'first',
        title: '第一个程序',
        paras: ['创建工程后，入口文件 main.cw 里写：'],
        codes: [{
          code: 'print("你好，Float Script！")',
          out: '你好，Float Script！'
        }],
        tips: [
          { kind: 'tip', text: 'print 的括号可以省略：print "你好" 也能跑。多个值用逗号隔开，输出时用空格连接。' }
        ]
      },
      {
        id: 'keyword',
        title: '中英文关键字都认',
        lead: '同一段逻辑，可以全用英文写，也可以全用中文写。',
        paras: ['这个设计是为了照顾不同习惯：从 Python 过来的用英文，习惯「代码世界」原版的用中文。两种写法能混用，但一个文件里最好统一。'],
        codes: [{
          code: '// 英文\nint n = 3\nfor i in range(n)\n    print(i)\n\n// 中文\n整数 m = 3\n循环 j 从 0 到 m - 1\n    打印(j)',
          out: '0\n1\n2\n0\n1\n2'
        }],
        tips: [
          { kind: 'warn', text: '别在同一段里一半英文一半中文，读起来会很难受。建议整个工程统一。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 2 ───────────────────────── */
  {
    id: 'types',
    title: '变量与类型',
    icon: '◈',
    summary: 'int / float / str / bool / list / dict，以及命名与作用域',
    sections: [
      {
        id: 'declare',
        title: '声明变量',
        paras: [
          '可以带类型写，也可以不带。带类型时编译器知道它是什么，补全和检查更准；不带也可以，运行时按值推断。'
        ],
        codes: [{
          code: 'int age = 20\nfloat pi = 3.14159\nstr name = "小方"\nbool ok = true\n\n// 也可以不写类型\nscore = 95\nprint(age)\nprint(name)\nprint(ok)\nprint(score)',
          out: '20\n小方\ntrue\n95'
        }]
      },
      {
        id: 'types-table',
        title: '六种基本类型',
        paras: ['这就是全部的内置类型，没有更多了——刻意保持小。'],
        codes: [{
          code: 'print(type(1))          // int\nprint(type(1.5))        // float\nprint(type("abc"))      // str\nprint(type(true))       // bool\nprint(type([1, 2]))     // list\nprint(type({"k": 1}))   // dict',
          out: 'int\nfloat\nstr\nbool\nlist\ndict'
        }],
        tips: [
          { kind: 'note', text: '整数和浮点是分开的类型：1 是 int，1.0 是 float。除法 7 / 2 得 3.5（不像 Python 3 之前那样截断）。' }
        ]
      },
      {
        id: 'naming',
        title: '命名规则',
        paras: [
          '变量名可以用中英文、数字、下划线，但不能以数字开头。中文名是合法的，写「总数」「平均分」都行。'
        ],
        codes: [{
          code: 'int 总分 = 270\nfloat avg = 总分 / 3.0\nprint(avg)',
          out: '90'
        }],
        tips: [
          { kind: 'warn', text: '不要用关键字当变量名：if / for / class / function / return / print / import 这些都不行。out、break、balo 单独成句时是跳转指令，当变量用要小心。' }
        ]
      },
      {
        id: 'scope',
        title: '作用域',
        paras: [
          '函数内部定义的变量只活在函数里；顶层定义的变量全局可见。函数体里可以读到全局变量，但想改动它要用函数返回值或者在顶层改。',
          '类的方法里，self.xxx 是实例字段，裸名 xxx 也能读到同名字段。'
        ],
        codes: [{
          code: 'int total = 100\n\nfunction show()\n    print(total)   // 读得到全局\n\nshow()',
          out: '100'
        }]
      }
    ]
  },

  /* ───────────────────────── 3 ───────────────────────── */
  {
    id: 'expr',
    title: '运算符与表达式',
    icon: '±',
    summary: '算术、比较、逻辑、字符串插值',
    sections: [
      {
        id: 'arith',
        title: '算术运算',
        codes: [{
          code: 'print(7 + 3)\nprint(7 - 3)\nprint(7 * 3)\nprint(7 / 2)\nprint(7 % 3)\nprint(2 ** 10)\nprint(-5)',
          out: '10\n4\n21\n3.5\n1\n1024\n-5'
        }],
        tips: [
          { kind: 'note', text: '** 是乘方。7 / 2 得 3.5，不是 3 —— 想要整除用 floor(7 / 2)。' }
        ]
      },
      {
        id: 'assign',
        title: '自增与复合赋值',
        codes: [{
          code: 'int n = 10\nn++\nprint(n)\nn--\nprint(n)\nn += 5\nprint(n)\nn *= 2\nprint(n)\nn -= 6\nprint(n)',
          out: '11\n10\n15\n30\n24'
        }]
      },
      {
        id: 'compare',
        title: '比较与逻辑',
        paras: ['比较返回 bool。逻辑用 && || !，也可以用 and or not。'],
        codes: [{
          code: 'print(3 > 2)\nprint(3 == 2)\nprint(3 != 2)\nprint(true && false)\nprint(true || false)\nprint(!true)',
          out: 'true\nfalse\ntrue\nfalse\ntrue\nfalse'
        }],
        tips: [
          { kind: 'warn', text: '判断相等用 ==，单个 = 是赋值。写成 if (x = 1) 会被语法检查拦住。' }
        ]
      },
      {
        id: 'interp',
        title: '字符串插值',
        lead: '在字符串里用 {表达式} 直接塞值，比一堆加号清爽得多。',
        codes: [{
          code: 'str name = "阿方"\nint hp = 87\nprint("角色 {name} 剩余血量 {hp}")\nprint("下回合会掉到 {hp - 20}")\nprint("平均分 {270 / 3}")',
          out: '角色 阿方 剩余血量 87\n下回合会掉到 67\n平均分 90'
        }],
        tips: [
          { kind: 'tip', text: '花括号里能放任何表达式：变量、运算、函数调用、方法调用都行。想输出真的花括号就写 \\{ 和 \\}。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 4 ───────────────────────── */
  {
    id: 'string',
    title: '字符串',
    icon: '“',
    summary: '拼接、切片、常用方法、类型转换',
    sections: [
      {
        id: 'concat',
        title: '拼接与长度',
        codes: [{
          code: 'str a = "Float"\nstr b = "IDE"\nprint(a + " " + b)\nprint(len(a))\nprint(join(a, " ", b, " v0.8"))',
          out: 'Float IDE\n5\nFloat IDE v0.8'
        }],
        tips: [
          { kind: 'tip', text: 'join(...) 能把数字直接拼进来，不用先转字符串：join("得分 ", 95)。' }
        ]
      },
      {
        id: 'slice',
        title: '下标与切片',
        codes: [{
          code: 'str s = "abcdef"\nprint(s[0])\nprint(s[-1])\nprint(s[1:4])\nprint(s[:3])\nprint(s[3:])',
          out: 'a\nf\nbcd\nabc\ndef'
        }],
        tips: [
          { kind: 'note', text: '负数下标从末尾算：-1 是最后一个字符。切片是「左闭右开」，s[1:4] 取的是 1、2、3 三个位置。' }
        ]
      },
      {
        id: 'methods',
        title: '常用方法',
        paras: ['字符串方法可以直接链式调用。'],
        codes: [{
          code: 'str s = "  Hello World  "\nprint(s.trim())\nprint(s.trim().upper())\nprint(s.trim().lower())\nprint(s.trim().replace("World", "Float"))\nprint(s.trim().split(" "))\nprint(s.contains("World"))\nprint(s.trim().starts_with("Hell"))\nprint(s.repeat(2).trim())',
          out: 'Hello World\nHELLO WORLD\nhello world\nHello Float\n["Hello", "World"]\ntrue\ntrue\nHello WorldHello World'
        }]
      },
      {
        id: 'convert',
        title: '类型转换',
        codes: [{
          code: 'print("123".to_int() + 1)\nprint("3.14".to_float() * 2)\nprint(str(42) + "!")\nprint(int("7"))\nprint(float("2.5"))',
          out: '124\n6.28\n42!\n7\n2.5'
        }],
        tips: [
          { kind: 'warn', text: '转不动的转换会抛错，不是给你 0。比如 int("abc") 会报错，拿不准时先用 try/catch 包住。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 5 ───────────────────────── */
  {
    id: 'control',
    title: '流程控制',
    icon: '⑂',
    summary: 'if / for / while / repeat，以及 break 与 continue',
    sections: [
      {
        id: 'if',
        title: '条件分支',
        paras: ['条件可以带括号也可以不带，缩进决定块的归属。'],
        codes: [{
          code: 'int score = 87\nif (score >= 90)\n    print("优秀")\nelif (score >= 80)\n    print("良好")\nelif (score >= 60)\n    print("及格")\nelse\n    print("不及格")',
          out: '良好'
        }],
        tips: [
          { kind: 'warn', text: 'if 后面必须跟条件。漏了条件会直接报错——不会默默跳过整段。' }
        ]
      },
      {
        id: 'for',
        title: '计数循环',
        lead: 'range 的终点可以是任意表达式，这是变量能在循环里用的关键。',
        codes: [{
          code: 'int n = 3\nfor i in range(n)\n    print(i)\n\nprint("---")\n\nfor i in range(2, 6)\n    print(i)\n\nprint("---")\n\nfor i in range(0, 10, 3)\n    print(i)',
          out: '0\n1\n2\n---\n2\n3\n4\n5\n---\n0\n3\n6\n9'
        }],
        tips: [
          { kind: 'note', text: 'range(n) 是 0 到 n-1；range(a, b) 是 a 到 b-1；range(a, b, step) 带步长。循环变量名随便起。' }
        ]
      },
      {
        id: 'forin',
        title: '遍历集合',
        codes: [{
          code: 'list xs = [10, 20, 30]\nfor x in xs\n    print(x)\n\nprint("---")\n\nstr s = "abc"\nfor ch in s\n    print(ch)\n\nprint("---")\n\ndict d = {"a": 1, "b": 2}\nfor k, v in d\n    print(join(k, "=", v))',
          out: '10\n20\n30\n---\na\nb\nc\n---\na=1\nb=2'
        }]
      },
      {
        id: 'while',
        title: 'while 与跳出',
        paras: ['跳出整个循环用 out 或 break；跳过本轮用 balo 或 continue。'],
        codes: [{
          code: 'int i = 0\nwhile (i < 6)\n    i++\n    if (i == 2)\n        continue\n    if (i == 5)\n        break\n    print(i)',
          out: '1\n3\n4'
        }],
        tips: [
          { kind: 'warn', text: 'while 的条件一定要能被改变，否则就是死循环。写错了也不怕：引擎在 400 层递归/超长时间后会主动停下并告诉你哪里可能不对，而不是把界面卡死。' }
        ]
      },
      {
        id: 'repeat',
        title: 'repeat 与 forever',
        codes: [{
          code: 'repeat(3)\n    print("滴答")\n\n// forever 是无限循环，靠 out 跳出\nint n = 0\nforever\n    n++\n    if (n >= 3)\n        out\nprint(n)',
          out: '滴答\n滴答\n滴答\n3'
        }]
      }
    ]
  },

  /* ───────────────────────── 6 ───────────────────────── */
  {
    id: 'list',
    title: '列表',
    icon: '▤',
    summary: '增删改查、排序、切片、嵌套',
    sections: [
      {
        id: 'list-basic',
        title: '创建与访问',
        codes: [{
          code: 'list xs = [3, 1, 2]\nprint(xs)\nprint(len(xs))\nprint(xs[0])\nprint(xs[-1])\n\n// 也可以先声明再一个个加\nlist ys\nadd 10 to ys\nadd 20 to ys\nprint(ys)',
          out: '[3, 1, 2]\n3\n3\n2\n[10, 20]'
        }]
      },
      {
        id: 'list-mod',
        title: '增删改',
        codes: [{
          code: 'list xs = [1, 2, 3]\nxs.push(4)\nprint(xs)\nxs.insert(0, 0)\nprint(xs)\nxs.remove(2)\nprint(xs)\nxs[0] = 99\nprint(xs)\nprint(xs.index_of(3))',
          out: '[1, 2, 3, 4]\n[0, 1, 2, 3, 4]\n[0, 1, 3, 4]\n[99, 1, 3, 4]\n2'
        }],
        tips: [
          { kind: 'tip', text: 'add 值 to 列表 是老写法，效果等同 push。两种都能用。' }
        ]
      },
      {
        id: 'list-algo',
        title: '排序、反转、求和',
        codes: [{
          code: 'list xs = [3, 1, 4, 1, 5]\nprint(xs.sort())\nprint(xs.reverse())\nprint(xs.sum())\nprint(xs.join(" / "))',
          out: '[1, 1, 3, 4, 5]\n[5, 4, 3, 1, 1]\n14\n5 / 4 / 3 / 1 / 1'
        }],
        tips: [
          { kind: 'note', text: 'sort() 是原地排序并返回自己，所以能接着链式调用。字符串排序按字典序。' }
        ]
      },
      {
        id: 'list-slice',
        title: '切片与嵌套',
        codes: [{
          code: 'list xs = [10, 20, 30, 40, 50]\nprint(xs.slice(1, 3))\nprint(xs.slice(2))\n\n// 列表里可以放列表\nlist grid = [[1, 2], [3, 4]]\nprint(grid[1][0])\nprint(grid)',
          out: '[20, 30]\n[30, 40, 50]\n3\n[[1, 2], [3, 4]]'
        }],
        tips: [
          { kind: 'warn', text: '列表是「按值」赋值的：list b = a 会复制一份，改 b 不影响 a。这一点和 Python 不一样，注意。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 7 ───────────────────────── */
  {
    id: 'dict',
    title: '字典',
    icon: '◫',
    summary: '键值对、遍历、嵌套、与列表配合',
    sections: [
      {
        id: 'dict-basic',
        title: '创建与读写',
        codes: [{
          code: 'dict scores = {"小明": 90, "小红": 95}\nprint(scores["小明"])\nscores["小刚"] = 88\nprint(scores)\nprint(len(scores))\nprint(scores.has("小红"))',
          out: '90\n{"小明": 90, "小红": 95, "小刚": 88}\n3\ntrue'
        }]
      },
      {
        id: 'dict-iter',
        title: '遍历与取值',
        codes: [{
          code: 'dict d = {"a": 1, "b": 2, "c": 3}\nfor k, v in d\n    print(join(k, " → ", v))\n\nprint(d.keys())\nprint(d.values())\nprint(d.get("z", 0))',
          out: 'a → 1\nb → 2\nc → 3\n["a", "b", "c"]\n[1, 2, 3]\n0'
        }],
        tips: [
          { kind: 'warn', text: '直接取不存在的键会报错。不确定有没有时用 get(键, 默认值)。' }
        ]
      },
      {
        id: 'dict-nest',
        title: '嵌套与复杂结构',
        paras: ['字典里可以放列表，列表里也可以放字典——这就是"数据建模"的全部工具。'],
        codes: [{
          code: 'dict player = {\n    "name": "阿方",\n    "hp": 100,\n    "bag": ["剑", "药"]\n}\nprint(player["name"])\nprint(player["bag"][0])\n\nlist team = [\n    {"name": "A", "lv": 3},\n    {"name": "B", "lv": 5}\n]\nfor m in team\n    print(join(m["name"], " Lv.", m["lv"]))',
          out: '阿方\n剑\nA Lv.3\nB Lv.5'
        }]
      }
    ]
  },

  /* ───────────────────────── 8 ───────────────────────── */
  {
    id: 'func',
    title: '函数',
    icon: 'ƒ',
    summary: '定义、参数、默认值、返回值、多返回值、递归',
    sections: [
      {
        id: 'func-def',
        title: '定义与调用',
        codes: [{
          code: 'function add(a, b)\n    return a + b\n\nprint(add(2, 3))\nprint(add("x", "y"))',
          out: '5\nxy'
        }],
        tips: [
          { kind: 'note', text: '顶层函数可以先用后定义——整个文件的函数在运行前就已经登记好了。' }
        ]
      },
      {
        id: 'func-default',
        title: '默认参数',
        codes: [{
          code: 'function greet(name, greeting = "你好")\n    return join(greeting, "，", name)\n\nprint(greet("阿方"))\nprint(greet("阿方", "早上好"))',
          out: '你好，阿方\n早上好，阿方'
        }]
      },
      {
        id: 'func-multi',
        title: '多返回值',
        lead: 'return a, b 会打包成列表返回，接收时可以直接拆开。',
        codes: [{
          code: 'function minmax(xs)\n    xs.sort()\n    return xs[0], xs[-1]\n\nint lo, int hi = minmax([5, 3, 9, 1])\nprint("最小 " + lo + "，最大 " + hi)',
          out: '最小 1，最大 9'
        }]
      },
      {
        id: 'func-recursion',
        title: '递归',
        codes: [{
          code: 'function fact(n)\n    if (n <= 1)\n        return 1\n    return n * fact(n - 1)\n\nprint(fact(5))\nprint(fact(10))',
          out: '120\n3628800'
        }],
        tips: [
          { kind: 'warn', text: '递归必须写结束条件。忘了写也不会崩：引擎在 400 层时会停下并告诉你「多半是 f() 的结束条件没写对」。' }
        ]
      },
      {
        id: 'func-higher',
        title: '函数作为值',
        paras: ['函数可以存进变量、传参、放进列表——这让「策略」可以当数据用。'],
        codes: [{
          code: 'function twice(x)\n    return x * 2\n\nfunction apply(f, v)\n    return f(v)\n\nprint(apply(twice, 21))\n\nlist ops = [twice]\nprint(ops[0](5))',
          out: '42\n10'
        }]
      }
    ]
  },

  /* ───────────────────────── 9 ───────────────────────── */
  {
    id: 'class',
    title: '类与对象',
    icon: '◧',
    summary: '字段、方法、init、继承、super、方法链',
    sections: [
      {
        id: 'class-def',
        title: '定义一个类',
        codes: [{
          code: 'class Hero\n    str name = ""\n    int hp = 100\n\n    function init(name, hp)\n        self.name = name\n        self.hp = hp\n\n    function info()\n        return name + " 剩余 " + hp\n\nHero h = new Hero("阿方", 120)\nprint(h.info())\nprint(h.hp)',
          out: '阿方 剩余 120\n120'
        }],
        tips: [
          { kind: 'note', text: 'init 是构造方法，new 的时候自动调用。方法里 self.name 和裸名 name 都能访问字段。' }
        ]
      },
      {
        id: 'class-mod',
        title: '修改状态',
        codes: [{
          code: 'class Hero\n    str name = ""\n    int hp = 100\n\n    function init(name, hp)\n        self.name = name\n        self.hp = hp\n\n    function hurt(d)\n        hp -= d\n        if (hp < 0)\n            hp = 0\n        return hp\n\nHero h = new Hero("阿方", 100)\nh.hurt(30)\nprint(h.hp)\nh.hurt(100)\nprint(h.hp)',
          out: '70\n0'
        }]
      },
      {
        id: 'class-inherit',
        title: '继承与 super',
        codes: [{
          code: 'class Animal\n    str name = ""\n    function init(name)\n        self.name = name\n    function speak()\n        return "……"\n\nclass Dog extends Animal\n    function init(name)\n        super(name)\n    function speak()\n        return name + "：汪！"\n\nDog d = new Dog("大黄")\nprint(d.speak())',
          out: '大黄：汪！'
        }]
      },
      {
        id: 'class-list',
        title: '对象放进集合',
        codes: [{
          code: 'class Item\n    str name = ""\n    int price = 0\n    function init(n, p)\n        self.name = n\n        self.price = p\n\nlist bag = [new Item("剑", 100), new Item("药", 20)]\nint total = 0\nfor it in bag\n    print(it.name)\n    total += it.price\nprint(join("合计 ", total))',
          out: '剑\n药\n合计 120'
        }]
      }
    ]
  },

  /* ───────────────────────── 10 ───────────────────────── */
  {
    id: 'error',
    title: '错误处理',
    icon: '⚠',
    summary: 'try / catch / finally / throw，以及怎么看懂报错',
    sections: [
      {
        id: 'try',
        title: '捕获错误',
        codes: [{
          code: 'try\n    int bad = 1 - "a"\n    print("这行不会执行")\ncatch e\n    print("抓到错误：" + e.message)\n    print("发生在第 " + e.line + " 行")\nfinally\n    print("无论如何都会执行")',
          out: '抓到错误：减法需要数字\n发生在第 2 行\n无论如何都会执行'
        }],
        tips: [
          { kind: 'note', text: 'e 是一个字典，有 message（错误说明）和 line（出错行号）。' }
        ]
      },
      {
        id: 'throw',
        title: '主动抛错',
        paras: ['自己的函数遇到非法输入时，抛错比返回一个错误码更清晰。'],
        codes: [{
          code: 'function div(a, b)\n    if (b == 0)\n        throw "除数不能为 0"\n    return a / b\n\ntry\n    print(div(10, 2))\n    print(div(10, 0))\ncatch e\n    print("错误：" + e.message)',
          out: '5\n错误：除数不能为 0'
        }]
      },
      {
        id: 'read-error',
        title: '怎么看懂报错',
        paras: [
          'Float IDE 的报错分三层，都在「问题」面板里汇总：',
          '① 语法错（红色波浪线）：写错了结构，比如少括号、if 没写条件。这类不会运行。',
          '② 语义警告（黄色）：用了没定义的变量或函数，会提示「是不是想写 xxx」。',
          '③ 运行错（控制台红色）：跑起来才出的问题，带行号和调用栈。'
        ],
        tips: [
          { kind: 'tip', text: '点「问题」面板里的任何一条，编辑器会跳到对应行。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 11 ───────────────────────── */
  {
    id: 'project',
    title: '模块与工程',
    icon: '⊞',
    summary: '多文件、import、project.json、工程包',
    sections: [
      {
        id: 'import',
        title: '拆成多个文件',
        paras: [
          '工程里的 .cw 文件可以互相 import。import "lib.cw" 之后，lib.cw 里的顶层函数和类就能用了。'
        ],
        codes: [{
          code: '// lib.cw\nfunction twice(x)\n    return x * 2\n\n// main.cw\nimport "lib.cw"\nprint(twice(21))',
          out: '42'
        }],
        tips: [
          { kind: 'warn', text: 'import 的文件里如果有运行期错误，调用方会看到带文件名的调用栈，方便定位。' }
        ]
      },
      {
        id: 'projfile',
        title: 'project.json',
        paras: ['每个工程根目录有一个 project.json，记录工程名和入口文件。'],
        codes: [{
          code: '{\n  "name": "我的工程",\n  "entry": "main.cw",\n  "version": "0.8.0",\n  "kind": "float"\n}'
        }],
        tips: [
          { kind: 'note', text: 'F5 / F6 永远从 entry 指向的文件开始跑。想换入口就改这里。' }
        ]
      },
      {
        id: 'package',
        title: '导出与导入工程包',
        paras: [
          'Ctrl+Shift+E 把整个工程打包成 .cw 文件；欢迎页「导入工程包」能还原。'
        ]
      }
    ]
  },

  /* ───────────────────────── 12 ───────────────────────── */
  {
    id: 'builtins',
    title: '内置函数速查',
    icon: '⚙',
    summary: '数学、字符串、集合、类型判断，一页看全',
    sections: [
      {
        id: 'bi-math',
        title: '数学',
        codes: [{
          code: 'print(abs(-5))\nprint(max(3, 9, 2))\nprint(min(3, 9, 2))\nprint(floor(3.9))\nprint(ceil(3.1))\nprint(round(3.5))\nprint(sqrt(16))\nprint(pow(2, 8))\nprint(random(1, 6))',
          out: '5\n9\n2\n3\n4\n4\n4\n256\n（1~6 的随机数）'
        }]
      },
      {
        id: 'bi-str',
        title: '字符串与集合',
        codes: [{
          code: 'print(len("abc"))\nprint(join("a", 1, true))\nprint(len([1, 2, 3]))\nprint(len({"a": 1}))\nprint(type(1.5))',
          out: '3\na1true\n3\n1\nfloat'
        }]
      },
      {
        id: 'bi-type',
        title: '类型判断',
        codes: [{
          code: 'print(is_num(1))\nprint(is_num("1"))\nprint(type(1))\nprint(type("a"))',
          out: 'true\nfalse\nint\nstr'
        }],
        tips: [
          { kind: 'tip', text: '完整列表在「帮助 → 内置函数」或者直接输入时看补全的说明。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 13 ───────────────────────── */
  {
    id: 'pitfall',
    title: '常见坑',
    icon: '☰',
    summary: '新手最容易踩的十个地方',
    sections: [
      {
        id: 'p-if',
        title: 'if 忘了写条件',
        paras: ['写成光秃秃的 if 会直接报错——这是故意的。否则整段分支会变成永不执行的死代码，你还以为逻辑跑过了。'],
        codes: [{
          code: '// 错\nif\n    print("永远不会执行")\n\n// 对\nif (true)\n    print("会执行")',
          out: '会执行'
        }]
      },
      {
        id: 'p-indent',
        title: '缩进不一致',
        paras: ['一个块里的缩进必须一样。混用 Tab 和空格最容易出问题，建议统一用 4 个空格（设置里可改）。']
      },
      {
        id: 'p-div',
        title: '除法得到小数',
        paras: ['7 / 2 是 3.5。想要整数结果用 floor(7 / 2)，或者用 7 % 2 取余。'],
        codes: [{
          code: 'print(7 / 2)\nprint(floor(7 / 2))\nprint(7 % 2)',
          out: '3.5\n3\n1'
        }]
      },
      {
        id: 'p-copy',
        title: '以为列表是引用',
        paras: ['在 Python 里 b = a 之后改 b 会动到 a。这里不会：赋值就是复制。想让两处共享同一份，得显式传引用（用类实例）。'],
        codes: [{
          code: 'list a = [1, 2]\nlist b = a\nb.push(3)\nprint(a)   // 还是 [1, 2]\nprint(b)   // [1, 2, 3]',
          out: '[1, 2]\n[1, 2, 3]'
        }]
      },
      {
        id: 'p-key',
        title: '字典取不存在的键',
        paras: ['会报错。不确定时用 get(键, 默认值)。'],
        codes: [{
          code: 'dict d = {"a": 1}\nprint(d.get("b", 0))\n// print(d["b"])  ← 这行会报错',
          out: '0'
        }]
      },
      {
        id: 'p-recur',
        title: '递归没有结束条件',
        paras: ['会一直调用自己。引擎在 400 层会主动停下并指出来，但代码得你自己改对。']
      }
    ]
  }
];

/** 章节 + 小节总数，用于页面顶部展示 */
export const TUTORIAL_STATS = {
  chapters: CHAPTERS.length,
  sections: CHAPTERS.reduce((n, c) => n + c.sections.length, 0),
  samples: CHAPTERS.reduce((n, c) => n + c.sections.reduce((m, s) => m + (s.codes?.length ?? 0), 0), 0)
};
