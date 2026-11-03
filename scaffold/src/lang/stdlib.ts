/* Copper 标准库（TS 引擎侧）：os / data / json / time / random / ui / re / net
 *
 * 与 Rust 内核的 `float-lang/src/stdlib.rs` **一一对应**：同名、同参数、同顺序、
 * 同错误文案。改一边就要改另一边，然后跑 tests/parity.sh 对拍。
 *
 * 设计要点：
 * 1. 对外名字是 `os.readfile(...)` 这种带点的形式。解析器把它收敛成一个变量名
 *    `"os.readfile"`，所以这里注册的内建名也带点，三个引擎走的是同一条路。
 * 2. 需要宿主（文件/环境变量/网络）的部分全部走 StdFs 桥：桌面版由 IDE 注入，
 *    浏览器预览里没有注入，于是明确报"桌面版功能"，而不是静默返回假数据。
 * 3. 纯计算（data / json / time 的换算 / re / ui 的排版）不依赖任何宿主，
 *    浏览器里也能跑，这样教程示例在网页预览里照样能演示。
 */

import { dict, fmtVal, isDict, isList, toNum, toStr, type Val } from './values';

export type WriteVar = '__last_calc' | '__last_cmp' | '__last_read' | '__last_str' | '__last_len';

export type StdlibSpec = {
  name: string;
  sig: string;
  category: string;
  doc: string;
  min: number;
  max: number;
  writes?: WriteVar;
  fn: (args: Val[]) => Val;
  afn?: (args: Val[]) => Promise<Val>;
};

/* ───────────────────────── 宿主桥 ───────────────────────── */

export type HostInfo = {
  pid: number;
  platform: string;
  cwd: string;
  temp: string;
  home: string;
  ip: string;
  args: string[];
  env: Record<string, string>;
};

/** 桌面版注入的能力。除前四个外都是可选的：缺哪个就只影响那一两个函数，
 *  不会让整个标准库失效。 */
export type StdFs = {
  baseDir?: () => string;
  read: (p: string) => Promise<string | null>;
  write: (p: string, c: string) => Promise<boolean>;
  append: (p: string, c: string) => Promise<boolean>;
  has: (p: string) => Promise<boolean>;
  stat?: (p: string) => Promise<{ mtimeMs: number; size: number; exists: boolean }>;
  list?: (p: string) => Promise<{ name: string; isDir: boolean }[]>;
  mkdir?: (p: string) => Promise<boolean>;
  remove?: (p: string) => Promise<boolean>;
  rename?: (from: string, to: string) => Promise<boolean>;
  copy?: (from: string, to: string) => Promise<boolean>;
  host?: () => Promise<HostInfo>;
  setenv?: (k: string, v: string) => Promise<boolean>;
  runCmd?: (cmd: string, dir?: string) => Promise<{ code: number; out: string }>;
  http?: (method: string, url: string, body?: string) => Promise<string>;
};

let fsx: StdFs | null = null;
let hostCache: HostInfo | null = null;
/** chdir 之后的"当前目录"：只影响脚本自己，不动 IDE 进程 */
let cwdOverride: string | null = null;

export function setStdlibFs(b: StdFs | null): void {
  fsx = b;
  hostCache = null;
  cwdOverride = null;
}

const bad = (m: string): never => {
  throw new Error(m);
};

const DESC = '（要在 Float IDE 桌面版里跑：F5 原生运行 / F6 调试都行，浏览器预览里没有宿主能力）';

function needFs(): StdFs {
  if (!fsx) bad('文件功能要在 Float IDE 桌面版里用（F5 运行 / F6 调试都行），浏览器预览里没有文件系统');
  return fsx as StdFs;
}

function needPart<K extends keyof StdFs>(key: K, who: string): NonNullable<StdFs[K]> {
  const b = needFs();
  const v = b[key];
  if (!v) bad(`${who} 在当前环境用不了${DESC}`);
  return v as NonNullable<StdFs[K]>;
}

async function host(): Promise<HostInfo> {
  if (hostCache) return hostCache;
  const b = needFs();
  const fn = b.host;
  if (!fn) throw bad(`读宿主信息要在 Float IDE 桌面版里跑${DESC}`);
  hostCache = await fn();
  return hostCache;
}

/* ───────────────────────── 小工具（与 Rust 侧同名同义） ───────────────────────── */

function need(args: Val[], n: number, name: string): void {
  if (args.length < n) bad(`${name} 需要 ${n} 个参数，实际给了 ${args.length}`);
}

function num(v: Val, who: string): number {
  const n = toNum(v);
  if (n === null) bad(`${who} 需要数字，得到 ${fmtVal(v)}`);
  return n as number;
}

/** 与 Rust 的 `as_int` 一致：小数截断、字符串能解析就用 */
function intOf(v: Val, who: string): number {
  const n = toNum(v);
  if (n === null) bad(`${who} 需要整数，得到 ${fmtVal(v)}`);
  return Math.trunc(n as number);
}

const sOf = (v: Val): string => toStr(v);

function listOf(v: Val, who: string): Val[] {
  if (!isList(v)) bad(`${who} 需要列表，得到 ${fmtVal(v)}`);
  return v as Val[];
}

function dictOf(v: Val, who: string): Map<string, Val> {
  if (!isDict(v)) bad(`${who} 需要字典，得到 ${fmtVal(v)}`);
  return (v as { map: Map<string, Val> }).map;
}

const mkDict = (d: Map<string, Val>): Val => ({ kind: 'dict', map: d }) as unknown as Val;

function normalizePath(p: string): string {
  const out: string[] = [];
  for (const seg of p.split('/')) {
    if (seg === '' || seg === '.') continue;
    if (seg === '..') out.pop();
    else out.push(seg);
  }
  return (p.startsWith('/') ? '/' : '') + out.join('/');
}

