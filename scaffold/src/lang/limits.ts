/**
 * 语言运行的全部上限，集中在一个地方（与 Rust 内核 float-lang/src/limits.rs 一一对应）。
 * 改这里的值 = 同时改 F5（编译）/ F6（解释）两条通道的行为，注意和 Rust 侧同步。
 * 报错信息统一用「当前 X（上限 Y）」格式，让用户知道卡在哪条线上、还差多少。
 */
export const LIMITS = {
  /** 函数调用（递归）最大深度。超了报「递归太深」，不让 JS 爆栈。 */
  callDepth: 400,
  /** import 最大嵌套层数。链子太长多半是库互相引用，该报错而不是吃栈。 */
  importDepth: 100,
  /** 单次运行最多执行多少步（约等于十几秒计算量）：真死循环抓得住，正经大计算不误杀。 */
  stepBudget: 50_000_000,
  /** 宏别名（alias / #define）最多套多少层。 */
  aliasDepth: 16,
  /* ── 内存护栏 ──
   * 只有步数上限拦不住 `while true { add 1 to xs }`：50M 步足够把列表撑到几个 G，
   * 整机开始 swap（实测把资源管理器都拖崩过）。容器/字符串到线就明确报错。 */
  /** 单个列表最多多少项。 */
  listItems: 1_000_000,
  /** 单个字典最多多少个键。 */
  dictItems: 500_000,
  /** 单个字符串最多多少字符（与 Rust 侧 16MB 字节上限对应）。 */
  stringBytes: 16 * 1024 * 1024,
  /** 单次运行最多输出多少字节（Rust 侧 MAX_OUTPUT 同值）。 */
  outputBytes: 32 * 1024 * 1024,
} as const;

/** 统一措辞：让用户知道卡在哪条线上、还差多少。和 Rust 内核 limits.rs 一字不差。 */
export function growError(what: string, cur: number, max: number): Error {
  return new Error(`${what} 太大（当前 ${cur}，上限 ${max}），是不是循环里忘了停？`);
}
export function guardList(cur: number, add = 1): void {
  const n = cur + add;
  if (n > LIMITS.listItems) throw growError('列表', n, LIMITS.listItems);
}
export function guardDict(cur: number, add = 1): void {
  const n = cur + add;
  if (n > LIMITS.dictItems) throw growError('字典', n, LIMITS.dictItems);
}
export function guardString(cur: number, add = 1): void {
  const n = cur + add;
  if (n > LIMITS.stringBytes) throw growError('字符串', n, LIMITS.stringBytes);
}
