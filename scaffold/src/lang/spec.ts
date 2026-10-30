/* Float Script 指令与关键字元数据（v0.2）。
 * 供高亮、补全、悬停、文档展示；内建函数文档来自 builtins.ts，避免两处各写一遍。
 */

import { builtinList } from './builtins';

export type InstDoc = {
  name: string;
  args: string;
  desc: string;
  result?: string;
  category: string;
};

export const INSTRUCTIONS: InstDoc[] = [
  /* 控制 */
  { name: 'if', args: 'if (条件)', desc: '条件分支；条件可省略（按 true 处理）。支持 if / elif / else 多分支', category: '控制' },
  { name: 'elif', args: 'elif (条件)', desc: '否则如果；可写在上一块之后，也可写成 } elif (…) {', category: '控制' },
  { name: 'else', args: 'else', desc: '否则分支；与 if 块同级缩进即可', category: '控制' },
  { name: 'while', args: 'while (条件)', desc: '条件为真时重复执行块内语句', category: '控制' },
  { name: 'for in range', args: 'for i in range(n) / range(a, b) / range(a, b, step)', desc: '计数循环；次数可以是变量或任意表达式，循环变量随便起名', category: '控制' },
  { name: 'for in', args: 'for x in 列表 / 字符串 / 字典', desc: '遍历集合；字典默认给键，写 for k, v in 字典 同时拿键值', category: '控制' },
  { name: 'for to', args: 'for i = 1 to 10', desc: '从 1 数到 10（含两端）；可写 step 指定步长', category: '控制' },
  { name: 'repeat', args: 'repeat(n)', desc: '重复执行 n 次', category: '控制' },
  { name: 'forever', args: 'forever / main loop', desc: '无限循环，用 out / break 跳出', category: '控制' },
  { name: 'wait', args: 'wait 0.5 / wait(表达式)', desc: '等待若干秒（真实挂起，界面不卡）', category: '控制' },
  { name: 'wait until', args: 'wait until (条件)', desc: '条件为假时重复执行块，条件成立后继续往下走', category: '控制' },
  { name: 'if out', args: 'if out(条件)', desc: '条件成立时跳出当前块（条件式 break）', category: '控制' },
  { name: 'break', args: 'break / out', desc: '跳出当前循环', category: '控制' },
  { name: 'balo', args: 'balo / continue', desc: '跳过本轮，进入下一轮', category: '控制' },
  { name: 'stop', args: 'stop / stop all / stop now / stop script', desc: '直接停止脚本', category: '控制' },

  /* 函数与类 */
  { name: 'function', args: 'function name(a, b = 默认值)', desc: '定义函数；支持多参数、默认值、返回值、递归。fn / Function / f 是同样写法', category: '函数' },
  { name: 'return', args: 'return 表达式 / return', desc: '从函数返回一个值', category: '函数' },
  { name: 'class', args: 'class 名称 / class 名称 extends 父类', desc: '定义类：类体里写字段和方法，init 是构造方法', category: '类' },
  { name: 'new', args: 'new 类名(参数)', desc: '创建实例；如果类里有 init，会自动调用', category: '类' },
  { name: 'self', args: 'self / self.字段', desc: '方法里指向当前实例；字段也可以直接裸名读写', category: '类' },
  { name: 'super', args: 'super(参数)', desc: '在子类方法里调用父类的 init', category: '类' },

  /* 变量 */
  { name: 'var decl', args: 'int a = 1 / str s = "x" / Person p = new Person()', desc: '声明变量；右边可以是任意表达式，也可以省略类型直接写 名称 = 值', category: '变量' },
  { name: 'assign', args: 'a = 表达式 / a += 1 / a++', desc: '赋值、复合赋值、自增自减；也支持 p.hp -= 10、xs[0] = 9', category: '变量' },

  /* 输入输出 */
  { name: 'print', args: 'print(值, 值, …)', desc: '输出到控制台，多个值用空格隔开', category: '输入输出' },
  { name: 'ask', args: 'ask "问题"', desc: '向用户提问，回答存进 ans 与 __last_ans', category: '输入输出' },

  /* 集合 */
  { name: 'list', args: 'list 名称 / list 名称 = [1, 2]', desc: '声明列表；也可以直接用列表字面量赋值', category: '列表' },
  { name: 'add to', args: 'add 值 to 列表', desc: '把值追加到列表末尾', category: '列表' },
  { name: 'insert to at', args: 'insert 值 to 列表 at 下标', desc: '在指定位置插入', category: '列表' },
  { name: 'replace with', args: 'replace 列表 下标 with 值', desc: '替换指定下标的元素', category: '列表' },
  { name: 'read', args: 'read 列表 下标', desc: '读取元素并写入 __last_read', result: '__last_read', category: '列表' },
  { name: 'del', args: 'del 列表 下标', desc: '删除指定下标的元素', category: '列表' },
  { name: 'len', args: 'len 列表 / len(值)', desc: '取长度并写入 __last_len；在表达式里可直接用 len(x)', result: '__last_len', category: '列表' },
  { name: 'dict', args: 'dict 名称 / dict d = {"键": 值}', desc: '声明字典，可用字面量或 key 写入', category: '字典' },
  { name: 'dict set', args: 'dict d key 键 = 值', desc: '写入一个键值对', category: '字典' },
  { name: 'dict read', args: 'dict d read 键 / d["键"] / d.键', desc: '读取键值并写入 __last_read', result: '__last_read', category: '字典' },
  { name: 'dict del', args: 'dict d del 键', desc: '删除一个键', category: '字典' },

  /* 运行时 / 模块 */
  { name: 'time read', args: 'time read', desc: '距上次 time reset 的秒数，写入 __last_time', result: '__last_time', category: '运行时' },
  { name: 'time reset', args: 'time reset', desc: '重置计时器', category: '运行时' },
  { name: 'import', args: 'import "库.cw" / import "文件夹"', desc: '导入同工程内的脚本；函数与类会一起带过来', category: '模块' }
];

