/**
 * 语言运行上限。
 *
 * 当前策略：不设人为上限。递归、容器、字符串、输出、执行步数都不拦截；
 * 计算要跑多久就跑多久，内存不够由宿主/系统自己处理。
 * 常量与 guard 函数保留只为兼容调用点，guard 恒通过。
 */
export const LIMITS = {
  callDepth: Number.MAX_SAFE_INTEGER,
  importDepth: Number.MAX_SAFE_INTEGER,
  stepBudget: Number.MAX_SAFE_INTEGER,
  aliasDepth: Number.MAX_SAFE_INTEGER,
  listItems: Number.MAX_SAFE_INTEGER,
  dictItems: Number.MAX_SAFE_INTEGER,
  stringBytes: Number.MAX_SAFE_INTEGER,
  outputBytes: Number.MAX_SAFE_INTEGER,
} as const;

export function growError(what: string, cur: number, max: number): Error {
  return new Error(`${what} 太大（当前 ${cur}，上限 ${max}）`);
}

export function guardList(_cur: number, _add = 1): void {}

export function guardDict(_cur: number, _add = 1): void {}

export function guardString(_cur: number, _add = 1): void {}