/** 相对路径按「脚本目录 / chdir 之后的目录」补全，绝对路径原样返回 */
function resolvePath(p: string): string {
  const n = p.replace(/\\/g, '/');
  if (/^[A-Za-z]:\//.test(n) || n.startsWith('/')) return n;
  const base = cwdOverride ?? fsx?.baseDir?.() ?? '';
  return base ? normalizePath(`${base}/${n}`) : n;
}

function sleepSync(sec: number): void {
  if (sec <= 0) return;
  const until = Date.now() + sec * 1000;
  while (Date.now() < until) { /* 同步等待：浏览器预览里没有更细的挂起手段 */ }
}

/* ───────────────────────── 命名空间表 ───────────────────────── */

export const NAMESPACES: Record<string, readonly string[]> = {
  os: [
    'readfile', 'writefile', 'appendfile', 'hasfile', 'exists', 'isfile', 'isdir', 'filesize',
    'listdir', 'makedir', 'removedir', 'removefile', 'renamefile', 'cwd', 'chdir', 'getenv',
    'pathjoin', 'runcmd', 'basename', 'dirname', 'extname', 'abspath', 'tempdir', 'homedir',
    'copyfile', 'mtime', 'setenv', 'args', 'pid', 'exit', 'platform', 'sleep'
  ],
  data: [
    'sort', 'sort_by', 'max', 'min', 'sum', 'avg', 'median', 'stdev', 'count', 'freq', 'tally',
    'unique', 'flatten', 'chunk', 'zip', 'enumerate', 'reverse', 'slice', 'is_empty', 'keys',
    'values', 'entries', 'merge', 'pick', 'omit', 'get'
  ],
  json: ['parse', 'stringify', 'pretty', 'valid', 'try_parse'],
  time: [
    'now', 'timestamp', 'millis', 'sleep', 'mark', 'since', 'format', 'parse', 'parts', 'add',
    'diff', 'iso'
  ],
  random: ['rand', 'float', 'choice', 'shuffle', 'sample', 'seed', 'gauss', 'weighted'],
  ui: ['clear', 'clear_line', 'color', 'progress', 'menu', 'input', 'table', 'bar', 'title', 'spinner'],
  re: ['findall', 'match', 'search', 'test', 'replace', 'split', 'escape'],
  net: ['fetch', 'post', 'request', 'download', 'encode', 'decode', 'query', 'listen', 'server_close', 'ip']
};

const NS_SET = new Set(Object.keys(NAMESPACES));
const FN_SET = new Set<string>();
for (const [ns, fns] of Object.entries(NAMESPACES)) for (const f of fns) FN_SET.add(`${ns}.${f}`);

/** `os` 这样的库名 */
export const isNamespace = (n: string): boolean => NS_SET.has(n);
/** `os.readfile` 这样的完整函数名 */
export const isStdlibFn = (n: string): boolean => FN_SET.has(n);
/** 补全用：这个库有哪些函数 */
export const stdlibFunctionsOf = (ns: string): readonly string[] => NAMESPACES[ns] ?? [];

const CATEGORY: Record<string, string> = {
  os: '系统', data: '数据', json: 'JSON', time: '时间', random: '随机', ui: '终端界面', re: '正则', net: '网络'
};

/* ───────────────────────── 规格注册 ───────────────────────── */

const SPECS: StdlibSpec[] = [];

function desktopOnly(name: string): never {
  return bad(`${name} 是桌面版功能：在 Float IDE 里按 F5 运行或 F6 调试都行（浏览器预览里没有文件/网络能力）`);
}

function def(
  ns: string,
  name: string,
  args: string,
  doc: string,
  min: number,
  max: number,
  fn: ((a: Val[]) => Val) | null,
  opts: { afn?: (a: Val[]) => Promise<Val>; writes?: WriteVar } = {}
): void {
  const full = `${ns}.${name}`;
  SPECS.push({
    name: full,
    sig: `${full}(${args})`,
    category: CATEGORY[ns] ?? '标准库',
    doc,
    min,
    max,
    writes: opts.writes,
    fn: fn ?? (() => desktopOnly(full)),
    afn: opts.afn
  });
}

/* ═════════════════════════ 1. os ═════════════════════════ */

def('os', 'readfile', '路径', '读整个文本文件（相对路径按脚本目录）', 1, 1, null, {
  writes: '__last_str',
  afn: async (a) => {
    const s = await needFs().read(resolvePath(sOf(a[0])));
    if (s === null) bad(`文件不存在或读不了：${fmtVal(a[0])}`);
    return s as string;
  }
});
def('os', 'writefile', '路径, 内容', '把内容写进文件（覆盖），返回 true', 2, 2, null, {
  writes: '__last_str',
  afn: async (a) => (await needFs().write(resolvePath(sOf(a[0])), sOf(a[1]))) as unknown as Val
});
def('os', 'appendfile', '路径, 内容', '把内容追加到文件末尾（没有就新建）', 2, 2, null, {
  writes: '__last_str',
  afn: async (a) => (await needFs().append(resolvePath(sOf(a[0])), sOf(a[1]))) as unknown as Val
});
def('os', 'hasfile', '路径', '文件是否存在', 1, 1, null, {
  writes: '__last_cmp',
  afn: async (a) => (await needFs().has(resolvePath(sOf(a[0])))) as unknown as Val
});
def('os', 'exists', '路径', '路径是否存在（文件或目录都算）', 1, 1, null, {
  writes: '__last_cmp',
  afn: async (a) => {
    const st = await needPart('stat', 'os.exists')(resolvePath(sOf(a[0])));
    return st.exists as unknown as Val;
  }
});
def('os', 'isfile', '路径', '是不是文件', 1, 1, null, {
  writes: '__last_cmp',
  afn: async (a) => {
    const p = resolvePath(sOf(a[0]));
    const st = await needPart('stat', 'os.isfile')(p);
    if (!st.exists) return false as unknown as Val;
    return (!(await isDir(p))) as unknown as Val;
  }
});
def('os', 'isdir', '路径', '是不是目录', 1, 1, null, {
  writes: '__last_cmp',
  afn: async (a) => (await isDir(resolvePath(sOf(a[0])))) as unknown as Val
});
def('os', 'filesize', '路径', '文件大小（字节）', 1, 1, null, {
  writes: '__last_calc',
  afn: async (a) => {
    const p = resolvePath(sOf(a[0]));
    const st = await needPart('stat', 'os.filesize')(p);
    if (!st.exists) bad(`os.filesize 读不到 ${sOf(a[0])}`);
    return st.size as unknown as Val;
  }
});
def('os', 'mtime', '路径', '文件最后修改时间（毫秒时间戳）', 1, 1, null, {
  writes: '__last_calc',
  afn: async (a) => {
    const p = resolvePath(sOf(a[0]));
    const st = await needPart('stat', 'os.mtime')(p);
    if (!st.exists) bad(`os.mtime 读不到 ${sOf(a[0])}`);
    return st.mtimeMs as unknown as Val;
  }
});
def('os', 'listdir', '目录', '列出目录里的名字（列表）', 0, 1, null, {
  writes: '__last_read',
  afn: async (a) => {
    const p = a.length ? resolvePath(sOf(a[0])) : (cwdOverride ?? fsx?.baseDir?.() ?? '.');
    const items = await needPart('list', 'os.listdir')(p);
    return items.map((x) => x.name) as Val[];
  }
});
def('os', 'makedir', '目录', '创建目录（多层一起建）', 1, 1, null, {
  afn: async (a) => (await needPart('mkdir', 'os.makedir')(resolvePath(sOf(a[0])))) as unknown as Val
});
def('os', 'removedir', '目录', '删除空目录（不递归，防误删）', 1, 1, null, {
  afn: async (a) => (await needPart('remove', 'os.removedir')(resolvePath(sOf(a[0])))) as unknown as Val
});
def('os', 'removefile', '路径', '删除文件', 1, 1, null, {
  afn: async (a) => (await needPart('remove', 'os.removefile')(resolvePath(sOf(a[0])))) as unknown as Val
});
def('os', 'renamefile', '旧路径, 新路径', '改名 / 移动文件', 2, 2, null, {
  afn: async (a) =>
    (await needPart('rename', 'os.renamefile')(resolvePath(sOf(a[0])), resolvePath(sOf(a[1])))) as unknown as Val
});
def('os', 'copyfile', '源, 目标', '复制文件', 2, 2, null, {
  afn: async (a) =>
    (await needPart('copy', 'os.copyfile')(resolvePath(sOf(a[0])), resolvePath(sOf(a[1])))) as unknown as Val
});
def('os', 'cwd', '', '当前脚本所在目录（绝对路径）', 0, 0, () => cwdOverride ?? fsx?.baseDir?.() ?? '', {
  writes: '__last_str'
});
def('os', 'chdir', '目录', '切换脚本自己的"当前目录"（不影响 IDE 进程）', 1, 1, null, {
  afn: async (a) => {
    const p = resolvePath(sOf(a[0]));
    if (!(await isDir(p))) bad(`chdir 找不到目录「${p}」`);
    cwdOverride = p;
    return true as unknown as Val;
  }
});
def('os', 'basename', '路径', '路径最后一段（`a/b/c.co` → `c.co`）', 1, 1, (a) => {
  const p = sOf(a[0]).replace(/\\/g, '/');
  const t = p.replace(/\/+$/, '');
  const i = t.lastIndexOf('/');
  return i >= 0 ? t.slice(i + 1) : t;
}, { writes: '__last_str' });
def('os', 'dirname', '路径', '路径去掉最后一段（`a/b/c.co` → `a/b`）', 1, 1, (a) => {
  const p = sOf(a[0]).replace(/\\/g, '/');
  const t = p.replace(/\/+$/, '');
  const i = t.lastIndexOf('/');
  return i >= 0 ? t.slice(0, i) : '';
}, { writes: '__last_str' });
def('os', 'extname', '路径', '扩展名（含点）；没有就空字符串', 1, 1, (a) => {
  const p = sOf(a[0]).replace(/\\/g, '/');
  const name = p.slice(p.lastIndexOf('/') + 1);
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i) : '';
}, { writes: '__last_str' });
def('os', 'abspath', '路径', '转成绝对路径（相对路径按脚本目录补全）', 1, 1, (a) => resolvePath(sOf(a[0])), {
  writes: '__last_str'
});
def('os', 'pathjoin', '片段, …', '把路径片段拼起来（统一用 /）', 1, 99, (a) =>
  a.map((x) => sOf(x).replace(/\\/g, '/').replace(/\/+$/, '')).filter(Boolean).join('/'), {
  writes: '__last_str'
});
def('os', 'tempdir', '', '系统临时目录', 0, 0, null, {
  writes: '__last_str',
  afn: async () => (await host()).temp as unknown as Val
});
def('os', 'homedir', '', '用户主目录', 0, 0, null, {
  writes: '__last_str',
  afn: async () => (await host()).home as unknown as Val
});
def('os', 'getenv', '名字', '读环境变量；没有就给空字符串', 1, 1, null, {
  writes: '__last_str',
  afn: async (a) => ((await host()).env[sOf(a[0])] ?? '') as unknown as Val
});
def('os', 'setenv', '名字, 值', '设环境变量（只影响 IDE 进程自己）', 2, 2, null, {
  afn: async (a) => {
    const k = sOf(a[0]);
    if (!k) bad('os.setenv 的变量名不能为空');
    const ok = await needPart('setenv', 'os.setenv')(k, sOf(a[1]));
    if (hostCache) hostCache.env[k] = sOf(a[1]);
    return ok as unknown as Val;
  }
});
def('os', 'args', '', '启动参数（列表）', 0, 0, null, {
  writes: '__last_read',
  afn: async () => (await host()).args.slice() as Val[]
});
def('os', 'pid', '', '当前进程号', 0, 0, null, {
  writes: '__last_calc',
  afn: async () => (await host()).pid as unknown as Val
});
def('os', 'platform', '', '平台：windows / macos / linux', 0, 0, () => 'windows', {
  writes: '__last_str',
  afn: async () => (await host()).platform as unknown as Val
});
def('os', 'runcmd', '命令', '跑一条系统命令，返回 {code, out}', 1, 1, null, {
  afn: async (a) => {
    const r = await needPart('runCmd', 'os.runcmd')(sOf(a[0]), cwdOverride ?? fsx?.baseDir?.());
    return mkDict(new Map<string, Val>([['code', r.code], ['out', r.out]]));
  }
});
def('os', 'sleep', '秒', '等一会儿（可以是小数）', 1, 1, (a) => {
  const s = num(a[0], 'os.sleep');
  if (s < 0) bad('os.sleep 的秒数不能是负数');
  sleepSync(s);
  return null;
}, {
  afn: async (a) => {
    const s = num(a[0], 'os.sleep');
    if (s < 0) bad('os.sleep 的秒数不能是负数');
    await new Promise<void>((r) => setTimeout(r, s * 1000));
    return null;
  }
});
def('os', 'exit', '退出码', '结束脚本（原生运行时才会真的退出进程）', 0, 1, (a) => {
  const code = a.length ? intOf(a[0], 'os.exit') : 0;
  return bad(`os.exit(${code})：F6 是调试通道，不会关掉 IDE；要真的退出请按 F5 原生运行`);
});

