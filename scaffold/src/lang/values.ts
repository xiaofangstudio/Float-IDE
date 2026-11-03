/* CW Script 值模型：数字 / 字符串 / 布尔 / null / 列表 / 字典 / 类实例。
 *
 * 与 0.1.0 相比多了「类」这一档：ClassDef 是模板，Instance 带自己的字段表。
 * 字典从裸对象改为 DictVal，避免和实例字段、原型链纠缠。
 */

import type { Expr, Stmt, TypeName } from './ast';

export type FuncDef = Extract<Stmt, { k: 'funcDef' }>;

export type ClassDef = {
  name: string;
  base: ClassDef | null;
  /** 字段名 → 默认值表达式与声明类型（实例化时求值） */
  fields: Map<string, { init: Expr | null; type: TypeName | null }>;
  /** 方法名 → 函数定义 */
  methods: Map<string, FuncDef>;
  file: string;
  line: number;
};

export type DictVal = { kind: 'dict'; map: Map<string, Val> };

/** 指针：指向一段可以按格子寻址的内存。
 *
 *  - `alloc(n)`：buf 是自己的一块（owner=true），要手动 free
 *  - `&xs[i]`：buf 就是那个列表本身（owner=false），改 *p 等于改 xs[i]
 *  - `&x`（变量）：走 cell 闭包，读 *p / 写 *p 都回到变量本身
 *
 *  idx 是"当前指到第几格"，指针运算（p+1）只改 idx，共享同一块 buf——
 *  和 C 里地址加一个元素大小是同一件事，只是把裸地址藏了起来，越界会报错而不是踩内存。
 */
export type PtrCell = { get: () => Val; set: (v: Val) => void; alive: () => boolean };
export type PtrVal = {
  kind: 'ptr';
  buf: Val[];
  idx: number;
  /** 是不是 alloc 出来的（只有自己的块才允许 free） */
  owner: boolean;
  freed: boolean;
  /** &变量：直接绑在变量上 */
  cell?: PtrCell;
  /** 这块内存第 0 格的"地址"（打印 address 与地址运算用）。
   *  是一串稳定的编号，不是真实操作系统地址——所以能打印、能加减，
   *  越界/悬空照样报错，不会真踩到别人的内存。 */
  base: number;
};

/** 地址号从这里开始，每次分配往后挪一段（一格 8 字节，16 字节对齐） */
let nextAddr = 0x7f0000000000;
export function freshBase(slots: number): number {
  const bytes = Math.max(1, slots) * 8;
  const base = nextAddr;
  nextAddr += Math.ceil(bytes / 16) * 16;
  return base;
}

export type Instance = {
  kind: 'instance';
  cls: ClassDef;
  fields: Map<string, Val>;
};

export type Val = number | string | boolean | null | Val[] | DictVal | Instance | CInstLike | PtrVal;

/** 编译通道的实例（结构判定，避免与 codegen.ts 循环依赖） */
export type CInstLike = {
  __cinst: 1;
  cls: { name: string };
  f: Record<string, Val>;
};

const isCInstLike = (v: unknown): v is CInstLike =>
  typeof v === 'object' && v !== null && (v as CInstLike).__cinst === 1;

export const dict = (entries?: [string, Val][]): DictVal => ({
  kind: 'dict',
  map: new Map(entries ?? [])
});

export const instance = (cls: ClassDef): Instance => ({ kind: 'instance', cls, fields: new Map() });

export const isPtr = (v: Val): v is PtrVal => typeof v === 'object' && v !== null && (v as PtrVal).kind === 'ptr';

/** 申请 n 个格子（初值 null） */
export const allocPtr = (n: number): PtrVal =>
  ({ kind: 'ptr', buf: new Array(n).fill(null), idx: 0, owner: true, freed: false, base: freshBase(n) });

/** 取地址：列表的元素（buf 共享，写入会改到原列表） */
export const aliasPtr = (arr: Val[], idx: number): PtrVal =>
  ({ kind: 'ptr', buf: arr, idx, owner: false, freed: false, base: freshBase(arr.length) });

/** 取地址：变量（读写都回到变量本身） */
export const cellPtr = (cell: PtrCell): PtrVal =>
  ({ kind: 'ptr', buf: [null], idx: 0, owner: false, freed: false, cell, base: freshBase(1) });

