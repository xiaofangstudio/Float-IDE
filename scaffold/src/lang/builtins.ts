/* CW Script 内建函数库。
 *
 * 同一张表被两处使用：
 * 1. 表达式求值：`x = add(a, b)`、`print(len(nums))`、`if (contain(s, "a"))`
 * 2. 指令行：单独写一行的 `add(a, b)` 仍然把结果写进 __last_calc（Copper 老习惯，沿自 CW Script）
 *    —— 靠 writes 字段决定写哪个特殊变量。
 */

import { dict, fmtVal, isDict, isInstance, isList, toNum, toStr, truthy, type Val, isPtr, allocPtr, typeName } from './values';
import { STDLIB_SPECS, STDLIB_MAP, setStdlibFs, type StdFs } from './stdlib';
import { guardList, guardString } from './limits';

export type BuiltinSpec = {
  name: string;
  sig: string;
  category: string;
  doc: string;
  min: number;
  max: number;
  /** 作为独立指令行书写时，结果写入的特殊变量 */
  writes?: '__last_calc' | '__last_cmp' | '__last_read' | '__last_str' | '__last_len';
  /** 异步内建（文件这类要走 Tauri invoke 的）：给了 afn 解释器就 await 它 */
  afn?: (args: Val[]) => Promise<Val>;
  fn: (args: Val[]) => Val;
};

/** 文件系统桥：桌面版由 IDE 注入（Tauri 的 fs 命令），浏览器预览里没有。
 *  相对路径由注入方按工程目录解析好；read 读不到返回 null。
 *  这一处抽象让 F5（Rust 内核）和 F6（解释器）的文件行为一致。
 *  标准库（stdlib.ts）的 os / net 那几个函数也吃同一份桥。 */
export type FsBridge = StdFs;
let fsBridge: FsBridge | null = null;
export function setFsBridge(b: FsBridge | null): void {
  fsBridge = b;
  setStdlibFs(b);
}
function needBridge(): FsBridge {
  if (!fsBridge) {
    bad('文件功能要在 Float IDE 桌面版里用（F5 运行 / F6 调试都行），浏览器预览里没有文件系统');
  }
  return fsBridge as FsBridge;
}

const bad = (msg: string): never => { throw new Error(msg); };

const num = (v: Val, who: string): number => {
  const n = toNum(v);
  if (n === null) bad(`${who} 需要数字，得到 ${fmtVal(v)}`);
  return n as number;
};

const listArg = (v: Val, who: string): Val[] => {
  if (!isList(v)) bad(`${who} 需要列表，得到 ${fmtVal(v)}`);
  return v as Val[];
};

const dictArg = (v: Val, who: string): Map<string, Val> => {
  if (!isDict(v)) bad(`${who} 需要字典，得到 ${fmtVal(v)}`);
  return (v as { map: Map<string, Val> }).map;
};

const div = (a: number, b: number): number => {
  if (b === 0) bad('除数不能为 0');
  return a / b;
};

const mod = (a: number, b: number): number => {
  if (b === 0) bad('取模的除数不能为 0');
  return a % b;
};

const spec = (s: BuiltinSpec): BuiltinSpec => s;