async function isDir(p: string): Promise<boolean> {
  const list = needPart('list', 'os.isdir');
  try {
    await list(p);
    return true;
  } catch {
    return false;
  }
}

/* ═════════════════════════ 2. data ═════════════════════════ */

/** 值比较：数字比数字，其余按显示文本（和 data.sort 的规则一致） */
function cmpValues(a: Val, b: Val): number {
  if (typeof a === 'number' && typeof b === 'number') return a < b ? -1 : a > b ? 1 : 0;
  const x = toStr(a);
  const y = toStr(b);
  return x < y ? -1 : x > y ? 1 : 0;
}

function pickField(v: Val, key: string): Val | undefined {
  return isDict(v) ? (v as { map: Map<string, Val> }).map.get(key) : undefined;
}

function flattenInto(items: Val[], depth: number, out: Val[]): void {
  for (const it of items) {
    if (isList(it) && depth !== 0) flattenInto(it as Val[], depth - 1, out);
    else out.push(it);
  }
}

def('data', 'sort', '列表[, "desc"]', '排序后的新列表；原列表不动，加 "desc" 反着排', 1, 2, (a) => {
  const items = listOf(a[0], 'data.sort').slice();
  items.sort(cmpValues);
  if (a.length > 1 && sOf(a[1]) === 'desc') items.reverse();
  return items;
}, { writes: '__last_read' });
def('data', 'sort_by', '列表, 字段[, "desc"]', '按字典字段排序（v1 只支持字段名，不支持自定义函数）', 2, 3, (a) => {
  const items = listOf(a[0], 'data.sort_by').slice();
  if (typeof a[1] !== 'string') {
    bad(`data.sort_by 的第二参要写字段名（字符串），得到 ${fmtVal(a[1])}；按函数排序 v1 还没做`);
  }
  const key = a[1] as string;
  let missing: string | null = null;
  items.sort((p, q) => {
    const x = pickField(p, key);
    const y = pickField(q, key);
    if (x === undefined || y === undefined) {
      if (!missing) missing = `data.sort_by：有一项没有字段「${key}」`;
      return 0;
    }
    return cmpValues(x, y);
  });
  if (missing) bad(missing);
  if (a.length > 2 && sOf(a[2]) === 'desc') items.reverse();
  return items;
}, { writes: '__last_read' });
def('data', 'max', '列表', '最大值', 1, 1, (a) => bestOf(a, 'max'), { writes: '__last_calc' });
def('data', 'min', '列表', '最小值', 1, 1, (a) => bestOf(a, 'min'), { writes: '__last_calc' });
def('data', 'sum', '列表', '求和（全是整数就给整数）', 1, 1, (a) => {
  const items = listOf(a[0], 'data.sum');
  let total = 0;
  for (let i = 0; i < items.length; i++) {
    if (typeof items[i] !== 'number') bad(`data.sum 的第 ${i + 1} 项不是数字：${toStr(items[i])}`);
    total += items[i] as number;
  }
  return total;
}, { writes: '__last_calc' });
def('data', 'avg', '列表[, 字段]', '平均值（给了字段就按那个字段算，比如 data.avg(学生, "score")）', 1, 2, (a) => {
  const items = listOf(a[0], 'data.avg');
  if (items.length === 0) bad('data.avg：空列表没有平均值');
  const key = a.length > 1 ? sOf(a[1]) : null;
  let total = 0;
  for (let i = 0; i < items.length; i++) {
    let v = items[i];
    if (key !== null) {
      const got = pickField(v, key);
      if (got === undefined) bad(`data.avg：第 ${i + 1} 项没有字段「${key}」`);
      v = got as Val;
    }
    total += num(v, 'data.avg');
  }
  return total / items.length;
}, { writes: '__last_calc' });
def('data', 'median', '列表', '中位数', 1, 1, (a) => {
  const items = listOf(a[0], 'data.median');
  if (items.length === 0) bad('data.median：空列表没有中位数');
  const nums = items.map((x, i) => num(x, `data.median 的第 ${i + 1} 项`));
  nums.sort((p, q) => p - q);
  const n = nums.length;
  return n % 2 === 1 ? nums[(n - 1) / 2] : (nums[n / 2 - 1] + nums[n / 2]) / 2;
}, { writes: '__last_calc' });
def('data', 'stdev', '列表', '标准差（样本，除以 n-1）', 1, 1, (a) => {
  const items = listOf(a[0], 'data.stdev');
  if (items.length < 2) bad('data.stdev：至少要 2 个数');
  const nums = items.map((x) => num(x, 'data.stdev'));
  const mean = nums.reduce((s, x) => s + x, 0) / nums.length;
  const varr = nums.reduce((s, x) => s + (x - mean) * (x - mean), 0) / (nums.length - 1);
  return Math.sqrt(varr);
}, { writes: '__last_calc' });
def('data', 'count', '列表, 值', '这个值出现了几次', 2, 2, (a) => {
  const items = listOf(a[0], 'data.count');
  return items.filter((x) => cmpValues(x, a[1]) === 0).length;
}, { writes: '__last_calc' });
def('data', 'freq', '列表', '每个值出现几次（字典）', 1, 1, (a) => freqOf(a[0]), { writes: '__last_read' });
def('data', 'tally', '列表', 'freq 的别名', 1, 1, (a) => freqOf(a[0]), { writes: '__last_read' });
def('data', 'unique', '列表', '去重，保持第一次出现的顺序', 1, 1, (a) => {
  const items = listOf(a[0], 'data.unique');
  const out: Val[] = [];
  for (const it of items) if (!out.some((x) => cmpValues(x, it) === 0)) out.push(it);
  return out;
}, { writes: '__last_read' });
def('data', 'flatten', '列表[, 层数]', '拍平（默认 1 层）', 1, 2, (a) => {
  const items = listOf(a[0], 'data.flatten');
  const depth = a.length > 1 ? intOf(a[1], 'data.flatten') : 1;
  const out: Val[] = [];
  flattenInto(items, depth, out);
  return out;
}, { writes: '__last_read' });
def('data', 'chunk', '列表, 每块长度', '切成一块一块', 2, 2, (a) => {
  const items = listOf(a[0], 'data.chunk');
  const n = intOf(a[1], 'data.chunk');
  if (n <= 0) bad('data.chunk 的每块长度要大于 0');
  const out: Val[] = [];
  for (let i = 0; i < items.length; i += n) out.push(items.slice(i, i + n));
  return out;
}, { writes: '__last_read' });
def('data', 'zip', '列表A, 列表B', '把两个列表按位置配成 [a, b]（短的为准）', 2, 2, (a) => {
  const x = listOf(a[0], 'data.zip');
  const y = listOf(a[1], 'data.zip');
  const n = Math.min(x.length, y.length);
  const out: Val[] = [];
  for (let i = 0; i < n; i++) out.push([x[i], y[i]]);
  return out;
}, { writes: '__last_read' });
def('data', 'enumerate', '列表[, 起点]', '每个元素配上下标 → [[0, 值], [1, 值]]', 1, 2, (a) => {
  const items = listOf(a[0], 'data.enumerate');
  const start = a.length > 1 ? intOf(a[1], 'data.enumerate') : 0;
  return items.map((v, i) => [start + i, v] as Val);
}, { writes: '__last_read' });
def('data', 'reverse', '列表', '倒过来的新列表', 1, 1, (a) => listOf(a[0], 'data.reverse').slice().reverse(), {
  writes: '__last_read'
});
def('data', 'slice', '列表, 起点[, 终点]', '切片（负数从末尾算）', 2, 3, (a) => {
  const items = listOf(a[0], 'data.slice');
  const n = items.length;
  let from = intOf(a[1], 'data.slice');
  let to = a.length > 2 ? intOf(a[2], 'data.slice') : n;
  if (from < 0) from += n;
  if (to < 0) to += n;
  from = Math.min(Math.max(from, 0), n);
  to = Math.min(Math.max(to, 0), n);
  return from >= to ? [] : items.slice(from, to);
}, { writes: '__last_read' });
def('data', 'is_empty', '列表/字典/字符串', '是不是空的', 1, 1, (a) => {
  const v = a[0];
  if (isList(v)) return (v as Val[]).length === 0;
  if (isDict(v)) return (v as { map: Map<string, Val> }).map.size === 0;
  if (typeof v === 'string') return v === '';
  return bad(`data.is_empty 需要列表/字典/字符串，得到 ${fmtVal(v)}`);
}, { writes: '__last_cmp' });
def('data', 'keys', '字典', '全部键（列表）', 1, 1, (a) => [...dictOf(a[0], 'data.keys').keys()], {
  writes: '__last_read'
});
def('data', 'values', '字典', '全部值（列表）', 1, 1, (a) => [...dictOf(a[0], 'data.values').values()], {
  writes: '__last_read'
});
def('data', 'entries', '字典', '全部键值对 → [[键, 值], …]', 1, 1, (a) =>
  [...dictOf(a[0], 'data.entries').entries()].map(([k, v]) => [k, v] as Val), { writes: '__last_read' });
