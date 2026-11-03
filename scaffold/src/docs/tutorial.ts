/* Copper 教程内容。
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
    summary: 'Copper 是什么、为什么改名、怎么跑起来、和 Python 有什么关系',
    sections: [
      {
        id: 'what',
        title: '这是什么语言',
        lead: 'Copper 是一门行式脚本语言：一行一句、缩进成块、中英文都认。',
        paras: [
          '它的原名是 CW Script，从 1.6.0 起正式改名 Copper（铜）。铜是流动性最好的金属之一：导电流、导热流，这正好是这门语言追求的两层「流动性」——对机器流动：一次编写，处处能跑，工程、插件、发布出去的应用用的是同一套源码；对人流动：语法简单到随便给谁看，都能一眼读懂、随时上手改。',
          '它有 Python 的读感，但走的是「先翻译成 JS 再跑」的路线：按 F5 时，整份 AST 会被编译成 JavaScript 交给 V8 执行。所以循环、递归、字符串拼接这些热路径，实测比 CPython 快 5～9 倍。',
          '同时它保留了「开箱即用」：没有编译等待，按下就跑；随时可以切到解释执行（F6）打断点、单步、看变量。'
        ],
        tips: [
          { kind: 'note', text: '源文件扩展名是 .co（纯小写）。旧时代的 .cw 文件全部兼容：能打开、能跑、能搜索，不用手动改名。' },
          { kind: 'note', text: '两种执行方式的分工：F5 = 编译执行（快，日常用）；F6 = 解释执行（慢一点，但能断点调试）。同一份代码两边结果一致。' },
          { kind: 'tip', text: '吉祥物是只叫廿七的三脚橘猫。所以代码在第 27 行出错时，报错会变成「喵～第 27 行瘸了…」，后面照常写原因——遇到别慌，改就是了。' }
        ]
      },
      {
        id: 'first',
        title: '第一个程序',
        paras: ['创建工程后，入口文件 main.co 里写：'],
        codes: [{
          code: 'print("你好，Copper！")',
          out: '你好，Copper！'
        }],
        tips: [
          { kind: 'tip', text: 'print 的括号可以省略：print "你好" 也能跑。多个值用逗号隔开，输出时用空格连接。' }
        ]
      },
      {
        id: 'keyword',
        title: '变量名、类型名可以写中文',
        lead: '标识符可以是中文，类型有中文别名；但关键字和内建函数一律用英文。',
        paras: [
          '变量名、函数名、类名都能写中文，所以「血量」「背包」这种名字不用翻译成英文。',
          '类型也有中文别名（整数 = int），老代码能继续跑；但 if / for / fn / print 这些关键字不做中文版——中文关键字是易语言那条路，读起来反而更别扭，这里不走。'
        ],
        codes: [{
          code: 'int 血量 = 100\nstr 名字 = "小方"\nprint(名字, 血量)\n\n-- 类型名有中文别名\n整数 数量 = 3\nprint(数量)',
          out: '小方 100\n3'
        }],
        tips: [
          { kind: 'warn', text: '别在同一段里一半英文一半中文地写关键字，读起来会很难受。建议整个工程统一。' }
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
          code: 'int age = 20\nfloat pi = 3.14159\nstr name = "小方"\nbool ok = true\n\n-- 也可以不写类型\nscore = 95\nprint(age)\nprint(name)\nprint(ok)\nprint(score)',
          out: '20\n小方\ntrue\n95'
        }]
      },
      {
        id: 'types-table',
        title: '六种基本类型',
        paras: ['这就是全部的内置类型，没有更多了——刻意保持小。'],
        codes: [{
          code: 'print(type(1))          -- int\nprint(type(1.5))        -- float\nprint(type("abc"))      -- str\nprint(type(true))       -- bool\nprint(type([1, 2]))     -- list\nprint(type({"k": 1}))   -- dict',
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
          code: 'int total = 100\n\nfunction show()\n    print(total)   -- 读得到全局\n\nshow()',
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
          code: 'str s = "abcdef"\nprint(s[0])          -- a\nprint(s[-1])         -- f\nprint(s.substr(1, 3))  -- bcd\nprint(s.substr(0, 3))  -- abc\nprint(s.substr(3))     -- def',
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
          code: 'str s = "  Hello World  "\nprint(s.trim())\nprint(s.trim().upper())\nprint(s.trim().lower())\nprint(s.trim().replace("World", "Float"))\nprint(s.trim().split(" "))\nprint(s.contains("World"))\nprint(s.trim().starts_with("Hell"))\nprint(s.trim().repeat(2))',
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
          code: 'repeat(3)\n    print("滴答")\n\n-- forever 是无限循环，靠 break 跳出\nint n = 0\nforever\n    n++\n    if (n >= 3)\n        break\nprint(n)',
          out: '滴答\n滴答\n滴答\n3'
        }]
      },
      {
        id: 'scope-rules',
        title: '作用域：什么活到块外，什么出不来',
        lead: '规则只有三条，记住就不容易踩坑。',
        paras: [
          '**1. 块里声明的变量，块外看得见**：if / while / for 里 `int x = …` 之后，外面可以直接用。',
          '**2. for 的循环变量活到循环外**，值等于最后一次循环的那个数（和 Python 一样）。',
          '**3. 函数里声明的变量出不了函数**：外面读它会报「变量没有定义」——这是好事，能挡住打错名字。'
        ],
        codes: [{
          code: 'for i in range(3)\n    print(i)\nprint(i)   -- 2：循环变量活到循环外\n\nif (true)\n    int a = 1\nprint(a)      -- 1：块内声明块外可见',
          out: '0\n1\n2\n2\n1'
        }],
        tips: [
          { kind: 'note', text: '列表、字典是按引用传的：函数里改内容（xs.push / d["k"] = …）调用方看得见；但函数里写 xs = … 只是换了个名字指向，外面不变。' }
        ]
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
          code: 'list xs = [3, 1, 2]\nprint(xs)\nprint(len(xs))\nprint(xs[0])\nprint(xs[-1])\n\n-- 也可以先声明再一个个加\nlist ys\nadd 10 to ys\nadd 20 to ys\nprint(ys)',
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
          code: 'list xs = [10, 20, 30, 40, 50]\nprint(xs.slice(1, 3))\nprint(xs.slice(2))\n\n-- 列表里可以放列表\nlist grid = [[1, 2], [3, 4]]\nprint(grid[1][0])\nprint(grid)',
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
          out: '90\n{小明: 90, 小红: 95, 小刚: 88}\n3\ntrue'
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
          code: '-- 一行一句：字面量要写在一行里\ndict player = {"name": "阿方", "hp": 100, "bag": ["剑", "药"]}\nprint(player["name"])\nprint(player["bag"][0])\n\nlist team = [{"name": "A", "lv": 3}, {"name": "B", "lv": 5}]\nfor m in team\n    print(join(m["name"], " Lv.", m["lv"]))',
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
        title: '函数不能当值传：用分支分发',
        paras: [
          'Copper 的函数是语句层的名字，不是可以传来传去的值：写 apply(twice, 21) 会明确报「twice 是函数，调用要写 twice(…)」。',
          '需要"按名字选一个动作"时，用 if / elif 分发——或者拿字符串当命令名，查表走 switch 式的分支。这样不会出现"这个变量到底是函数还是数据"的歧义。'
        ],
        codes: [{
          code: 'fn twice(x)\n    return x * 2\n\nfn pick(name, v)\n    if name == "twice"\n        return twice(v)\n    return v\n\nprint(pick("twice", 21))\nprint(pick("none", 10))',
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
          out: '抓到错误：「-」需要数字，得到 a\n发生在第 2 行\n无论如何都会执行'
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
          '工程里的 .co 文件可以互相 import。import "lib.co" 之后，lib.co 里的顶层函数和类就能用了。后缀可以省略：import "lib" 也行。'
        ],
        codes: [{
          code: '-- lib.co\nfunction twice(x)\n    return x * 2'
        }, {
          code: '-- main.co\nimport "lib.co"\nprint(twice(21))   -- 42'
        }],
        tips: [
          { kind: 'warn', text: 'import 的文件里如果有运行期错误，调用方会看到带文件名的调用栈，方便定位。' },
          { kind: 'note', text: '这两段要分别建成 lib.co 和 main.co 才能跑；单独拷一段会报「找不到 lib.co」。' }
        ]
      },
      {
        id: 'projfile',
        title: 'project.json',
        paras: ['每个工程根目录有一个 project.json，记录工程名和入口文件。'],
        codes: [{
          code: '{\n  "name": "我的工程",\n  "entry": "main.co",\n  "version": "0.8.0",\n  "kind": "float"\n}'
        }],
        tips: [
          { kind: 'note', text: 'F5 / F6 永远从 entry 指向的文件开始跑。想换入口就改这里。' }
        ]
      },
      {
        id: 'package',
        title: '导出与导入工程包',
        paras: [
          'Ctrl+Shift+E 把整个工程打包成工程包文件；欢迎页「导入工程包」能还原。'
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
          code: 'print(abs(-5))\nprint(max(3, 9, 2))\nprint(min(3, 9, 2))\nprint(floor(3.9))\nprint(ceil(3.1))\nprint(round(3.5))\nprint(sqrt(16))\nprint(pow(2, 8))\nprint(rand(1, 6) >= 1)',
          out: '5\n9\n2\n3\n4\n4\n4\n256\ntrue'
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
          code: 'print(type(1))          -- int\nprint(type("1"))        -- str\nprint(type(1) == "int")  -- true\nprint(type("1") == "int")  -- false',
          out: 'int\nstr\ntrue\nfalse'
        }],
        tips: [
          { kind: 'tip', text: '完整列表在「帮助 → 内置函数」或者直接输入时看补全的说明。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 12.5 标准库 ───────────────────────── */
  {
    id: 'stdlib',
    title: '标准库',
    icon: '▤',
    summary: 'import 库名就能用：data 统计、json、time、random、re、ui',
    sections: [
      {
        id: 'std-import',
        title: '写法：先 import，再写库名.函数',
        paras: [
          '标准库不用下载、不用装包。写一行 import data，就表示"这个文件要用 data 这个库"，然后所有函数都带库名前缀：data.sum(xs)、json.parse(文本)。',
          'import 只是"声明用哪个库"，不会去读文件，也不会多占一行输出。旧的内建名（readfile、now、fetch）全部还在，和库里的名字是同一份实现——但新代码请用带库名的写法，一眼就知道是哪个库。',
          '补全里输入 data. 就会把这个库的函数全列出来；写错库名里的函数（os.nothing）在运行前就会被拦住，不会跑到一半才炸。'
        ],
        codes: [{
          code: `import data
import json

list xs = [3, 1, 2]
print(str(data.sort(xs)))
print(str(data.sum(xs)))
print(str(data.unique([1, 2, 2, 3])))`,
          out: '[1, 2, 3]\n6\n[1, 2, 3]'
        }],
        tips: [
          { kind: 'tip', text: '目前有 8 个库：os 文件与路径、data 数据加工、json、time、random、ui 控制台排版、re 正则、net 网络。完整函数表在「帮助 → 标准库」。' }
        ]
      },
      {
        id: 'std-data',
        title: '成绩统计：不用再手撸冒泡',
        paras: [
          '排序、平均、中位、标准差、去重、频次、切片、拍平、zip 全在 data 里。带字段的列表直接用 data.sort_by(列表, "字段")，第三个参数写 "desc" 就是倒序。',
          'data.sort 不改编传进来的列表（返回排好序的新列表）；random.shuffle 才是就地打乱。这一条和 Python 的 sorted / shuffle 分工一样。'
        ],
        codes: [{
          code: `import data

list 学生 = [{"name": "老陈", "score": 88}, {"name": "阿方", "score": 93}, {"name": "小北", "score": 78}]
for s in data.sort_by(学生, "score", "desc")
    print(s["name"] + " " + str(s["score"]))

print("平均 " + str(data.avg(学生, "score")))
print("中位 " + str(data.median([88, 93, 78])))`,
          out: '阿方 93\n老陈 88\n小北 78\n平均 86.333333333333\n中位 88'
        }],
        tips: [
          { kind: 'note', text: 'data.avg(学生, "score") 的第二参是字段名；不给就按列表元素本身的数字算（data.avg([1, 2, 3]) == 2）。' }
        ]
      },
      {
        id: 'std-json',
        title: 'JSON：读配置、存数据',
        paras: [
          'json.parse 把文本变成字典/列表（对象→字典，数组→列表，有小数点的是小数，没有的是整数）。json.stringify 反过来，给第二个参数就带缩进。',
          '解析失败默认会报错并指出第几个字符不对；不想让一行坏数据把程序炸掉，就用 json.try_parse：它返回 {ok, value} 或 {ok, error}，自己判断。'
        ],
        codes: [{
          code: `import json

str 文本 = "{\"name\": \"阿方\", \"scores\": [88, 93], \"ok\": true}"
var d = json.parse(文本)
print(d["name"])
print(str(d["scores"]))
print(json.stringify(d))`,
          out: '阿方\n[88, 93]\n{"name":"阿方","scores":[88,93],"ok":true}'
        }],
        tips: [
          { kind: 'warn', text: '字符串里的 {名字} 是插值！所以要写真正的 JSON 时，键必须带引号（{"a": 1} 没问题）；像 "{bad}" 这种会被当成"插入变量 bad"。' }
        ]
      },
      {
        id: 'std-time-random',
        title: '时间和随机数',
        paras: [
          'time.format(时间戳, "YYYY-MM-DD HH:mm:ss") 想怎么排就怎么排（Y/M/D/H/m/s 都认，写两个就是补零）。time.mark() 打点，time.since(打点) 拿到"过了几秒"，比自己减时间戳省事。',
          'random.seed(n) 之后整串随机数就固定了——做游戏地图、写测试、复现 bug 都靠它。不 seed 就用时间做种，每次都不一样。'
        ],
        codes: [{
          code: `import time
import random

print(time.format(0, "YYYY-MM-DD"))
print(str(time.diff(100, 160)))

random.seed(42)
print(str(random.rand(1, 6)))`,
          out: '1970-01-01\n60\n4'
        }],
        tips: [
          { kind: 'note', text: 'F5 和 F6 用的是同一套随机算法，同一个 seed 在两个引擎里跑出来的序列完全一样。' }
        ]
      },
      {
        id: 'std-re-ui',
        title: '正则与终端排版',
        paras: [
          're.findall 找全部匹配；模式里有一对括号就只返回括号里的内容——抓网页里的链接 href="([^"]+)" 一行就够。re.replace / split / test / escape 都在。',
          'ui 负责让控制台好看：ui.bar 画进度条、ui.progress 在同一行刷新百分比、ui.table 按显示宽度对齐（中文算两格）、ui.color 上色。'
        ],
        codes: [{
          code: `import re
import ui

print(str(re.findall("[0-9]+", "a1b22")))
print(re.replace("[0-9]+", "N", "a1b22"))
print(ui.bar(1, 4, 8))`,
          out: '["1", "22"]\naNbN\n[██░░░░░░]'
        }],
        tips: [
          { kind: 'tip', text: '正则只需要常用那一小撮（. * + ? [] ^ $ \\d \\w \\s 和分组），够爬数据、清洗文本；不追求兼容 PCRE 全部语法。' }
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
          code: '-- 错：if 后面没有条件\nif\n    print("永远不会执行")',
          out: '(报错) if 后面要跟条件，例如 if (a > 1)'
        }, {
          code: '-- 对\nif (true)\n    print("会执行")',
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
        title: '列表赋值是共享，不是复制',
        paras: [
          'list b = a 之后，a 和 b 指向同一份数据：改 b 就是改 a。这点和 Python 一样，和很多人的直觉不一样。',
          '想要独立副本，用 a.slice()（或 a + [] 这种新建）。改副本不会影响原件。'
        ],
        codes: [{
          code: 'list a = [1, 2]\nlist b = a\nb.push(3)\nprint(a)   -- [1, 2, 3]：同一份\n\nlist c = a.slice()\nc.push(4)\nprint(a)   -- [1, 2, 3]：c 是副本\nprint(c)',
          out: '[1, 2, 3]\n[1, 2, 3]\n[1, 2, 3, 4]'
        }]
      },
      {
        id: 'p-key',
        title: '字典取不存在的键',
        paras: ['会报错。不确定时用 get(键, 默认值)。'],
        codes: [{
          code: 'dict d = {"a": 1}\nprint(d.get("b", 0))\n-- print(d["b"])  ← 这行会报错',
          out: '0'
        }]
      },
      {
        id: 'p-recur',
        title: '递归没有结束条件',
        paras: ['会一直调用自己。引擎在 400 层会主动停下并指出来，但代码得你自己改对。']
      }
    ]
  },

  /* ───────────────────────── 14 ───────────────────────── */
  {
    id: 'pointer',
    title: '指针基础：地址与手动内存',
    icon: '⌖',
    summary: '取地址 &、解引用 *、指针运算 —— 把地址藏起来，只留纪律',
    sections: [
      {
        id: 'ptr-why',
        title: '它和 C 的指针差在哪',
        lead: 'C 的指针本体是"地址"这个数字，所以能运算、能乱指、能踩内存；这里的指针把数字藏起来，只保留"指向哪一格"。',
        paras: [
          '你拿不到一块内存的数值地址，也就不可能算出野指针。但该有的能力一个不少：取地址 &、解引用 *、指针加减、按下标寻址。',
          '越界、free 之后再用、指向已经返回的函数的局部变量——这几种在 C 里是未定义行为，在这里都是明确的中文报错。'
        ]
      },
      {
        id: 'ptr-addr',
        title: 'addr 取地址，read 读回来',
        paras: [
          'addr(x) 拿到变量 x 的地址，read(p) 读出它指向的那一格；write(p, 值) 写回去——改的就是变量本身。'
        ],
        codes: [{
          code: 'int x = 10\naddr p = addr(x)\nprint(read(p))      -- 10\nwrite(p, 42)\nprint(x)       -- 42\nprint(read(p))      -- 42',
          out: '10\n42\n42'
        }],
        tips: [{ kind: 'tip', text: '*p += 1、*p *= 2 这些复合赋值也走指针，写回的还是变量本身。' }]
      },
      {
        id: 'ptr-list',
        title: '取列表元素的地址',
        paras: [
          'addr(xs, i) 指向列表里的那一格（共享同一块内存，不是拷贝）。改 read(p) 就等于改 xs[i]，写"交换两个元素"这类代码特别顺手。'
        ],
        codes: [{
          code: 'list xs = [1, 2, 3]\naddr p = addr(xs, 1)\nprint(read(p))      -- 2\nwrite(p, 9)\nprint(xs[1])   -- 9',
          out: '2\n9'
        }]
      },
      {
        id: 'ptr-math',
        title: '指针运算',
        paras: [
          'p + 1 往后挪一格，p - 1 往回挪一格；两个指针相减得到"中间隔了几格"，和 C 一样。',
          'p[k] 等价于 *(p + k)——相对当前位置的偏移，负数就是往回。'
        ],
        codes: [{
          code: 'list ys = [10, 20, 30]\naddr a = addr(ys, 0)\nprint(read(a + 1))   -- 20\nprint(a[2])       -- 30\nptr b = a + 2\nprint(read(b - 1))   -- 20\nprint(b - a)      -- 2',
          out: '20\n30\n20\n2'
        }],
        tips: [{ kind: 'warn', text: '指针越界不会踩内存，而是当场报错——这是它比 C 安全的地方。' }]
      },
      {
        id: 'ptr-fn',
        title: '拿指针让函数修改调用方',
        paras: [
          'Copper 的参数默认按值传。想让函数改外面的变量，传它的地址进去——这就是最朴素的"输出参数"。'
        ],
        codes: [{
          code: 'fn bump(ptr p)\n    write(p, read(p) + 1)\n\nint x = 41\nbump(addr(x))\nprint(x)   -- 42',
          out: '42'
        }]
      },
      {
        id: 'ptr-alloc',
        title: 'alloc / free：手动内存',
        paras: [
          'alloc(n) 申请 n 格，出来的指针要自己 free。忘了还不会自动回收——Rust 内核在程序结束时会点名报错；IDE 里 F5/F6 用的是另外两个引擎，不做这项检查，账得自己记。',
          '同一个 box 里，动态内存和手动内存可以混着用——这是这门语言设计上最特别的一点，不需要任何开关。'
        ],
        codes: [{
          code: 'addr p = alloc(3)\np[0] = 7\np[1] = p[0] * 2\nprint(p[0], p[1], len(p))\nfree(p)',
          out: '7 14 3'
        }],
        tips: [
          { kind: 'warn', text: 'free 之后再访问、重复 free 都会明确报错；addr(xs, 0) 这种"别人的地址"不允许 free。' },
          { kind: 'note', text: '指向已返回函数的局部变量的指针叫悬空指针，解引用时会报"已经离开作用域"。' }
        ]
      }
    ]
  },

  /* ───────────────────────── 15 ───────────────────────── */
  {
    id: 'pointer-data',
    title: '指针与数据结构',
    icon: '⛓',
    summary: '一块内存当数组、当链表、当返回值——指针真正开始干活的地方',
    sections: [
      {
        id: 'ptr-edges',
        title: '三条边界：逃逸、扩容、free 错东西',
        lead: '这三个问题一定会被问到，答案都是实测出来的。',
        paras: [
          '**悬空是"用到才查"**：函数里 return &局部变量，拿在手里不报错；第一次 *p 才报「这个指针指向的变量已经离开作用域了」。不是不查，是查到用的时候。',
          '**addr(xs, i) 绑的是"那个列表对象"**，不是物理地址：push 扩容不会让指针失效；pop 掉被指的那一格后再读会越界报错；如果把 xs 整个换成新列表，指针仍指着旧列表。',
          '**free 只认 alloc 出来的块**：free(addr(x))、free(addr(xs, 0)) 一律报「这不是 alloc 出来的内存…不能 free」，不会把栈或列表搞坏。'
        ],
        codes: [{
          code: 'list xs = [1, 2]\naddr p = addr(xs, 0)\nxs.push(3)\nprint(read(p))        -- 1：扩容不影响\nprint(xs)\n\nxs = [7, 8]      -- 换了一个列表\nprint(read(p))        -- 1：指针还指旧的\nprint(xs)', 
          out: '1\n[1, 2, 3]\n1\n[7, 8]'
        }, {
          code: 'list xs = [1, 2]\naddr p = addr(xs, 1)\nxs.pop()\nprint(read(p))   -- 被指的这格没了',
          out: '(报错) 指针指到第 1 格，越界了'
        }],
        tips: [
          { kind: 'warn', text: '所以别把 addr(xs, i) 长期存着：列表一变短就可能越界。要长期保存，存下标（int），用的时候再 xs[i]。' }
        ]
      },
      {
        id: 'p-ptrlist',
        title: '把指针放进列表',
        lead: '指针本身也是值，可以装进列表、当参数传、当返回值交出去。',
        paras: [
          '下面申请两块内存，把它们放进一个列表里，然后按下标去读写——这就是"一堆内存块"最朴素的写法。',
          '注意每个块都要自己 free。谁 alloc、谁 free，这条规矩在能看见指针的地方永远成立。'
        ],
        codes: [{
          code: 'addr a = alloc(2)\nptr b = alloc(2)\nlist ps = [a, b]\nps[0][0] = 1\nps[1][0] = 2\nprint(ps[0][0], ps[1][0])\nfree(a)\nfree(b)',
          out: '1 2'
        }],
        tips: [{ kind: 'warn', text: '列表里存的是指针本身，不是它指向的内容；free 之后列表里那个指针就作废了。' }]
      },
      {
        id: 'p-return',
        title: '指针做返回值',
        paras: [
          '函数里 alloc 的内存不会随函数结束消失——把指针返回出去是合法的，接的人负责 free。',
          '这跟返回局部变量的地址是两回事：那种情况引擎会当场拦下来（见第 16 章）。'
        ],
        codes: [{
          code: 'fn make(n)\n    addr q = alloc(n)\n    q[0] = n\n    return q\n\nptr r = make(3)\nprint(r[0], len(r))\nfree(r)',
          out: '3 3'
        }]
      },
      {
        id: 'p-arena',
        title: 'arena：一次申请，自己切块',
        lead: 'arena 不是"一个数组"：它是一次申请一大块，再用游标切出多个不同大小的对象。',
        paras: [
          '做法很朴素：alloc 一整块池子，用 off 记住"下一个空位在哪"。要一块 n 格，就返回当前的 off，然后把 off 往后推 n 格。',
          '每个对象只记自己在池子里的起始偏移（下面代码里的 a、b），读写一律按 偏移 + 下标 算。最后 free(pool) 整块回收，里面所有对象一起没了。',
          '换来的是：分配只是"游标加几格"，没有每次 alloc 的开销；释放只有一次，不会漏掉某一块。代价是不能单独释放中间某一块——arena 要的就是这个取舍。'
        ],
        codes: [{
          code: 'addr pool = alloc(32)\nint off = 0\n\n-- 切 n 格：cursor 里存"下一个空位"，返回这块的起点偏移\nfn carve(ptr cursor, int n)\n    int at = read(cursor)\n    if at + n > len(pool)\n        return -1\n    write(cursor, at + n)\n    return at\n\nint a = carve(addr(off), 3)\npool[a] = 100\npool[a + 1] = 200\npool[a + 2] = 300\n\nint b = carve(addr(off), 2)\npool[b] = 7\npool[b + 1] = 8\n\nint big = carve(addr(off), 99)\nif big == -1\n    print("池子不够了，这块没给")\n\nprint("a 在", a, "b 在", b, "光标", off)\nprint(pool[a], pool[a + 1], pool[a + 2])\nprint(pool[b], pool[b + 1])\nfree(pool)',
          out: '池子不够了，这块没给\na 在 0 b 在 3 光标 5\n100 200 300\n7 8'
        }],
        tips: [
          { kind: 'warn', text: 'carve 里的容量检查是必须的：引擎只在真的越界访问时报错，游标算过头它不知道。' },
          { kind: 'note', text: 'free(pool) 之后，a、b 这些偏移就全作废了——再拿它们访问，引擎会拦下并报「这块内存已经 free」。' }
        ]
      },
      {
        id: 'p-linked',
        title: '用指针拼一条链表',
        paras: [
          '每个节点占两格：第 0 格存值，第 1 格存"下一个节点从哪格开始"，-1 表示到头了。',
          '这是一条真正靠地址串起来的链子——换语言也还是这套东西，只是这里每一步都看得见。'
        ],
        codes: [{
          code: 'addr m = alloc(6)\nm[0] = 10\nm[1] = 2\nm[2] = 20\nm[3] = 4\nm[4] = 30\nm[5] = -1\nint cur = 0\nwhile cur != -1\n    print(m[cur])\n    cur = m[cur + 1]\nfree(m)',
          out: '10\n20\n30'
        }]
      }
    ]
  },

  /* ───────────────────────── 16 ───────────────────────── */
  {
    id: 'pointer-advanced',
    title: '高级指针',
    icon: '✦',
    summary: '多级指针、让指针改指向、悬空与越界、以及什么时候不该用指针',
    sections: [
      {
        id: 'p-multi',
        title: '多级指针：read(read(pp))',
        lead: '指针本身也有地址。&p 拿到"指针变量的地址"，read(read(pp)) 一路解到最里面那格。',
        paras: [
          '词法上 ** 是幂运算符，但幂运算不可能出现在操作数位置——所以前缀位置的 ** 就是"解引用两次"，和 *(\*pp) 完全等价。',
          '想要三层也行：***p 依次解开三层。'
        ],
        codes: [{
          code: 'int x = 7\naddr p = addr(x)\naddr pp = addr(p)\nprint(read(read(pp)))      -- 7\nwrite(read(pp), 9)\nprint(x)         -- 9',
          out: '7\n9'
        }],
        tips: [{ kind: 'note', text: '写成 *(\*pp) 结果一样。前缀 ** 只是为了少写一层括号，AST 上是同一个东西。' }]
      },
      {
        id: 'p-repoint',
        title: '让指针改指向',
        paras: [
          'pp 指向 p 这个变量本身，所以 *pp = &y 改的是"p 现在指着谁"，而不是 y 里面的值。',
          '这是二级指针最常见的用途：函数里想让你手里的指针改指向，就传它的地址。'
        ],
        codes: [{
          code: 'int x = 1\nint y = 2\naddr p = addr(x)\naddr pp = addr(p)\nwrite(read(pp), 5)  -- 改的是 x\nwrite(pp, addr(y))  -- 让 p 改指 y\nprint(x, y, read(p))  -- 5 2 2',
          out: '5 2 2'
        }]
      },
      {
        id: 'p-cursor',
        title: '指针当游标',
        paras: [
          '想让"当前位置"跟着循环走，可以拿一个指针当游标：p 挪到哪，*p 就是哪一格。',
          '这里的 p 和 p + k 是同一个内存块的两个位置，谁改都是改那块内存——没有拷贝，也没有副本。'
        ],
        codes: [{
          code: 'addr buf = alloc(5)\nint i = 0\nwhile i < 5\n    buf[i] = i + 1\n    i += 1\nptr p = buf\nint sum = 0\nint k = 0\nwhile k < 5\n    sum += read(p + k)\n    k += 1\nprint(sum)\nfree(buf)',
          out: '15'
        }],
        tips: [{ kind: 'tip', text: '想真的"挪"指针就写 p = p + 1，之后 p[0] 跟着变；想保留起点就用 p + k，起点不动。' }]
      },
      {
        id: 'p-reverse',
        title: '双指针：原地反转',
        paras: [
          '最经典的高级用法：一个指针从左、一个从右，交换到中间相遇。不新建列表，直接在原内存上改。',
          '注意这里交换的是列表元素本身（addr(xs, i) 拿到的是列表那一格），所以改完 xs 就变了。'
        ],
        codes: [{
          code: 'list xs = [1, 2, 3, 4, 5]\nint i = 0\nint j = len(xs) - 1\nwhile i < j\n    addr a = addr(xs, i)\n    addr b = addr(xs, j)\n    int t = read(a)\n    write(a, read(b))\n    write(b, t)\n    i += 1\n    j -= 1\nprint(xs)',
          out: '[5, 4, 3, 2, 1]'
        }],
        tips: [{ kind: 'note', text: '同样的写法能改一整块 alloc 出来的内存：把 ptr 参数传进函数，函数里 p[i] = p[i] * 2，出来整块都变了。' }]
      },
      {
        id: 'p-block',
        title: '把一整块交给函数改',
        paras: [
          '传指针等于交出"这块内存的修改权"。函数里按格改，调用方看到的就是改完的结果。',
          '这是 C 里最常用的手法之一；在这里它同样安全——越界会报错，不会写坏隔壁的变量。'
        ],
        codes: [{
          code: 'fn double_all(ptr p, int n)\n    int i = 0\n    while i < n\n        p[i] = p[i] * 2\n        i += 1\n\nptr buf = alloc(4)\nbuf[0] = 1\nbuf[1] = 2\nbuf[2] = 3\nbuf[3] = 4\ndouble_all(buf, 4)\nprint(buf[0], buf[1], buf[2], buf[3])\nfree(buf)',
          out: '2 4 6 8'
        }]
      },
      {
        id: 'p-dangling',
        title: '悬空指针：地址还在，东西没了',
        paras: [
          '函数结束时它的局部变量就没了，返回它的地址会得到一个"悬空指针"。解引用时引擎会明确告诉你"已经离开作用域"，而不是读到一堆垃圾。',
          'alloc 出来的内存不受作用域限制，所以第 15 章那种"函数里申请、外面 free"才是对的用法。'
        ],
        codes: [{
          code: 'fn bad()\n    int local = 42\n    return addr(local)\n\naddr p = bad()\nprint(read(p))   -- 这里会报错，不是打印 42',
          out: '(报错) 这个指针指向的变量已经离开作用域了（悬空指针）'
        }],
        tips: [{ kind: 'warn', text: 'free 之后再解引用、重复 free、free 一个来自列表的地址，同样都会当场报错。' }]
      },
      {
        id: 'p-bounds',
        title: '这里的指针和 C 的边界',
        paras: [
          '拿不到裸地址：你没法把指针当整数打印、加减、强转，所以算不出野地址。',
          '越界当场报错，不是踩坏邻居的内存；错误信息会说是第几格、这块一共几格。',
          '仍然是手动内存：alloc 了不 free，内存就一直占着不还（Rust 内核跑完会点名报错，IDE 的两个引擎不报）——这属于你的责任，语言不替你兜。'
        ]
      },
      {
        id: 'p-when',
        title: '什么时候别用指针',
        paras: [
          '日常写脚本：列表、字典、字符串都是自动回收的，比指针安全也够快，别为了"像 C"而用。',
          '需要指针的场景其实很具体：要函数改调用方的变量、要一块连续内存做缓冲、要自己管生命周期。',
          '性能敏感的地方先量再改：指针省的是拷贝和分配，不是魔法；写清楚比写快更难，也更值钱。'
        ],
        tips: [{ kind: 'tip', text: '拿不准就用列表。等真的需要"改调用方"或"连续内存"时，再回来翻这一章。' }]
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