const REGISTRY: BuiltinSpec[] = [
  /* 数学 */
  spec({ name: 'add', sig: 'add(a, b)', category: '数学', doc: '相加；任一参数是字符串时按拼接处理。也支持汇编式写法 c = add a b（空格分隔，可多个）', min: 2, max: 8, writes: '__last_calc', fn: (a) => a.slice(1).reduce<Val>((x, y) => {
    if (typeof x === 'string' || typeof y === 'string') {
      const p = toStr(x), q = toStr(y);
      guardString(p.length, q.length);
      return p + q;
    }
    return num(x, 'add') + num(y, 'add');
  }, a[0]) }),
  spec({ name: 'sub', sig: 'sub(a, b)', category: '数学', doc: '相减。也支持汇编式写法 c = sub a b', min: 2, max: 8, writes: '__last_calc', fn: (a) => a.slice(1).reduce((x, y) => num(x, 'sub') - num(y, 'sub'), a[0]) }),
  spec({ name: 'mul', sig: 'mul(a, b)', category: '数学', doc: '相乘；字符串乘整数可重复。也支持汇编式写法 c = mul a b', min: 2, max: 8, writes: '__last_calc', fn: (a) => a.slice(1).reduce<Val>((x, y) => {
    if (typeof x === 'string' && typeof y === 'number') {
      const n = Math.max(0, Math.trunc(y));
      guardString(0, x.length * n);
      return x.repeat(n);
    }
    return num(x, 'mul') * num(y, 'mul');
  }, a[0]) }),
  spec({ name: 'div', sig: 'div(a, b)', category: '数学', doc: '相除（除数为 0 报错）。也支持汇编式写法 c = div a b', min: 2, max: 8, writes: '__last_calc', fn: (a) => a.slice(1).reduce((x, y) => div(num(x, 'div'), num(y, 'div')), a[0]) }),
  spec({ name: 'mod', sig: 'mod(a, b)', category: '数学', doc: '取余。也支持汇编式写法 c = mod a b', min: 2, max: 8, writes: '__last_calc', fn: (a) => a.slice(1).reduce((x, y) => mod(num(x, 'mod'), num(y, 'mod')), a[0]) }),
  spec({ name: 'pow', sig: 'pow(a, b)', category: '数学', doc: 'a 的 b 次方', min: 2, max: 2, writes: '__last_calc', fn: (a) => Math.pow(num(a[0], 'pow'), num(a[1], 'pow')) }),
  spec({ name: 'rand', sig: 'rand(a, b)', category: '数学', doc: '[a, b] 之间的随机整数（含两端）', min: 2, max: 2, writes: '__last_calc', fn: (a) => {
    const lo = Math.trunc(Math.min(num(a[0], 'rand'), num(a[1], 'rand')));
    const hi = Math.trunc(Math.max(num(a[0], 'rand'), num(a[1], 'rand')));
    return lo + Math.floor(Math.random() * (hi - lo + 1));
  } }),
  spec({ name: 'random', sig: 'random()', category: '数学', doc: '0 到 1 之间的小数', min: 0, max: 0, writes: '__last_calc', fn: () => Math.random() }),
  spec({ name: 'sqrt', sig: 'sqrt(a)', category: '数学', doc: '平方根', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.sqrt(num(a[0], 'sqrt')) }),
  spec({ name: 'abs', sig: 'abs(a)', category: '数学', doc: '绝对值', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.abs(num(a[0], 'abs')) }),
  spec({ name: 'floor', sig: 'floor(a)', category: '数学', doc: '向下取整', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.floor(num(a[0], 'floor')) }),
  spec({ name: 'ceil', sig: 'ceil(a)', category: '数学', doc: '向上取整', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.ceil(num(a[0], 'ceil')) }),
  spec({ name: 'round', sig: 'round(a)', category: '数学', doc: '四舍五入', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.round(num(a[0], 'round')) }),
  spec({ name: 'sin', sig: 'sin(a)', category: '数学', doc: '正弦（弧度）', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.sin(num(a[0], 'sin')) }),
  spec({ name: 'cos', sig: 'cos(a)', category: '数学', doc: '余弦（弧度）', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.cos(num(a[0], 'cos')) }),
  spec({ name: 'tan', sig: 'tan(a)', category: '数学', doc: '正切（弧度）', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.tan(num(a[0], 'tan')) }),
  spec({ name: 'asin', sig: 'asin(a)', category: '数学', doc: '反正弦', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.asin(num(a[0], 'asin')) }),
  spec({ name: 'acos', sig: 'acos(a)', category: '数学', doc: '反余弦', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.acos(num(a[0], 'acos')) }),
  spec({ name: 'atan', sig: 'atan(a)', category: '数学', doc: '反正切', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.atan(num(a[0], 'atan')) }),
  spec({ name: 'atan2', sig: 'atan2(y, x)', category: '数学', doc: '两参数反正切', min: 2, max: 2, writes: '__last_calc', fn: (a) => Math.atan2(num(a[0], 'atan2'), num(a[1], 'atan2')) }),
  spec({ name: 'log', sig: 'log(a)', category: '数学', doc: '自然对数', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.log(num(a[0], 'log')) }),
  spec({ name: 'exp', sig: 'exp(a)', category: '数学', doc: 'e 的 a 次方', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.exp(num(a[0], 'exp')) }),
  spec({
    name: 'min', sig: 'min(a, b, …) 或 min(列表)', category: '数学', doc: '最小值（多参数或一个列表）',
    min: 1, max: 99, writes: '__last_calc',
    fn: (a) => {
      const xs = a.length === 1 && Array.isArray(a[0]) ? a[0] : a;
      return Math.min(...xs.map((x) => num(x, 'min')));
    }
  }),
  spec({
    name: 'max', sig: 'max(a, b, …) 或 max(列表)', category: '数学', doc: '最大值（多参数或一个列表）',
    min: 1, max: 99, writes: '__last_calc',
    fn: (a) => {
      const xs = a.length === 1 && Array.isArray(a[0]) ? a[0] : a;
      return Math.max(...xs.map((x) => num(x, 'max')));
    }
  }),

  /* 比较 / 逻辑 */
  spec({ name: 'lt', sig: 'lt(a, b)', category: '比较', doc: 'a < b', min: 2, max: 2, writes: '__last_cmp', fn: (a) => (typeof a[0] === 'string' && typeof a[1] === 'string' ? a[0] < a[1] : num(a[0], 'lt') < num(a[1], 'lt')) }),
  spec({ name: 'gt', sig: 'gt(a, b)', category: '比较', doc: 'a > b', min: 2, max: 2, writes: '__last_cmp', fn: (a) => (typeof a[0] === 'string' && typeof a[1] === 'string' ? a[0] > a[1] : num(a[0], 'gt') > num(a[1], 'gt')) }),
  spec({ name: 'eq', sig: 'eq(a, b)', category: '比较', doc: 'a == b（容器按内容比较）', min: 2, max: 2, writes: '__last_cmp', fn: (a) => JSON.stringify(norm(a[0])) === JSON.stringify(norm(a[1])) }),
  spec({ name: 'and', sig: 'and(a, b)', category: '逻辑', doc: '逻辑与', min: 2, max: 2, writes: '__last_cmp', fn: (a) => truthy(a[0]) && truthy(a[1]) }),
  spec({ name: 'or', sig: 'or(a, b)', category: '逻辑', doc: '逻辑或', min: 2, max: 2, writes: '__last_cmp', fn: (a) => truthy(a[0]) || truthy(a[1]) }),
  spec({ name: 'not', sig: 'not(a)', category: '逻辑', doc: '逻辑非', min: 1, max: 1, writes: '__last_cmp', fn: (a) => !truthy(a[0]) }),
  spec({ name: 'listin', sig: 'listin(值, 列表)', category: '列表', doc: '值是否在列表里', min: 2, max: 2, writes: '__last_cmp', fn: (a) => listArg(a[1], 'listin').some((x) => JSON.stringify(norm(x)) === JSON.stringify(norm(a[0]))) }),
  spec({ name: 'dictin', sig: 'dictin(字典, 键)', category: '字典', doc: '键是否在字典里', min: 2, max: 2, writes: '__last_cmp', fn: (a) => dictArg(a[0], 'dictin').has(toStr(a[1])) }),

  /* 字符串 */
  spec({ name: 'sorted', sig: 'sorted(列表)', category: '列表', doc: '返回排好序的新列表，原列表不动', min: 1, max: 1, writes: '__last_str', fn: (a) => {
    const xs = a[0] as Val[];
    if (!isList(xs)) bad('sorted() 要的是列表，收到 ' + typeName(xs));
    const isNum = (x: unknown): x is number => typeof x === 'number';
    const cp = xs.slice();
    cp.sort((p, q) => {
      if (isNum(p) && isNum(q)) return (p as number) - (q as number);
      const sp = toStr(p), sq = toStr(q);
      return sp < sq ? -1 : sp > sq ? 1 : 0;
    });
    return cp;
  } }),
  spec({ name: 'join', sig: 'join(a, b, …)', category: '字符串', doc: '把多个值拼成字符串', min: 1, max: 99, writes: '__last_str', fn: (a) => {
    const parts = a.map(toStr);
    guardString(0, parts.reduce((n, x) => n + x.length, 0));
    return parts.join('');
  } }),
  spec({ name: 'char', sig: 'char(s, i)', category: '字符串', doc: '取第 i 个字符（从 0 开始）', min: 2, max: 2, writes: '__last_str', fn: (a) => {
    const s = toStr(a[0]);
    const i = Math.trunc(num(a[1], 'char'));
    if (i < 0 || i >= s.length) bad(`char 下标 ${i} 超出范围（长度 ${s.length}）`);
    return s[i];
  } }),
  spec({ name: 'strlen', sig: 'strlen(s)', category: '字符串', doc: '字符串长度', min: 1, max: 1, writes: '__last_str', fn: (a) => toStr(a[0]).length }),
  spec({ name: 'contain', sig: 'contain(s, 子串)', category: '字符串', doc: '是否包含子串', min: 2, max: 2, writes: '__last_cmp', fn: (a) => toStr(a[0]).includes(toStr(a[1])) }),
  spec({ name: 'upper', sig: 'upper(s)', category: '字符串', doc: '转大写', min: 1, max: 1, fn: (a) => toStr(a[0]).toUpperCase() }),
  spec({ name: 'lower', sig: 'lower(s)', category: '字符串', doc: '转小写', min: 1, max: 1, fn: (a) => toStr(a[0]).toLowerCase() }),
  spec({ name: 'trim', sig: 'trim(s)', category: '字符串', doc: '去掉首尾空白', min: 1, max: 1, fn: (a) => toStr(a[0]).trim() }),
  spec({ name: 'split', sig: 'split(s, 分隔符)', category: '字符串', doc: '按分隔符切成列表', min: 2, max: 2, fn: (a) => {
    const src = toStr(a[0]);
    guardList(0, src.length + 1);
    return src.split(toStr(a[1])) as unknown as Val;
  } }),
  spec({ name: 'indexof', sig: 'indexof(s, 子串)', category: '字符串', doc: '子串位置，找不到返回 -1', min: 2, max: 2, fn: (a) => toStr(a[0]).indexOf(toStr(a[1])) }),
  spec({ name: 'substr', sig: 'substr(s, 起点, 长度)', category: '字符串', doc: '截取子串', min: 2, max: 3, fn: (a) => {
    const s = toStr(a[0]);
    const start = Math.trunc(num(a[1], 'substr'));
    const len = a.length >= 3 ? Math.trunc(num(a[2], 'substr')) : s.length;
    return s.slice(start, start + len);
  } }),

  /* 通用 */
  spec({ name: 'len', sig: 'len(列表 / 字符串 / 字典)', category: '通用', doc: '长度', min: 1, max: 1, writes: '__last_len', fn: (a) => {
    const v = a[0];
    if (isList(v)) return v.length;
    if (typeof v === 'string') return v.length;
    if (isDict(v)) return v.map.size;
    if (isInstance(v)) return v.fields.size;
    if (isPtr(v)) return Math.max(0, v.buf.length - v.idx);
    return toStr(v).length;
  } }),
  spec({ name: 'alloc', sig: 'alloc(格子数)', category: '内存', doc: '手动申请一段内存，返回指针；用完自己 free（不回收也不会自动提醒）', min: 1, max: 1, fn: (a) => {
    const n = Math.trunc(num(a[0], 'alloc'));
    if (n < 0) bad(`alloc 要一个不小于 0 的格子数，实际是 ${n}`);
    if (n > 10_000_000) bad(`alloc 一次要了 ${n} 格，太多了（上限 1000 万格）`);
    return allocPtr(n);
  } }),
  spec({ name: 'free', sig: 'free(指针)', category: '内存', doc: '归还手动申请的内存；归还后再访问会报错', min: 1, max: 1, fn: (a) => {
    const p = a[0];
    if (!isPtr(p)) throw new Error(`free 要一个指针，实际是 ${typeName(p)}`);
    if (p.freed) throw new Error('这块内存已经 free 过了，不用再 free 一次');
    if (!p.owner) {
      throw new Error('这不是 alloc 出来的内存（多半是 &列表[i] 或 &变量 拿到的地址），不能 free');
    }
    p.freed = true;
    p.buf.length = 0;
    return null;
  } }),
  spec({ name: 'range', sig: 'range(n) / range(a, b) / range(a, b, step)', category: '通用', doc: '生成数字列表，常用在 for 里', min: 1, max: 3, fn: (a) => {
    let from = 0;
    let to = 0;
    let step = 1;
    if (a.length === 1) to = num(a[0], 'range');
    else { from = num(a[0], 'range'); to = num(a[1], 'range'); if (a.length >= 3) step = num(a[2], 'range'); }
    if (step === 0) bad('range 的步长不能为 0');
    // 先估算个数：等生成完再拒绝，内存已经吃下去了
    const est = Math.max(0, Math.ceil((to - from) / step));
    guardList(0, est);
    const out: Val[] = [];
    if (step > 0) for (let v = from; v < to; v += step) out.push(v);
    else for (let v = from; v > to; v += step) out.push(v);
    guardList(0, out.length);
    return out;
  } }),
  spec({ name: 'dictallkey', sig: 'dictallkey(字典)', category: '字典', doc: '取出全部键（列表）', min: 1, max: 1, writes: '__last_read', fn: (a) => [...dictArg(a[0], 'dictallkey').keys()] }),
  spec({ name: 'dictallval', sig: 'dictallval(字典)', category: '字典', doc: '取出全部值（列表）', min: 1, max: 1, writes: '__last_read', fn: (a) => [...dictArg(a[0], 'dictallval').values()] }),
  spec({ name: 'type', sig: 'type(值)', category: '通用', doc: '类型名：int / float / str / bool / list / dict / class / null', min: 1, max: 1, fn: (a) => typeOfVal(a[0]) }),

  /* 类型转换 */
  spec({ name: 'int', sig: 'int(值)', category: '转换', doc: '转整数（截断小数）', min: 1, max: 1, fn: (a) => {
    if (typeof a[0] === 'boolean') return a[0] ? 1 : 0;
    const n = Number(toStr(a[0]).trim());
    if (Number.isNaN(n)) bad(`int() 转不了 ${fmtVal(a[0])}`);
    return Math.trunc(n);
  } }),
  spec({ name: 'float', sig: 'float(值)', category: '转换', doc: '转小数', min: 1, max: 1, fn: (a) => {
    const n = Number(toStr(a[0]).trim());
    if (Number.isNaN(n)) bad(`float() 转不了 ${fmtVal(a[0])}`);
    return n;
  } }),
  spec({ name: 'str', sig: 'str(值)', category: '转换', doc: '转字符串', min: 1, max: 1, fn: (a) => toStr(a[0]) }),
  spec({ name: 'bool', sig: 'bool(值)', category: '转换', doc: '转布尔', min: 1, max: 1, fn: (a) => truthy(a[0]) }),

  /* 时间 / 文件 / 网络（正式写法一律英文，不用中文关键字那一套） */
  spec({ name: 'now', sig: 'now()', category: '时间', doc: '当前时间的毫秒级 Unix 时间戳（int）', min: 0, max: 0, writes: '__last_calc', fn: () => Date.now() }),
  spec({
    name: 'readfile', sig: 'readfile(路径)', category: '文件',
    doc: '读整个文本文件的内容（相对路径按工程目录）；桌面版功能。也支持 readfile "路径"',
    min: 1, max: 1, writes: '__last_str',
    afn: async (a) => {
      const s = await needBridge().read(toStr(a[0]));
      if (s === null) bad(`文件不存在或读不了：${fmtVal(a[0])}`);
      return s;
    },
    fn: (a) => desktopOnly('readfile', a)
  }),
  spec({
    name: 'writefile', sig: 'writefile(路径, 内容)', category: '文件',
    doc: '把内容写进文件（覆盖）；返回 true，桌面版功能。也支持 writefile "路径" "内容"',
    min: 2, max: 2, writes: '__last_str',
    afn: async (a) => (await needBridge().write(toStr(a[0]), toStr(a[1]))) as unknown as Val,
    fn: (a) => desktopOnly('writefile', a)
  }),
  spec({
    name: 'appendfile', sig: 'appendfile(路径, 内容)', category: '文件',
    doc: '把内容追加到文件末尾（没有就新建）；返回 true，桌面版功能',
    min: 2, max: 2, writes: '__last_str',
    afn: async (a) => (await needBridge().append(toStr(a[0]), toStr(a[1]))) as unknown as Val,
    fn: (a) => desktopOnly('appendfile', a)
  }),
  spec({
    name: 'hasfile', sig: 'hasfile(路径)', category: '文件',
    doc: '文件是否存在（true / false），桌面版功能',
    min: 1, max: 1, writes: '__last_cmp',
    afn: async (a) => (await needBridge().has(toStr(a[0]))) as unknown as Val,
    fn: (a) => desktopOnly('hasfile', a)
  }),
  spec({ name: 'fetch', sig: 'fetch(网址)', category: '网络', doc: 'GET 请求，返回响应正文（桌面版，10 秒超时）', min: 1, max: 1, writes: '__last_str', fn: (a) => desktopOnly('fetch', a) }),
  spec({ name: 'post', sig: 'post(网址, 内容)', category: '网络', doc: 'POST 请求，把内容发出去并返回响应正文（桌面版）', min: 2, max: 2, writes: '__last_str', fn: (a) => desktopOnly('post', a) }),

  /* 数学：三角函数 / 对数 / 整数组合 / 判断 / 常量 */
  spec({ name: 'idiv', sig: 'idiv(a, b)', category: '数学', doc: '整除：向下取整的除法（7 idiv 2 = 3，-7 idiv 2 = -4）。与 `//` 运算符同义，idiv 是兼容旧版本的写法', min: 2, max: 2, writes: '__last_calc', fn: (a) => {
    const x = num(a[0], 'idiv');
    const y = num(a[1], 'idiv');
    if (!y) bad('idiv 的除数不能是 0');
    const q = Math.floor(x / y);
    return q;
  } }),
  spec({ name: 'asin', sig: 'asin(x)', category: '数学', doc: '反正弦', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.asin(num(a[0], 'asin')) }),
  spec({ name: 'acos', sig: 'acos(x)', category: '数学', doc: '反余弦', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.acos(num(a[0], 'acos')) }),
  spec({ name: 'atan', sig: 'atan(x)', category: '数学', doc: '反正切', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.atan(num(a[0], 'atan')) }),
  spec({ name: 'atan2', sig: 'atan2(y, x)', category: '数学', doc: '两参数反正切（象限也会认）', min: 2, max: 2, writes: '__last_calc', fn: (a) => Math.atan2(num(a[0], 'atan2'), num(a[1], 'atan2')) }),
  spec({ name: 'log2', sig: 'log2(x)', category: '数学', doc: '以 2 为底的对数', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.log2(num(a[0], 'log2')) }),
  spec({ name: 'log10', sig: 'log10(x)', category: '数学', doc: '以 10 为底的对数', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.log10(num(a[0], 'log10')) }),
  spec({ name: 'log1p', sig: 'log1p(x)', category: '数学', doc: 'ln(1 + x)，x 很小时比 log(1+x) 准', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.log1p(num(a[0], 'log1p')) }),
  spec({ name: 'expm1', sig: 'expm1(x)', category: '数学', doc: 'e 的 x 次方减 1，x 很小时比 exp(x)-1 准', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.expm1(num(a[0], 'expm1')) }),
  spec({ name: 'hypot', sig: 'hypot(a, b, …)', category: '数学', doc: '平方和开方（勾股定理，可以给任意多个数）', min: 1, max: 99, writes: '__last_calc', fn: (a) => Math.hypot(...a.map((x) => num(x, 'hypot'))) }),
  spec({ name: 'trunc', sig: 'trunc(x)', category: '数学', doc: '去掉小数部分（朝 0 取整）', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.trunc(num(a[0], 'trunc')) }),
  spec({ name: 'sign', sig: 'sign(x)', category: '数学', doc: '符号：正数 1、负数 -1、0 是 0', min: 1, max: 1, writes: '__last_calc', fn: (a) => Math.sign(num(a[0], 'sign')) }),
  spec({ name: 'gcd', sig: 'gcd(a, b)', category: '数学', doc: '最大公约数', min: 2, max: 2, writes: '__last_calc', fn: (a) => {
    let x = Math.abs(Math.trunc(num(a[0], 'gcd')));
    let y = Math.abs(Math.trunc(num(a[1], 'gcd')));
    while (y) { const t = x % y; x = y; y = t; }
    return x;
  } }),
  spec({ name: 'lcm', sig: 'lcm(a, b)', category: '数学', doc: '最小公倍数', min: 2, max: 2, writes: '__last_calc', fn: (a) => {
    const x = Math.abs(Math.trunc(num(a[0], 'lcm')));
    const y = Math.abs(Math.trunc(num(a[1], 'lcm')));
    if (!x || !y) return 0;
    let p = x, q = y;
    while (q) { const t = p % q; p = q; q = t; }
    return (x / p) * y;
  } }),
  spec({ name: 'factorial', sig: 'factorial(n)', category: '数学', doc: '阶乘（n 不超过 20）', min: 1, max: 1, writes: '__last_calc', fn: (a) => {
    const n = Math.trunc(num(a[0], 'factorial'));
    if (n < 0) bad('factorial 要一个不小于 0 的整数');
    if (n > 20) bad(`factorial(${n}) 太大了（超过 20 会超出整数范围）`);
    let acc = 1;
    for (let i = 2; i <= n; i++) acc *= i;
    return acc;
  } }),
  spec({ name: 'comb', sig: 'comb(n, k)', category: '数学', doc: '组合数 C(n, k)', min: 2, max: 2, writes: '__last_calc', fn: (a) => {
    const n = Math.trunc(num(a[0], 'comb'));
    const k = Math.trunc(num(a[1], 'comb'));
    if (n < 0 || k < 0) bad('comb(n, k) 的两个参数都不能是负数');
    if (k > n) return 0;
    const kk = Math.min(k, n - k);
    let acc = 1;
    for (let i = 0; i < kk; i++) acc = (acc * (n - i)) / (i + 1);
    return Math.round(acc);
  } }),
  spec({ name: 'perm', sig: 'perm(n, k)', category: '数学', doc: '排列数 P(n, k)', min: 2, max: 2, writes: '__last_calc', fn: (a) => {
    const n = Math.trunc(num(a[0], 'perm'));
    const k = Math.trunc(num(a[1], 'perm'));
    if (n < 0 || k < 0) bad('perm(n, k) 的两个参数都不能是负数');
    if (k > n) return 0;
    let acc = 1;
    for (let i = 0; i < k; i++) acc *= n - i;
    return acc;
  } }),
  spec({ name: 'isqrt', sig: 'isqrt(n)', category: '数学', doc: '整数平方根（向下取整）', min: 1, max: 1, writes: '__last_calc', fn: (a) => {
    const n = Math.trunc(num(a[0], 'isqrt'));
    if (n < 0) bad('isqrt 要一个不小于 0 的整数');
    return Math.floor(Math.sqrt(n));
  } }),
  spec({ name: 'isfinite', sig: 'isfinite(x)', category: '数学', doc: '是不是有限数', min: 1, max: 1, writes: '__last_cmp', fn: (a) => Number.isFinite(num(a[0], 'isfinite')) }),
  spec({ name: 'isinf', sig: 'isinf(x)', category: '数学', doc: '是不是无穷大', min: 1, max: 1, writes: '__last_cmp', fn: (a) => !Number.isFinite(num(a[0], 'isinf')) }),
  spec({ name: 'isnan', sig: 'isnan(x)', category: '数学', doc: '是不是"不是数"（NaN）', min: 1, max: 1, writes: '__last_cmp', fn: (a) => Number.isNaN(num(a[0], 'isnan')) }),
  spec({ name: 'near', sig: 'near(a, b) / near(a, b, 容差)', category: '数学', doc: '两个数是不是足够接近（浮点比较别直接 == 用这个），默认相对误差 1e-9', min: 2, max: 3, writes: '__last_cmp', fn: (a) => {
    const x = num(a[0], 'near');
    const y = num(a[1], 'near');
    const tol = a.length >= 3 ? Math.abs(num(a[2], 'near')) : 1e-9;
    return Math.abs(x - y) <= tol * Math.max(Math.abs(x), Math.abs(y), 1);
  } }),
  spec({ name: 'degrees', sig: 'degrees(弧度)', category: '数学', doc: '弧度转角度', min: 1, max: 1, writes: '__last_calc', fn: (a) => (num(a[0], 'degrees') * 180) / Math.PI }),
  spec({ name: 'radians', sig: 'radians(角度)', category: '数学', doc: '角度转弧度', min: 1, max: 1, writes: '__last_calc', fn: (a) => (num(a[0], 'radians') * Math.PI) / 180 }),
  spec({ name: 'pi', sig: 'pi()', category: '数学', doc: '圆周率 π', min: 0, max: 0, writes: '__last_calc', fn: () => Math.PI }),
  spec({ name: 'e', sig: 'e()', category: '数学', doc: '自然常数 e', min: 0, max: 0, writes: '__last_calc', fn: () => Math.E }),
  spec({ name: 'tau', sig: 'tau()', category: '数学', doc: 'τ = 2π', min: 0, max: 0, writes: '__last_calc', fn: () => Math.PI * 2 }),
  spec({ name: 'inf', sig: 'inf()', category: '数学', doc: '无穷大', min: 0, max: 0, writes: '__last_calc', fn: () => Infinity }),
  spec({ name: 'nan', sig: 'nan()', category: '数学', doc: 'NaN（不是数）', min: 0, max: 0, writes: '__last_calc', fn: () => NaN }),

  /* 操作系统（桌面版功能；pathjoin 是纯字符串运算，哪都能用） */
  spec({ name: 'cwd', sig: 'cwd()', category: '系统', doc: '当前脚本所在目录（绝对路径）', min: 0, max: 0, writes: '__last_str', fn: (a) => desktopOnly('cwd', a) }),
  spec({ name: 'chdir', sig: 'chdir(目录)', category: '系统', doc: '切换脚本的"当前目录"（只影响脚本自己，不改整个进程）', min: 1, max: 1, fn: (a) => desktopOnly('chdir', a) }),
  spec({ name: 'listdir', sig: 'listdir(目录)', category: '系统', doc: '列出目录里的文件名（列表）', min: 0, max: 1, writes: '__last_read', fn: (a) => desktopOnly('listdir', a) }),
  spec({ name: 'makedir', sig: 'makedir(目录)', category: '系统', doc: '创建目录（多层会一起建）', min: 1, max: 1, fn: (a) => desktopOnly('makedir', a) }),
  spec({ name: 'removedir', sig: 'removedir(目录)', category: '系统', doc: '删除空目录（不递归，防误删）', min: 1, max: 1, fn: (a) => desktopOnly('removedir', a) }),
  spec({ name: 'removefile', sig: 'removefile(路径)', category: '系统', doc: '删除文件', min: 1, max: 1, fn: (a) => desktopOnly('removefile', a) }),
  spec({ name: 'renamefile', sig: 'renamefile(旧路径, 新路径)', category: '系统', doc: '改名 / 移动文件', min: 2, max: 2, fn: (a) => desktopOnly('renamefile', a) }),
  spec({ name: 'exists', sig: 'exists(路径)', category: '系统', doc: '路径是否存在（文件或目录都算）', min: 1, max: 1, writes: '__last_cmp', fn: (a) => desktopOnly('exists', a) }),
  spec({ name: 'isfile', sig: 'isfile(路径)', category: '系统', doc: '是不是文件', min: 1, max: 1, writes: '__last_cmp', fn: (a) => desktopOnly('isfile', a) }),
  spec({ name: 'isdir', sig: 'isdir(路径)', category: '系统', doc: '是不是目录', min: 1, max: 1, writes: '__last_cmp', fn: (a) => desktopOnly('isdir', a) }),
  spec({ name: 'filesize', sig: 'filesize(路径)', category: '系统', doc: '文件大小（字节）', min: 1, max: 1, writes: '__last_calc', fn: (a) => desktopOnly('filesize', a) }),
  spec({ name: 'getenv', sig: 'getenv(名字)', category: '系统', doc: '读环境变量；没有就给空字符串', min: 1, max: 1, writes: '__last_str', fn: (a) => desktopOnly('getenv', a) }),
  spec({ name: 'pathjoin', sig: 'pathjoin(a, b, …)', category: '系统', doc: '把路径片段拼起来（统一用 /）', min: 1, max: 99, writes: '__last_str', fn: (a) => a.map((x) => toStr(x).replace(/\\/g, '/').replace(/\/+$/, '')).filter(Boolean).join('/') }),
  spec({ name: 'runcmd', sig: 'runcmd(命令)', category: '系统', doc: '跑一条系统命令，返回 {code, out}（out 含标准输出和错误输出）', min: 1, max: 1, fn: (a) => desktopOnly('runcmd', a) }),

  /* 标准库（os / data / json / time / random / ui / re / net）
   * 名字带点，直接并进这张总表：补全、悬停、参数检查、求值全都走同一条路。 */
  ...(STDLIB_SPECS as unknown as BuiltinSpec[])
];