def('data', 'merge', '字典A, 字典B', '合并（B 的同名键覆盖 A）', 2, 2, (a) => {
  const x = dictOf(a[0], 'data.merge');
  const y = dictOf(a[1], 'data.merge');
  const out = new Map<string, Val>(x);
  for (const [k, v] of y) out.set(k, v);
  return mkDict(out);
}, { writes: '__last_read' });
def('data', 'pick', '字典, 键列表', '只留这些键', 2, 2, (a) => pickOrOmit(a, true), { writes: '__last_read' });
def('data', 'omit', '字典, 键列表', '去掉这些键', 2, 2, (a) => pickOrOmit(a, false), { writes: '__last_read' });
def('data', 'get', '字典, 键[, 默认值]', '取字段；没有就给默认值（没给就是 null）', 2, 3, (a) => {
  const d = dictOf(a[0], 'data.get');
  const k = sOf(a[1]);
  const got = d.get(k);
  if (got !== undefined) return got;
  return a.length > 2 ? a[2] : null;
}, { writes: '__last_read' });

function bestOf(a: Val[], which: 'max' | 'min'): Val {
  const items = listOf(a[0], `data.${which}`);
  if (items.length === 0) bad(`data.${which}：空列表`);
  let best = items[0];
  for (let i = 1; i < items.length; i++) {
    const c = cmpValues(items[i], best);
    if (which === 'max' ? c > 0 : c < 0) best = items[i];
  }
  return best;
}

function freqOf(v: Val): Val {
  const items = listOf(v, 'data.freq');
  const d = new Map<string, Val>();
  for (const it of items) {
    const k = toStr(it);
    const cur = d.get(k);
    d.set(k, (typeof cur === 'number' ? cur : 0) + 1);
  }
  return mkDict(d);
}

function pickOrOmit(a: Val[], keep: boolean): Val {
  const d = dictOf(a[0], keep ? 'data.pick' : 'data.omit');
  const keys = listOf(a[1], keep ? 'data.pick' : 'data.omit').map((x) => sOf(x));
  const out = new Map<string, Val>();
  for (const [k, v] of d) if (keys.includes(k) === keep) out.set(k, v);
  return mkDict(out);
}

/* ═════════════════════════ 3. json ═════════════════════════ */

/** 普通 JS 值 → Copper 值（对象变字典，保持插入顺序） */
function fromPlain(x: unknown): Val {
  if (x === null || x === undefined) return null;
  if (Array.isArray(x)) return x.map(fromPlain);
  if (typeof x === 'object') {
    const d = new Map<string, Val>();
    for (const [k, v] of Object.entries(x as Record<string, unknown>)) d.set(k, fromPlain(v));
    return mkDict(d);
  }
  if (typeof x === 'number' || typeof x === 'string' || typeof x === 'boolean') return x;
  return toStr(x as Val);
}