/** 指针平移：共享同一块内存，只挪 idx */
export const shiftPtr = (p: PtrVal, n: number): PtrVal =>
  ({ ...p, idx: p.idx + n });

export const isDict = (v: Val): v is DictVal => typeof v === 'object' && v !== null && (v as DictVal).kind === 'dict';
export const isInstance = (v: Val): v is Instance => typeof v === 'object' && v !== null && (v as Instance).kind === 'instance';
export const isList = (v: Val): v is Val[] => Array.isArray(v);

export function typeName(v: Val): string {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'list';
  if (isDict(v)) return 'dict';
  if (isInstance(v) || isCInstLike(v)) return 'class';
  if (isPtr(v)) return 'ptr';
  if (typeof v === 'number') return Number.isInteger(v) ? 'int' : 'float';
  if (typeof v === 'boolean') return 'bool';
  return 'str';
}

/**
 * 容器格式化。两个保护，缺一个都会让 IDE 当场崩：
 *  - 环检测：list a = [1]; add a to a 之后 print a 会无限递归 → 爆栈。
 *  - 深度上限：几万层嵌套的列表不该把界面卡死。
 * seen 只在「当前这条路径」上生效（递归返回就摘掉），所以同一个对象在不同位置仍会正常展开。
 */
const FMT_MAX_DEPTH = 16;

function fmtInner(v: Val, seen: Set<object>, depth: number): string {
  if (typeof v === 'string') return JSON.stringify(v);
  return fmtVal(v, seen, depth);
}

export function fmtVal(v: Val, seen: Set<object> = new Set(), depth = 0): string {
  if (v === null) return 'null';
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) return String(v);
    // 和 Rust 的 fmt_float 完全一致：整数值直接打整数，其余四舍五入到 1e-12。
    // 两个引擎同一条规则，print 出来的小数位才不会差几位（对拍靠的就是这个）。
    if (Number.isInteger(v) && Math.abs(v) < 9.007199254740992e15) return String(v);
    return String(Math.round(v * 1e12) / 1e12);
  }
  if (isPtr(v)) {
    // 打印成地址：一格 8 字节
    const addr = `0x${(v.base + v.idx * 8).toString(16).toUpperCase().padStart(12, '0')}`;
    return v.freed ? `${addr}（已释放）` : addr;
  }
  const isBox = Array.isArray(v) || isDict(v) || isInstance(v) || isCInstLike(v);
  if (isBox) {
    const boxed = v as unknown as object;
    if (seen.has(boxed)) return '…(循环)';
    if (depth >= FMT_MAX_DEPTH) return '…';
    seen.add(boxed);
    try {
      if (Array.isArray(v)) return '[' + v.map((x) => fmtInner(x, seen, depth + 1)).join(', ') + ']';
      if (isDict(v)) {
        const parts: string[] = [];
        for (const [k, x] of v.map) parts.push(`${k}: ${fmtInner(x, seen, depth + 1)}`);
        return '{' + parts.join(', ') + '}';
      }
      if (isInstance(v)) {
        const parts: string[] = [];
        for (const [k, x] of v.fields) parts.push(`${k}: ${fmtInner(x, seen, depth + 1)}`);
        return `${v.cls.name}{` + parts.join(', ') + '}';
      }
      const ci = v as { cls: { name: string }; f: Record<string, Val> };
      const parts: string[] = [];
      for (const [k, x] of Object.entries(ci.f)) parts.push(`${k}: ${fmtInner(x, seen, depth + 1)}`);
      return `${ci.cls.name}{` + parts.join(', ') + '}';
    } finally {
      seen.delete(boxed);
    }
  }
  return String(v);
}

/** print / join 用的字符串形式：不加引号 */
export function toStr(v: Val): string {
  return fmtVal(v);
}

export function truthy(v: Val): boolean {
  if (v === null) return false;
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0 && !Number.isNaN(v);
  if (typeof v === 'string') return v !== '';
  if (Array.isArray(v)) return v.length > 0;
  if (isDict(v)) return v.map.size > 0;
  if (isPtr(v)) return !v.freed;
  return true;
}

