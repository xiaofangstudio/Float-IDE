/* CW Script 内建函数库。
 *
 * 同一张表被两处使用：
 * 1. 表达式求值：`x = add(a, b)`、`print(len(nums))`、`if (contain(s, "a"))`
 * 2. 指令行：单独写一行的 `add(a, b)` 仍然把结果写进 __last_calc（CW Script 老习惯）
 *    —— 靠 writes 字段决定写哪个特殊变量。
 */

import { dict, fmtVal, isDict, isInstance, isList, toNum, toStr, truthy, type Val, isPtr, allocPtr, typeName } from './values';

export type BuiltinSpec = {
  name: string;
  sig: string;
  category: string;
  doc: string;
  min: number;
  max: number;
  /** 作为独立指令行书写时，结果写入的特殊变量 */
  writes?: '__last_calc' | '__last_cmp' | '__last_read' | '__last_str' | '__last_len';
  fn: (args: Val[]) => Val;
};

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
  spec({ name: 'add', sig: 'add(a, b)', category: '数学', doc: '相加；任一参数是字符串时按拼接处理', min: 2, max: 2, writes: '__last_calc', fn: (a) => (typeof a[0] === 'string' || typeof a[1] === 'string' ? toStr(a[0]) + toStr(a[1]) : num(a[0], 'add') + num(a[1], 'add')) }),
  spec({ name: 'sub', sig: 'sub(a, b)', category: '数学', doc: '相减', min: 2, max: 2, writes: '__last_calc', fn: (a) => num(a[0], 'sub') - num(a[1], 'sub') }),
  spec({ name: 'mul', sig: 'mul(a, b)', category: '数学', doc: '相乘；字符串乘整数可重复', min: 2, max: 2, writes: '__last_calc', fn: (a) => (typeof a[0] === 'string' && typeof a[1] === 'number' ? a[0].repeat(Math.max(0, Math.trunc(a[1]))) : num(a[0], 'mul') * num(a[1], 'mul')) }),
  spec({ name: 'div', sig: 'div(a, b)', category: '数学', doc: '相除（除数为 0 报错）', min: 2, max: 2, writes: '__last_calc', fn: (a) => div(num(a[0], 'div'), num(a[1], 'div')) }),
  spec({ name: 'mod', sig: 'mod(a, b)', category: '数学', doc: '取余', min: 2, max: 2, writes: '__last_calc', fn: (a) => mod(num(a[0], 'mod'), num(a[1], 'mod')) }),
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
  spec({ name: 'min', sig: 'min(a, b, …)', category: '数学', doc: '最小值', min: 1, max: 99, writes: '__last_calc', fn: (a) => Math.min(...a.map((x) => num(x, 'min'))) }),
  spec({ name: 'max', sig: 'max(a, b, …)', category: '数学', doc: '最大值', min: 1, max: 99, writes: '__last_calc', fn: (a) => Math.max(...a.map((x) => num(x, 'max'))) }),

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
  spec({ name: 'join', sig: 'join(a, b, …)', category: '字符串', doc: '把多个值拼成字符串', min: 1, max: 99, writes: '__last_str', fn: (a) => a.map((x) => toStr(x)).join('') }),
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
  spec({ name: 'split', sig: 'split(s, 分隔符)', category: '字符串', doc: '按分隔符切成列表', min: 2, max: 2, fn: (a) => toStr(a[0]).split(toStr(a[1])) }),
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
    if (isPtr(v)) return v.buf.length;
    return toStr(v).length;
  } }),
  spec({ name: 'alloc', sig: 'alloc(格子数)', category: '内存', doc: '手动申请一段内存，返回指针；用完必须 free（严格模式）', min: 1, max: 1, fn: (a) => {
    const n = Math.trunc(num(a[0], 'alloc'));
    if (n < 0) bad(`alloc 要一个不小于 0 的格子数，实际是 ${n}`);
    if (n > 10_000_000) bad(`alloc 一次要了 ${n} 格，太多了（上限 1000 万格）`);
    return allocPtr(n);
  } }),
  spec({ name: 'free', sig: 'free(指针)', category: '内存', doc: '归还手动申请的内存；归还后再访问会报错', min: 1, max: 1, fn: (a) => {
    const p = a[0];
    if (!isPtr(p)) throw new Error(`free 要一个指针，实际是 ${typeName(p)}`);
    if (p.freed) throw new Error('这块内存已经 free 过了，不用再 free 一次');
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
    const out: Val[] = [];
    if (step > 0) for (let v = from; v < to; v += step) out.push(v);
    else for (let v = from; v > to; v += step) out.push(v);
    if (out.length > 1_000_000) bad('range 生成的列表超过 100 万个元素');
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
  spec({ name: 'bool', sig: 'bool(值)', category: '转换', doc: '转布尔', min: 1, max: 1, fn: (a) => truthy(a[0]) })
];

function typeOfVal(v: Val): string {
  if (v === null) return 'null';
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

export function getBuiltin(name: string): BuiltinSpec | undefined {
  return BUILTINS.get(name);
}

export function isBuiltin(name: string): boolean {
  return BUILTINS.has(name);
}

/** 给 IDE 补全/悬停用 */
export function builtinList(): BuiltinSpec[] {
  return REGISTRY;
}

/** 空字典构造（供运行时使用） */
export const emptyDict = (): Val => dict();