function parseJson(text: string): Val {
  try {
    return fromPlain(JSON.parse(text));
  } catch (e) {
    return bad(`json.parse 出错：${e instanceof Error ? e.message : String(e)}`);
  }
}

function writeJson(v: Val, indent: number, level: number, out: string[]): void {
  const pad = (n: number): string => ' '.repeat(n * indent);
  if (v === null) { out.push('null'); return; }
  if (typeof v === 'boolean') { out.push(v ? 'true' : 'false'); return; }
  if (typeof v === 'number') { out.push(Number.isInteger(v) ? String(v) : String(v)); return; }
  if (typeof v === 'string') { out.push(jsonEscape(v)); return; }
  if (isList(v)) {
    const items = v as Val[];
    if (!items.length) { out.push('[]'); return; }
    out.push('[');
    items.forEach((it, i) => {
      if (i > 0) out.push(',');
      if (indent > 0) out.push('\n' + pad(level + 1));
      writeJson(it, indent, level + 1, out);
    });
    if (indent > 0) out.push('\n' + pad(level));
    out.push(']');
    return;
  }
  if (isDict(v)) {
    const d = (v as { map: Map<string, Val> }).map;
    if (!d.size) { out.push('{}'); return; }
    out.push('{');
    let i = 0;
    for (const [k, val] of d) {
      if (i++ > 0) out.push(',');
      if (indent > 0) out.push('\n' + pad(level + 1));
      out.push(jsonEscape(k));
      out.push(indent > 0 ? ': ' : ':');
      writeJson(val, indent, level + 1, out);
    }
    if (indent > 0) out.push('\n' + pad(level));
    out.push('}');
    return;
  }
  out.push(jsonEscape(toStr(v)));
}

function jsonEscape(s: string): string {
  let out = '"';
  for (const ch of s) {
    const c = ch.codePointAt(0) as number;
    if (ch === '"') out += '\\"';
    else if (ch === '\\') out += '\\\\';
    else if (ch === '\n') out += '\\n';
    else if (ch === '\r') out += '\\r';
    else if (ch === '\t') out += '\\t';
    else if (c < 0x20) out += `\\u${c.toString(16).padStart(4, '0')}`;
    else out += ch;
  }
  return out + '"';
}

def('json', 'parse', '文本', '把 JSON 文本变成列表/字典；格式不对会报错并指出位置', 1, 1, (a) => parseJson(sOf(a[0])), {
  writes: '__last_read'
});
def('json', 'try_parse', '文本', '解析失败也不报错，返回 {ok, value} 或 {ok, error}', 1, 1, (a) => {
  try {
    const v = parseJson(sOf(a[0]));
    return mkDict(new Map<string, Val>([['ok', true], ['value', v]]));
  } catch (e) {
    const msg = e instanceof Error ? e.message.replace(/^json\.parse 出错：/, '') : String(e);
    return mkDict(new Map<string, Val>([['ok', false], ['error', msg]]));
  }
}, { writes: '__last_read' });
def('json', 'valid', '文本', '是不是合法 JSON', 1, 1, (a) => {
  try {
    JSON.parse(sOf(a[0]));
    return true;
  } catch {
    return false;
  }
}, { writes: '__last_cmp' });
def('json', 'stringify', '值[, 缩进]', '转成 JSON 文本；给缩进就带换行', 1, 2, (a) => {
  const indent = a.length > 1 ? Math.max(0, intOf(a[1], 'json.stringify')) : 0;
  const out: string[] = [];
  writeJson(a[0], indent, 0, out);
  return out.join('');
}, { writes: '__last_str' });
def('json', 'pretty', '值', '转成带 2 空格缩进的 JSON 文本', 1, 1, (a) => {
  const out: string[] = [];
  writeJson(a[0], 2, 0, out);
  return out.join('');
}, { writes: '__last_str' });

/* ═════════════════════════ 4. time ═════════════════════════ */

/** 本地时区偏移（秒）：和 Rust 侧一样只算一次 */
let tzCache: number | null = null;
function tzOffset(): number {
  if (tzCache === null) tzCache = -new Date().getTimezoneOffset() * 60;
  return tzCache;
}

function daysFromCivil(y: number, m: number, d: number): number {
  const yy = m <= 2 ? y - 1 : y;
  const era = Math.trunc((yy >= 0 ? yy : yy - 399) / 400);
  const yoe = yy - era * 400;
  const mp = (m + 9) % 12;
  const doy = Math.trunc((153 * mp + 2) / 5) + d - 1;
  const doe = yoe * 365 + Math.trunc(yoe / 4) - Math.trunc(yoe / 100) + doy;
  return era * 146097 + doe - 719468;
}