/** 中文别名：早期实现留下的写法，只保证老脚本还能跑，不进补全、不进文档 */
const ALIASES: Record<string, string> = {
  读文件: 'readfile',
  写文件: 'writefile',
  加文件: 'appendfile',
  有文件: 'hasfile',
  时间戳: 'now',
  取网页: 'fetch',
  发网页: 'post'
};

/** 浏览器 demo 里给一句人话：这些功能要走桌面版（F5 走 Rust 内核才有宿主能力） */
function desktopOnly(name: string, _args: Val[]): never {
  throw bad(`${name} 是桌面版功能：在 Float IDE 里按 F5 运行或 F6 调试都行（浏览器预览里没有文件/网络能力）`);
}

function typeOfVal(v: Val): string {
  if (v === null) return 'null';
  if (isPtr(v)) return 'ptr';
  if (Array.isArray(v)) return 'list';
  if (isDict(v)) return 'dict';
  if (isInstance(v)) return 'class';
  if (typeof v === 'number') return Number.isInteger(v) ? 'int' : 'float';
  if (typeof v === 'boolean') return 'bool';
  return 'str';
}

/** eq() 用的归一化：字典/实例转可比较结构 */
function norm(v: Val): unknown {
  if (Array.isArray(v)) return v.map(norm);
  if (isDict(v)) {
    const o: Record<string, unknown> = {};
    for (const [k, x] of v.map) o[k] = norm(x);
    return o;
  }
  if (isInstance(v)) return { __cls: v.cls.name, ...Object.fromEntries([...v.fields].map(([k, x]) => [k, norm(x)])) };
  return v;
}

