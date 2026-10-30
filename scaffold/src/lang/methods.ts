/* 内建方法：字符串 / 列表 / 字典的链式调用。
 *
 * 解释器与编译器共用这一张表，保证 F5 与 F6 行为一致：
 *   s.upper()  xs.push(1)  d.keys()
 */

import { dict, fmtVal, isDict, isList, toNum, toStr, type DictVal, type Val } from './values';

export type MethodResult = { handled: true; value: Val } | { handled: false };

const NOT = { handled: false } as const;

export class MethodError extends Error {}

const bad = (msg: string): never => { throw new MethodError(msg); };

const num = (v: Val, who: string): number => {
  const n = toNum(v);
  if (n === null) bad(`${who} 需要数字，得到 ${fmtVal(v)}`);
  return n as number;
};

const str = (v: Val): string => toStr(v);

const listOf = (v: Val): Val[] => {
  if (!isList(v)) bad(`需要列表，得到 ${fmtVal(v)}`);
  return v as Val[];
};

/** 方法调用入口：处理不了返回 { handled: false }，交给类实例那套逻辑。
 *  参数/下标错误抛 MethodError，两端各自转成自己的报错类型。 */
export function callBuiltinMethod(obj: Val, name: string, args: Val[]): MethodResult {
  if (typeof obj === 'string') return strMethod(obj, name, args);
  if (isList(obj)) return listMethod(obj, name, args);
  if (isDict(obj)) return dictMethod(obj, name, args);
  return NOT;
}

function strMethod(s: string, name: string, a: Val[]): MethodResult {
  switch (name) {
    case 'upper': return { handled: true, value: s.toUpperCase() };
    case 'lower': return { handled: true, value: s.toLowerCase() };
    case 'trim': return { handled: true, value: s.trim() };
    case 'split': return { handled: true, value: s.split(a.length ? str(a[0]) : '') };
    case 'replace': {
      const from = str(a[0] ?? '');
      const to = str(a[1] ?? '');
      return { handled: true, value: s.split(from).join(to) };
    }
    case 'find':
    case 'index_of': return { handled: true, value: s.indexOf(str(a[0] ?? '')) };
    case 'contains':
    case 'includes': return { handled: true, value: s.includes(str(a[0] ?? '')) };
    case 'starts_with': return { handled: true, value: s.startsWith(str(a[0] ?? '')) };
    case 'ends_with': return { handled: true, value: s.endsWith(str(a[0] ?? '')) };
    case 'substr':
    case 'substring': {
      const start = Math.trunc(num(a[0] ?? 0, 'substr'));
      const len = a.length >= 2 ? Math.trunc(num(a[1], 'substr')) : s.length;
      return { handled: true, value: s.slice(start, start + len) };
    }
    case 'repeat': return { handled: true, value: s.repeat(Math.max(0, Math.trunc(num(a[0] ?? 1, 'repeat')))) };
    case 'chars': return { handled: true, value: s.split('') };
    case 'to_int': return { handled: true, value: Math.trunc(num(s.trim() === '' ? 0 : s, 'to_int')) };
    case 'to_float': return { handled: true, value: num(s.trim() === '' ? 0 : s, 'to_float') };
    case 'len': return { handled: true, value: s.length };
    default: return NOT;
  }
}

function listMethod(xs: Val[], name: string, a: Val[]): MethodResult {
  switch (name) {
    case 'push':
    case 'add': xs.push(a[0] ?? null); return { handled: true, value: xs.length };
    case 'pop': return { handled: true, value: xs.length ? (xs.pop() as Val) : null };
    case 'insert': {
      const i = Math.trunc(num(a[0] ?? 0, 'insert'));
      const at = Math.max(0, Math.min(xs.length, i));
      xs.splice(at, 0, a[1] ?? null);
      return { handled: true, value: xs.length };
    }
    case 'remove': {
      const i = Math.trunc(num(a[0] ?? 0, 'remove'));
      if (i < 0 || i >= xs.length) bad(`下标 ${i} 超出范围（长度 ${xs.length}）`);
      return { handled: true, value: xs.splice(i, 1)[0] ?? null };
    }
    case 'index_of': return { handled: true, value: xs.findIndex((x) => JSON.stringify(x) === JSON.stringify(a[0] ?? null)) };
    case 'contains': return { handled: true, value: xs.some((x) => JSON.stringify(x) === JSON.stringify(a[0] ?? null)) };
    case 'join': return { handled: true, value: xs.map((x) => str(x)).join(a.length ? str(a[0]) : '') };
    case 'reverse': xs.reverse(); return { handled: true, value: xs };
    case 'sort': {
      const nums = xs.every((x) => typeof x === 'number');
      xs.sort((p, q) => (nums ? (p as number) - (q as number) : str(p).localeCompare(str(q), 'zh-Hans-CN')));
      return { handled: true, value: xs };
    }
    case 'slice': {
      const from = Math.trunc(num(a[0] ?? 0, 'slice'));
      const to = a.length >= 2 ? Math.trunc(num(a[1], 'slice')) : xs.length;
      return { handled: true, value: xs.slice(from, to) };
    }
    case 'sum': return { handled: true, value: xs.reduce((n: number, x) => n + num(x, 'sum'), 0) };
    case 'clear': xs.length = 0; return { handled: true, value: 0 };
    case 'len': return { handled: true, value: xs.length };
    default: return NOT;
  }
}

function dictMethod(d: DictVal, name: string, a: Val[]): MethodResult {
  switch (name) {
    case 'keys': return { handled: true, value: [...d.map.keys()] };
    case 'values': return { handled: true, value: [...d.map.values()] };
    case 'items':
      return { handled: true, value: [...d.map].map(([k, v]) => [k, v] as Val) };
    case 'has': return { handled: true, value: d.map.has(str(a[0] ?? '')) };
    case 'remove': {
      const k = str(a[0] ?? '');
      const had = d.map.get(k) ?? null;
      d.map.delete(k);
      return { handled: true, value: had };
    }
    case 'get': return { handled: true, value: d.map.has(str(a[0] ?? '')) ? (d.map.get(str(a[0] ?? '')) as Val) : (a[1] ?? null) };
    case 'set': {
      d.map.set(str(a[0] ?? ''), a[1] ?? null);
      return { handled: true, value: dict() };
    }
    case 'len': return { handled: true, value: d.map.size };
    default: return NOT;
  }
}