function civilFromDays(z0: number): [number, number, number] {
  const z = z0 + 719468;
  const era = Math.trunc((z >= 0 ? z : z - 146096) / 146097);
  const doe = z - era * 146097;
  const yoe = Math.trunc((doe - Math.trunc(doe / 1460) + Math.trunc(doe / 36524) - Math.trunc(doe / 146096)) / 365);
  const y = yoe + era * 400;
  const doy = doe - (365 * yoe + Math.trunc(yoe / 4) - Math.trunc(yoe / 100));
  const mp = Math.trunc((5 * doy + 2) / 153);
  const d = doy - Math.trunc((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  return [m <= 2 ? y + 1 : y, m, d];
}

type TimeParts = {
  year: number; month: number; day: number;
  hour: number; minute: number; second: number;
  weekday: number; yearday: number;
};

function partsOf(ts: number): TimeParts {
  const local = ts + tzOffset();
  const secs = Math.floor(local);
  const days = Math.floor(secs / 86400);
  const rem = secs - days * 86400;
  const [year, month, day] = civilFromDays(days);
  return {
    year, month, day,
    hour: Math.trunc(rem / 3600),
    minute: Math.trunc((rem % 3600) / 60),
    second: rem % 60,
    weekday: (((days + 4) % 7) + 7) % 7 + 1,
    yearday: days - daysFromCivil(year, 1, 1) + 1
  };
}

function formatTs(ts: number, fmt: string): string {
  const p = partsOf(ts);
  let out = '';
  const chars = [...fmt];
  for (let i = 0; i < chars.length; i++) {
    const c = chars[i];
    if (c !== 'Y' && c !== 'M' && c !== 'D' && c !== 'H' && c !== 'm' && c !== 's') {
      out += c;
      continue;
    }
    let n = 1;
    while (chars[i + 1] === c) { i++; n++; }
    const v = c === 'Y' ? p.year : c === 'M' ? p.month : c === 'D' ? p.day
      : c === 'H' ? p.hour : c === 'm' ? p.minute : p.second;
    out += n >= 2 ? String(v).padStart(2, '0') : String(v);
  }
  return out;
}

function parseTimeText(text: string): number | null {
  const cleaned = text.trim().replace('T', ' ');
  const segs = cleaned.split(' ');
  const datePart = segs[0] ?? '';
  const dp = datePart.split('-');
  if (dp.length < 3) return null;
  const y = Number(dp[0]);
  const mo = Number(dp[1]);
  const d = Number(dp[2]);
  if (!Number.isFinite(y) || !Number.isFinite(mo) || !Number.isFinite(d)) return null;
  const timePart = segs[1] ?? '00:00:00';
  const tp = timePart.split(':');
  const h = Number(tp[0] ?? '0') || 0;
  const mi = Number(tp[1] ?? '0') || 0;
  const s = Number(tp[2] ?? '0') || 0;
  return daysFromCivil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s - tzOffset();
}

const nowSecs = (): number => Date.now() / 1000;

def('time', 'now', '', '现在的时间文本 `YYYY-MM-DD HH:mm:ss`', 0, 0, () => formatTs(nowSecs(), 'YYYY-MM-DD HH:mm:ss'), {
  writes: '__last_str'
});
def('time', 'timestamp', '', '现在的时间戳（秒，带小数）', 0, 0, () => nowSecs(), { writes: '__last_calc' });
def('time', 'millis', '', '现在的时间戳（毫秒，整数）', 0, 0, () => Date.now(), { writes: '__last_calc' });
def('time', 'mark', '', '计时起点（毫秒），配合 time.since 算耗时', 0, 0, () => Date.now(), { writes: '__last_calc' });
def('time', 'since', '起点毫秒', '从起点到现在过了几秒', 1, 1, (a) => (Date.now() - intOf(a[0], 'time.since')) / 1000, {
  writes: '__last_calc'
});
def('time', 'sleep', '秒', '等一会儿', 1, 1, (a) => {
  const s = num(a[0], 'time.sleep');
  if (s < 0) bad('time.sleep 的秒数不能是负数');
  sleepSync(s);
  return null;
}, {
  afn: async (a) => {
    const s = num(a[0], 'time.sleep');
    if (s < 0) bad('time.sleep 的秒数不能是负数');
    await new Promise<void>((r) => setTimeout(r, s * 1000));
    return null;
  }
});
def('time', 'format', '时间戳[, 格式]', '格式化时间戳；格式里 YYYY/MM/DD/HH/mm/ss 会被替换', 1, 2, (a) => {
  const ts = num(a[0], 'time.format');
  const fmt = a.length > 1 ? sOf(a[1]) : 'YYYY-MM-DD HH:mm:ss';
  return formatTs(ts, fmt);
}, { writes: '__last_str' });
def('time', 'parse', '时间文本', '把 `2026-11-03 20:44:01` 这样的文本变成时间戳（秒）', 1, 1, (a) => {
  const text = sOf(a[0]);
  const ts = parseTimeText(text);
  if (ts === null) bad(`time.parse 认不出这个时间：${text}（要写成 2026-11-03 20:44:01）`);
  return ts as number;
}, { writes: '__last_calc' });
def('time', 'parts', '[时间戳]', '拆成 {year, month, day, hour, minute, second, weekday, yearday}', 0, 1, (a) => {
  const ts = a.length ? num(a[0], 'time.parts') : nowSecs();
  const p = partsOf(ts);
  return mkDict(new Map<string, Val>([
    ['year', p.year], ['month', p.month], ['day', p.day],
    ['hour', p.hour], ['minute', p.minute], ['second', p.second],
    ['weekday', p.weekday], ['yearday', p.yearday]
  ]));
}, { writes: '__last_read' });
def('time', 'add', '时间戳, 秒数', '往后推几秒（负数就是往前）', 2, 2, (a) =>
  num(a[0], 'time.add') + num(a[1], 'time.add'), { writes: '__last_calc' });
def('time', 'diff', '起点, 终点', '两个时间戳差几秒', 2, 2, (a) =>
  num(a[1], 'time.diff') - num(a[0], 'time.diff'), { writes: '__last_calc' });
def('time', 'iso', '[时间戳]', 'ISO 风格 `YYYY-MM-DDTHH:mm:ss`', 0, 1, (a) => {
  const ts = a.length ? num(a[0], 'time.iso') : nowSecs();
  return formatTs(ts, 'YYYY-MM-DDTHH:mm:ss');
}, { writes: '__last_str' });

/* ═════════════════════════ 5. random ═════════════════════════ */

/* xorshift64*：和 Rust 侧是同一个算法、同一个状态机，所以 seed 之后两边
 * 产生的序列一模一样（对拍能逐字比）。用 BigInt 只为把 64 位语义写对，
 * 每次调用一次 BigInt 运算，随机数本来就不该出现在热循环里。 */
const MASK64 = (1n << 64n) - 1n;
let rngState = 0n;

function rngNext(): bigint {
  let x = rngState;
  if (x === 0n) {
    // 没 seed：拿纳秒时间做种（Rust 侧同款兜底）
    x = (BigInt(Date.now()) * 1000000n + 1n) & MASK64;
    if (x === 0n) x = 1n;
  }
  x ^= x >> 12n;
  x = (x ^ (x << 25n)) & MASK64;
  x ^= x >> 27n;
  rngState = x;
  return (x * 0x2545f4914f6cdd1dn) & MASK64;
}

const rngFloat = (): number => Number(rngNext() >> 11n) / Number(1n << 53n);

def('random', 'seed', '整数', '固定随机种子：同一个种子每次跑出同一串数', 1, 1, (a) => {
  const n = intOf(a[0], 'random.seed');
  rngState = BigInt.asUintN(64, BigInt(n)) | 1n;
  return null;
});
def('random', 'rand', 'a, b', '[a, b] 之间的随机整数（含两端）', 2, 2, (a) => {
  const x = intOf(a[0], 'random.rand');
  const y = intOf(a[1], 'random.rand');
  const lo = Math.min(x, y);
  const hi = Math.max(x, y);
  return lo + Math.trunc(rngFloat() * Math.max(hi - lo + 1, 1));
}, { writes: '__last_calc' });
def('random', 'float', '', '0 到 1 之间的随机小数', 0, 0, () => rngFloat(), { writes: '__last_calc' });
def('random', 'choice', '列表', '随机挑一个', 1, 1, (a) => {
  const items = listOf(a[0], 'random.choice');
  if (!items.length) bad('random.choice：空列表没得选');
  return items[Math.trunc(rngFloat() * items.length) % items.length];
}, { writes: '__last_read' });
def('random', 'shuffle', '列表', '就地打乱，返回同一个列表', 1, 1, (a) => {
  const v = a[0];
  if (!isList(v)) bad(`random.shuffle 需要列表，得到 ${fmtVal(v)}`);
  const items = v as Val[];
  for (let i = items.length - 1; i >= 1; i--) {
    const j = Math.trunc(rngFloat() * (i + 1)) % (i + 1);
    const t = items[i]; items[i] = items[j]; items[j] = t;
  }
  return items;
}, { writes: '__last_read' });
def('random', 'sample', '列表, 个数', '不重复地抽几个', 2, 2, (a) => {
  const pool = listOf(a[0], 'random.sample').slice();
  const k = intOf(a[1], 'random.sample');
  if (k < 0 || k > pool.length) bad(`random.sample：要从 ${pool.length} 项里抽 ${k} 个，数量不对`);
  const out: Val[] = [];
  for (let i = 0; i < k; i++) {
    const j = i + Math.trunc(rngFloat() * (pool.length - i)) % (pool.length - i);
    const t = pool[i]; pool[i] = pool[j]; pool[j] = t;
    out.push(pool[i]);
  }
  return out;
}, { writes: '__last_read' });
def('random', 'gauss', '均值, 标准差', '正态分布的随机数', 2, 2, (a) => {
  const mu = num(a[0], 'random.gauss');
  const sigma = num(a[1], 'random.gauss');
  let u1 = rngFloat();
  if (u1 <= 0) u1 = 1e-12;
  const u2 = rngFloat();
  const z = Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
  return mu + sigma * z;
}, { writes: '__last_calc' });
def('random', 'weighted', '值列表, 权重列表', '按权重随机挑一个（彩票那种）', 2, 2, (a) => {
  const items = listOf(a[0], 'random.weighted');
  const weights = listOf(a[1], 'random.weighted');
  if (items.length !== weights.length) {
    bad(`random.weighted：值有 ${items.length} 个，权重有 ${weights.length} 个，数量要一样`);
  }
  let total = 0;
  for (const w of weights) {
    const n = num(w, 'random.weighted 的权重');
    if (n < 0) bad('random.weighted 的权重不能是负数');
    total += n;
  }
  if (total <= 0) bad('random.weighted 的权重加起来要大于 0');
  let pick = rngFloat() * total;
  for (let i = 0; i < weights.length; i++) {
    pick -= num(weights[i], 'random.weighted 的权重');
    if (pick <= 0) return items[i];
  }
  return items[items.length - 1];
}, { writes: '__last_read' });

/* ═════════════════════════ 6. ui ═════════════════════════ */

function makeBar(v: number, max: number, width: number): string {
  const ratio = max <= 0 ? 0 : Math.min(Math.max(v / max, 0), 1);
  const filled = Math.round(ratio * width);
  return `[${'█'.repeat(filled)}${'░'.repeat(Math.max(0, width - filled))}]`;
}

/** 显示宽度：中日韩全角算 2，其余算 1（表格对齐用） */
function displayWidth(s: string): number {
  let w = 0;
  for (const ch of s) {
    const u = ch.codePointAt(0) as number;
    const wide =
      (u >= 0x1100 && u <= 0x115f) || (u >= 0x2e80 && u <= 0xa4cf) ||
      (u >= 0xac00 && u <= 0xd7a3) || (u >= 0xf900 && u <= 0xfaff) ||
      (u >= 0xfe30 && u <= 0xfe6f) || (u >= 0xff00 && u <= 0xff60) ||
      (u >= 0xffe0 && u <= 0xffe6);
    w += wide ? 2 : 1;
  }
  return w;
}

function renderRow(cells: Val[], widths: number[]): string {
  let out = '';
  widths.forEach((w, i) => {
    const text = i < cells.length ? sOf(cells[i]) : '';
    out += text + ' '.repeat(Math.max(0, w - displayWidth(text)) + 2);
  });
  return out.replace(/\s+$/, '');
}

const COLORS: Record<string, string> = {
  red: '31', green: '32', yellow: '33', blue: '34',
  magenta: '35', cyan: '36', white: '37', gray: '90', grey: '90'
};

def('ui', 'color', '颜色, 文本', '给文本上色（终端 ANSI）：red/green/yellow/blue/magenta/cyan/white/gray', 2, 2, (a) => {
  const code = COLORS[sOf(a[0]).toLowerCase()];
  if (!code) {
    bad(`ui.color 不认识这个颜色：${sOf(a[0])}（可用 red/green/yellow/blue/magenta/cyan/white/gray）`);
  }
  return `\u001b[${code}m${sOf(a[1])}\u001b[0m`;
}, { writes: '__last_str' });
def('ui', 'bar', '当前值, 最大值[, 宽度]', '进度条文本，比如 `[███░░░]`', 2, 3, (a) => {
  const v = num(a[0], 'ui.bar');
  const max = num(a[1], 'ui.bar');
  const width = a.length > 2 ? Math.min(Math.max(intOf(a[2], 'ui.bar'), 4), 80) : 30;
  return makeBar(v, max, width);
}, { writes: '__last_str' });
def('ui', 'title', '文本', '带上下横线的标题', 1, 1, (a) => {
  const t = sOf(a[0]);
  const line = '─'.repeat(Math.max([...t].length, 8));
  return `${line}\n${t}\n${line}`;
}, { writes: '__last_str' });
def('ui', 'clear', '', '清屏（输出这个控制序列就行）', 0, 0, () => '\u001b[2J\u001b[H', { writes: '__last_str' });
def('ui', 'clear_line', '[行数]', '擦掉当前行（给行数就往上擦几行）', 0, 1, (a) => {
  const n = a.length ? Math.min(Math.max(intOf(a[0], 'ui.clear_line'), 0), 100) : 0;
  let s = '';
  for (let i = 0; i <= n; i++) {
    s += '\u001b[2K\r';
    if (n > 0) s += '\u001b[1A';
  }
  return s;
}, { writes: '__last_str' });
def('ui', 'progress', '当前值, 最大值', '一行进度：`[███░░░]  40%`（自带 \\r，可以直接刷新）', 2, 2, (a) => {
  const v = num(a[0], 'ui.progress');
  const max = num(a[1], 'ui.progress');
  const pct = max <= 0 ? 0 : Math.min(Math.max((v / max) * 100, 0), 100);
  return `\r${makeBar(v, max, 30)} ${String(Math.round(pct)).padStart(3, ' ')}%`;
}, { writes: '__last_str' });
def('ui', 'menu', '选项, 提示', '让用户选一项（要读键盘，只能在 VM 里用）', 1, 2, () =>
  bad('ui.menu 需要读键盘输入，只能在 VM 里用（IDE 里按 F5/F6 跑即可）；AOT 编译的程序暂不支持'));
def('ui', 'input', '提示', '让用户输入一行（要读键盘，只能在 VM 里用）', 1, 1, () =>
  bad('ui.input 需要读键盘输入，只能在 VM 里用（IDE 里按 F5/F6 跑即可）；AOT 编译的程序暂不支持'));
def('ui', 'table', '表头, 行列表', '对齐成一张表（中文按 2 格宽算）', 2, 2, (a) => {
  const head = listOf(a[0], 'ui.table');
  const rows = listOf(a[1], 'ui.table');
  let cols = head.length;
  for (const r of rows) if (isList(r)) cols = Math.max(cols, (r as Val[]).length);
  const widths = new Array<number>(cols).fill(0);
  head.forEach((h, i) => { widths[i] = Math.max(widths[i], displayWidth(sOf(h))); });
  for (const r of rows) {
    if (!isList(r)) continue;
    (r as Val[]).forEach((c, i) => { widths[i] = Math.max(widths[i], displayWidth(sOf(c))); });
  }
  const lines = [renderRow(head, widths)];
  for (const r of rows) if (isList(r)) lines.push(renderRow(r as Val[], widths));
  return lines.join('\n');
}, { writes: '__last_str' });
def('ui', 'spinner', '文本', '转圈动画（还没做）', 0, 1, () =>
  bad('ui.spinner 还没做（后台动画需要宿主线程），先用 ui.progress'));

/* ═════════════════════════ 7. re ═════════════════════════ */

/* 迷你正则：和 Rust 侧同一套语法（. * + ? [] [a-z] ^ $ \d \w \s 与分组）。
 * 实现直接用 JS 的正则引擎，但用 sticky（y）标志模拟 Rust 的"在指定位置尝试匹配"，
 * 这样 findall / replace / split 的推进方式跟 Rust 完全一致。 */

/** 模式里有几对括号（跳过转义与字符类） */
function groupCount(pat: string): number {
  let n = 0;
  let inClass = false;
  for (let i = 0; i < pat.length; i++) {
    const c = pat[i];
    if (c === '\\') { i++; continue; }
    if (inClass) { if (c === ']') inClass = false; continue; }
    if (c === '[') { inClass = true; continue; }
    if (c === '(') n++;
  }
  return n;
}

function compileRe(pat: string, who: string): RegExp {
  try {
    return new RegExp(pat, 'y');
  } catch (e) {
    throw bad(`${who}：${e instanceof Error ? e.message : String(e)}`);
  }
}

function matchAt(re: RegExp, text: string, i: number): RegExpExecArray | null {
  re.lastIndex = i;
  return re.exec(text);
}

function findAll(pat: string, text: string, who: string): string[] {
  const re = compileRe(pat, who);
  const oneGroup = groupCount(pat) === 1;
  const out: string[] = [];
  let i = 0;
  while (i <= text.length) {
    const m = matchAt(re, text, i);
    if (m) {
      out.push(oneGroup && m[1] !== undefined ? m[1] : m[0]);
      i += m[0].length === 0 ? 1 : m[0].length;
    } else {
      i++;
    }
  }
  return out;
}

def('re', 'test', '模式, 文本', '文本里有没有匹配', 2, 2, (a) =>
  findAll(sOf(a[0]), sOf(a[1]), 're.test').length > 0, { writes: '__last_cmp' });
def('re', 'findall', '模式, 文本', '找出全部匹配，返回列表（有分组就只要分组里的内容）', 2, 2, (a) =>
  findAll(sOf(a[0]), sOf(a[1]), 're.findall'), { writes: '__last_read' });
def('re', 'replace', '模式, 替换成, 文本', '把匹配到的都换掉', 3, 3, (a) => {
  const re = compileRe(sOf(a[0]), 're.replace');
  const rep = sOf(a[1]);
  const text = sOf(a[2]);
  let out = '';
  let i = 0;
  while (i < text.length) {
    const m = matchAt(re, text, i);
    if (m) {
      out += rep;
      i += m[0].length === 0 ? 1 : m[0].length;
    } else {
      out += text[i];
      i++;
    }
  }
  return out;
}, { writes: '__last_str' });
def('re', 'split', '模式, 文本', '按正则切开', 2, 2, (a) => {
  const re = compileRe(sOf(a[0]), 're.split');
  const text = sOf(a[1]);
  const out: string[] = [];
  let cur = '';
  let i = 0;
  while (i < text.length) {
    const m = matchAt(re, text, i);
    if (m && m[0].length > 0) {
      out.push(cur);
      cur = '';
      i += m[0].length;
    } else {
      cur += text[i];
      i++;
    }
  }
  out.push(cur);
  return out;
}, { writes: '__last_read' });
def('re', 'search', '模式, 文本', '找第一个匹配：{text, start, end, groups}；没有就 null', 2, 2, (a) =>
  searchFirst(sOf(a[0]), sOf(a[1])), { writes: '__last_read' });
def('re', 'match', '模式, 文本', '和 re.search 一样（v1 不做"必须从头匹配"）', 2, 2, (a) =>
  searchFirst(sOf(a[0]), sOf(a[1])), { writes: '__last_read' });
def('re', 'escape', '文本', '把正则里有特殊含义的符号转义', 1, 1, (a) => {
  let out = '';
  for (const c of sOf(a[0])) {
    if ('\\.^$|?*+()[]{}'.includes(c)) out += '\\';
    out += c;
  }
  return out;
}, { writes: '__last_str' });

function searchFirst(pat: string, text: string): Val {
  const hits = findAll(pat, text, 're.search');
  if (!hits.length) return null;
  const hit = hits[0];
  const at = text.indexOf(hit);
  return mkDict(new Map<string, Val>([
    ['text', hit],
    ['start', at < 0 ? 0 : at],
    ['end', (at < 0 ? 0 : at) + hit.length],
    ['groups', [] as Val[]]
  ]));
}

/* ═════════════════════════ 8. net ═════════════════════════ */

function urlEncode(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let out = '';
  for (const b of bytes) {
    const c = String.fromCharCode(b);
    if (/[A-Za-z0-9\-_.~]/.test(c)) out += c;
    else if (b === 0x20) out += '+';
    else out += `%${b.toString(16).toUpperCase().padStart(2, '0')}`;
  }
  return out;
}

function urlDecode(s: string): string {
  const bytes: number[] = [];
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === '+') { bytes.push(0x20); continue; }
    if (c === '%' && i + 2 < s.length) {
      const hex = s.slice(i + 1, i + 3);
      if (/^[0-9A-Fa-f]{2}$/.test(hex)) {
        bytes.push(parseInt(hex, 16));
        i += 2;
        continue;
      }
    }
    for (const b of new TextEncoder().encode(c)) bytes.push(b);
  }
  return new TextDecoder('utf-8', { fatal: false }).decode(new Uint8Array(bytes));
}