/** 表达式里可调用的内建函数名（来自 builtins.ts） */
export const EXPR_CALLABLE: string[] = builtinList().map((b) => b.name);

export const SPECIAL_VARS: { name: string; desc: string }[] = [
  { name: 'ans', desc: 'ask 的回答' },
  { name: '__last_ans', desc: 'ans 指令写入的上一次回答' },
  { name: '__last_calc', desc: '数学类指令（add / sub / …）的结果' },
  { name: '__last_cmp', desc: '比较与逻辑类指令的结果' },
  { name: '__last_read', desc: 'read / dict read / dictallkey / dictallval 的结果' },
  { name: '__last_len', desc: 'len 的结果' },
  { name: '__last_str', desc: 'join / char / strlen 的结果' },
  { name: '__last_time', desc: 'time read 的结果（秒）' }
];

export const KEYWORDS: string[] = [
  'if', 'elif', 'else', 'while', 'for', 'in', 'to', 'step', 'repeat', 'forever', 'main', 'loop',
  'wait', 'until', 'out', 'break', 'balo', 'continue', 'stop', 'all', 'now', 'script', 'return',
  'function', 'fn', 'Function', 'class', 'extends', 'new', 'self', 'this', 'super',
  'int', 'float', 'str', 'bool', 'list', 'dict', 'var',
  'print', 'ask', 'ans', 'import', 'time', 'read', 'reset', 'key', 'del', 'insert', 'replace',
  'with', 'at', 'add', 'len', 'range', 'true', 'false', 'null', 'nil'
];

export const TYPE_WORDS: string[] = ['int', 'float', 'str', 'bool', 'list', 'dict', 'var'];

/** 新建文件 / 补全用的代码骨架 */
export const SNIPPETS: { label: string; detail: string; body: string }[] = [
  {
    label: 'class',
    detail: '类骨架（字段 + init + 方法）',
    body: 'class ${1:Person}\n    str ${2:name} = ""\n    int ${3:age} = 0\n\n    function init(${2:name}, ${3:age})\n        self.${2:name} = ${2:name}\n        self.${3:age} = ${3:age}\n\n    function ${4:greet}()\n        return join("我是", ${2:name})\n'
  },
  {
    label: 'function',
    detail: '带参数与返回值的函数',
    body: 'function ${1:name}(${2:a}, ${3:b})\n    return ${2:a} + ${3:b}\n'
  },
  {
    label: 'for range',
    detail: '计数循环',
    body: 'for ${1:i} in range(${2:3})\n    print(${1:i})\n'
  },
  {
    label: 'for in',
    detail: '遍历列表 / 字典',
    body: 'for ${1:item} in ${2:列表}\n    print(${1:item})\n'
  },
  {
    label: 'if else',
    detail: '条件分支',
    body: 'if (${1:条件})\n    ${2:print("成立")}\nelse\n    ${3:print("不成立")}\n'
  },
  {
    label: 'while',
    detail: '条件循环',
    body: 'while (${1:条件})\n    ${2:break}\n'
  }
];

export const LANG_ID = 'floatscript';

/** 内建方法（字符串 / 列表 / 字典），给 p. 之后的补全兜底用 */
export const BUILTIN_METHODS: { name: string; owner: string; desc: string }[] = [
  { name: 'upper', owner: 'str', desc: '转大写' },
  { name: 'lower', owner: 'str', desc: '转小写' },
  { name: 'trim', owner: 'str', desc: '去掉首尾空白' },
  { name: 'split', owner: 'str', desc: '按分隔符切列表' },
  { name: 'replace', owner: 'str', desc: '替换子串' },
  { name: 'find', owner: 'str', desc: '子串位置，找不到 -1' },
  { name: 'contains', owner: 'str / list', desc: '是否包含' },
  { name: 'starts_with', owner: 'str', desc: '是否以…开头' },
  { name: 'ends_with', owner: 'str', desc: '是否以…结尾' },
  { name: 'substr', owner: 'str', desc: '截取子串' },
  { name: 'repeat', owner: 'str', desc: '重复 n 次' },
  { name: 'chars', owner: 'str', desc: '拆成字符列表' },
  { name: 'to_int', owner: 'str', desc: '转整数' },
  { name: 'to_float', owner: 'str', desc: '转小数' },
  { name: 'push', owner: 'list', desc: '末尾追加' },
  { name: 'pop', owner: 'list', desc: '取出末尾' },
  { name: 'insert', owner: 'list', desc: '指定位置插入' },
  { name: 'remove', owner: 'list / dict', desc: '按下标（键）删除' },
  { name: 'index_of', owner: 'list', desc: '元素下标' },
  { name: 'join', owner: 'list', desc: '用分隔符拼成字符串' },
  { name: 'sort', owner: 'list', desc: '排序' },
  { name: 'reverse', owner: 'list', desc: '反转' },
  { name: 'slice', owner: 'list', desc: '截取一段' },
  { name: 'sum', owner: 'list', desc: '求和' },
  { name: 'keys', owner: 'dict', desc: '全部键' },
  { name: 'values', owner: 'dict', desc: '全部值' },
  { name: 'has', owner: 'dict', desc: '键是否存在' },
  { name: 'get', owner: 'dict', desc: '取键值，可给默认值' },
  { name: 'len', owner: 'str / list / dict', desc: '长度' }
];