/* a = [1]; add a to a 之后，再比较/拷贝就会无限递归爆栈。
 * 内部递归版本记下「正在处理的这一对」，撞见回路时按相等/共用处理。 */
type SeenPair = Map<object, object>;

function deepEqRec(a: Val, b: Val, seen: SeenPair): boolean {
  if (a === b) return true;
  // 地址按"指向哪里"比较：同一块内存的同一格就算相等（跟 C 里比指针一个意思）
  if (isPtr(a) && isPtr(b)) return a.base === b.base && a.idx === b.idx;
  const aBox = Array.isArray(a) || isDict(a);
  const bBox = Array.isArray(b) || isDict(b);
  if (aBox && bBox) {
    const ao = a as unknown as object;
    const bo = b as unknown as object;
    const prev = seen.get(ao);
    if (prev !== undefined) return prev === bo;
    seen.set(ao, bo);
    try {
      if (Array.isArray(a) && Array.isArray(b)) {
        return a.length === b.length && a.every((x, i) => deepEqRec(x, b[i], seen));
      }
      if (isDict(a) && isDict(b)) {
        if (a.map.size !== b.map.size) return false;
        for (const [k, v] of a.map) {
          if (!b.map.has(k) || !deepEqRec(v, b.map.get(k)!, seen)) return false;
        }
        return true;
      }
      return false;
    } finally { seen.delete(ao); }
  }
  if (isInstance(a) && isInstance(b)) return a === b;
  return false;
}

export function deepEq(a: Val, b: Val): boolean {
  return deepEqRec(a, b, new Map());
}

/** 数字转换：允许 "3" → 3，其余报错由调用方包装 */
export function toNum(v: Val): number | null {
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (typeof v === 'string' && v.trim() !== '' && !Number.isNaN(Number(v))) return Number(v);
  return null;
}

/** 深拷贝：赋值/传参时的容器语义（列表、字典按值，实例按引用） */
export function cloneVal(v: Val): Val {
  return cloneRec(v, new Map());
}

function cloneRec(v: Val, seen: Map<object, Val>): Val {
  if (Array.isArray(v)) {
    const hit = seen.get(v);
    if (hit !== undefined) return hit;          // 自引用：保持同一份，别无限展开
    const out: Val[] = [];
    seen.set(v, out);
    for (const x of v) out.push(cloneRec(x, seen));
    return out;
  }
  if (isDict(v)) {
    const hit = seen.get(v);
    if (hit !== undefined) return hit;
    const d = dict();
    seen.set(v, d);
    for (const [k, x] of v.map) d.map.set(k, cloneRec(x, seen));
    return d;
  }
  return v;
}

/** 类沿继承链查找方法 */
export function findMethod(cls: ClassDef | null, name: string): { fn: FuncDef; owner: ClassDef } | null {
  let c = cls;
  while (c) {
    const fn = c.methods.get(name);
    if (fn) return { fn, owner: c };
    c = c.base;
  }
  return null;
}

export function hasMethod(cls: ClassDef | null, name: string): boolean {
  return findMethod(cls, name) !== null;
}

/** 类沿继承链收集字段（父类在前，子类覆盖） */
export function collectFields(cls: ClassDef): { name: string; init: Expr | null; type: TypeName | null }[] {
  const chain: ClassDef[] = [];
  let c: ClassDef | null = cls;
  while (c) { chain.unshift(c); c = c.base; }
  const out = new Map<string, { init: Expr | null; type: TypeName | null }>();
  for (const cd of chain) for (const [k, v] of cd.fields) out.set(k, v);
  return [...out.entries()].map(([name, f]) => ({ name, init: f.init, type: f.type }));
}

/** 类沿继承链收集方法名（含父类） */
export function collectMethods(cls: ClassDef): string[] {
  const out = new Set<string>();
  let c: ClassDef | null = cls;
  while (c) { for (const k of c.methods.keys()) out.add(k); c = c.base; }
  return [...out];
}

/** 脚本里 `throw 值` 抛出来的东西：把原值原样带到 catch，
 *  和 Rust 内核一致（throw 抛什么，catch 就拿到什么）。 */
export class ThrownValue extends Error {
  constructor(public value: Val) {
    super('throw');
    this.name = 'ThrownValue';
  }
}