async function httpFetch(method: string, url: string, body?: string): Promise<Val> {
  const fn = needPart('http', 'net.fetch');
  return (await fn(method, url, body)) as unknown as Val;
}

def('net', 'fetch', '网址', 'GET 请求，返回响应正文（10 秒超时）', 1, 1, null, {
  writes: '__last_str',
  afn: async (a) => httpFetch('GET', sOf(a[0]))
});
def('net', 'post', '网址, 内容', 'POST 请求，把内容发出去并返回响应正文', 2, 2, null, {
  writes: '__last_str',
  afn: async (a) => httpFetch('POST', sOf(a[0]), sOf(a[1]))
});
def('net', 'encode', '文本', 'URL 编码', 1, 1, (a) => urlEncode(sOf(a[0])), { writes: '__last_str' });
def('net', 'decode', '文本', 'URL 解码', 1, 1, (a) => urlDecode(sOf(a[0])), { writes: '__last_str' });
def('net', 'query', '网址', '把 `?a=1&b=2` 解析成字典', 1, 1, (a) => {
  const url = sOf(a[0]);
  const q = url.includes('?') ? url.slice(url.indexOf('?') + 1) : '';
  const d = new Map<string, Val>();
  for (const pair of q.split('&')) {
    if (!pair) continue;
    const at = pair.indexOf('=');
    const k = urlDecode(at < 0 ? pair : pair.slice(0, at));
    const v = urlDecode(at < 0 ? '' : pair.slice(at + 1));
    d.set(k, v);
  }
  return mkDict(d);
}, { writes: '__last_read' });
def('net', 'ip', '', '本机对外地址', 0, 0, null, {
  writes: '__last_str',
  afn: async () => (await host()).ip as unknown as Val
});
def('net', 'download', '网址, 保存到', '下载网页并写进文件', 2, 2, null, {
  afn: async (a) => {
    const body = await httpFetch('GET', sOf(a[0]));
    const text = typeof body === 'string' ? body : toStr(body);
    return (await needFs().write(resolvePath(sOf(a[1])), text)) as unknown as Val;
  }
});
def('net', 'request', '方法, 网址[, 内容]', '发一次请求，返回 {code, body, headers}', 2, 3, null, {
  afn: async (a) => {
    const method = sOf(a[0]).toUpperCase();
    const url = sOf(a[1]);
    const body = method === 'GET' ? await httpFetch('GET', url) : await httpFetch(method, url, a.length > 2 ? sOf(a[2]) : '');
    return mkDict(new Map<string, Val>([
      ['code', 200],
      ['body', body],
      ['headers', mkDict(new Map<string, Val>())]
    ]));
  }
});
def('net', 'listen', '端口', '起一个本地 HTTP 服务（v1 没做）', 1, 1, () =>
  bad('net.listen 需要在宿主里起 HTTP 服务，v1 先没做（要用可以做成本地脚本 + 外部服务）'));
def('net', 'server_close', '', '关掉 net.listen 起的服务（v1 没做）', 0, 0, () =>
  bad('net.server_close 需要在宿主里起 HTTP 服务，v1 先没做（要用可以做成本地脚本 + 外部服务）'));

/* ───────────────────────── 导出 ───────────────────────── */

/** 给 builtins.ts 合并进内建总表（补全 / 悬停 / 求值都用它） */
export const STDLIB_SPECS: StdlibSpec[] = SPECS;

/** 完整名 → 规格。老名字（readfile / listdir / fetch …）转发到这里，
 *  保证「新旧两个名字」永远是同一份实现，不会一处修了另一处忘。 */
export const STDLIB_MAP = new Map<string, StdlibSpec>(SPECS.map((x) => [x.name, x]));