export const BUILTINS = new Map<string, BuiltinSpec>(REGISTRY.map((b) => [b.name, b]));

/* 老名字 → 标准库实现。
 *
 * `readfile` 和 `os.readfile` 必须是同一份逻辑：以前两处各写一遍，
 * F5 修了 F6 忘改就会"同一个脚本换个引擎结果不同"。这里统一转发，
 * 老名字只保留自己的文档（兼容用），实现完全共用。 */
const LEGACY_TO_STD: Record<string, string> = {
  readfile: 'os.readfile',
  writefile: 'os.writefile',
  appendfile: 'os.appendfile',
  hasfile: 'os.hasfile',
  exists: 'os.exists',
  isfile: 'os.isfile',
  isdir: 'os.isdir',
  filesize: 'os.filesize',
  listdir: 'os.listdir',
  makedir: 'os.makedir',
  removedir: 'os.removedir',
  removefile: 'os.removefile',
  renamefile: 'os.renamefile',
  cwd: 'os.cwd',
  chdir: 'os.chdir',
  getenv: 'os.getenv',
  pathjoin: 'os.pathjoin',
  runcmd: 'os.runcmd',
  fetch: 'net.fetch',
  post: 'net.post'
};
for (const [old, full] of Object.entries(LEGACY_TO_STD)) {
  const b = BUILTINS.get(old);
  const s = STDLIB_MAP.get(full);
  if (!b || !s) continue;
  const impl = s;
  /* 报错文案里把完整名换回用户写的老名字：
   * 写 hasfile 的人看到「os.hasfile 是桌面版功能」会以为 IDE 认错了函数。 */
  const rename = (e: unknown): never => {
    const msg = e instanceof Error ? e.message : String(e);
    throw new Error(msg.split(full).join(old));
  };
  b.fn = (a: Val[]): Val => {
    try { return impl.fn(a); } catch (e) { return rename(e); }
  };
  if (impl.afn) {
    const afn = impl.afn;
    b.afn = async (a: Val[]): Promise<Val> => {
      try { return await afn(a); } catch (e) { return rename(e); }
    };
  }
}

export function getBuiltin(name: string): BuiltinSpec | undefined {
  const n = ALIASES[name] ?? name;
  return BUILTINS.get(n);
}

export function isBuiltin(name: string): boolean {
  const n = ALIASES[name] ?? name;
  return BUILTINS.has(n);
}

/** 给 IDE 补全/悬停用 */
export function builtinList(): BuiltinSpec[] {
  return REGISTRY;
}

/** 空字典构造（供运行时使用） */
export const emptyDict = (): Val => dict();
