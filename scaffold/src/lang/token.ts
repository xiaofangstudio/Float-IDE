/* CW Script 词法层：把一行源码切成 Token 流。
 *
 * 与旧版正则解析的区别：这里不"猜整行形状"，而是先切词、再由语法层组合，
 * 因此 `for i in range(n)` 里的 n 可以是任意表达式，函数参数、成员访问也都能递归解析。
 */

export type TokKind = 'num' | 'str' | 'ident' | 'kw' | 'op' | 'punc' | 'eof';

export type Token = {
  k: TokKind;
  /** 原始文本（字符串 token 已去引号并转义） */
  v: string;
  line: number;
  /** 1 基列号 */
  col: number;
  endCol: number;
  /** 数字字面量的值 */
  num?: number;
};

/** 只保留真正影响语法的关键字；new 是前缀运算符 */
const KEYWORDS = new Set(['new', 'class', 'extends', 'function', 'fn', 'return', 'import', 'super']);

const OPERATORS = [
  '===', '!==', '**=', '<<=', '>>=',
  '==', '!=', '<=', '>=', '&&', '||', '**', '<<', '>>', '+=', '-=', '*=', '/=', '%=',
  '++', '--', '=>',
  '+', '-', '*', '/', '%', '<', '>', '!', '~', '&', '|', '^', '=', '?', ':'
];

const PUNCS = '()[]{},.;';

export class LexError extends Error {
  constructor(message: string, public line: number, public col: number) {
    super(message);
    this.name = 'LexError';
  }
}

export const isIdStart = (c: string): boolean =>
  /[A-Za-z_]/.test(c) || (c >= '\u4e00' && c <= '\u9fa5') || (c >= '\u3400' && c <= '\u4dbf');

export const isIdPart = (c: string): boolean => isIdStart(c) || /[0-9]/.test(c);

/** 切一行（不含换行符）为 Token 序列。抛 LexError 时调用方转成诊断。 */
export function tokenizeLine(src: string, line: number, startCol = 1): Token[] {
  const out: Token[] = [];
  let i = 0;
  const push = (k: TokKind, v: string, col: number, endCol: number, num?: number): void => {
    out.push({ k, v, line, col, endCol, num });
  };

  while (i < src.length) {
    const c = src[i];

    if (c === ' ' || c === '\t' || c === '\u3000') { i++; continue; }
    if (c === '\r') { i++; continue; }

    // 行注释：// 之后全部丢弃（调用方需要原文时自己切）
    if (c === '/' && src[i + 1] === '/') break;

    // 字符串
    if (c === '"' || c === "'") {
      const quote = c;
      const startCol2 = startCol + i;
      let j = i + 1;
      let text = '';
      let depth = 0; // {…} 插值里允许出现引号
      while (j < src.length && !(src[j] === quote && depth === 0)) {
        if (src[j] === '{') depth++;
        else if (src[j] === '}' && depth > 0) depth--;
        if (src[j] === '\\' && j + 1 < src.length) {
          const e = src[j + 1];
          text += e === 'n' ? '\n'
            : e === 't' ? '\t'
            : e === 'r' ? '\r'
            : e === '0' ? '\0'
            : e === '\\' ? '\\'
            : e === '"' ? '"'
            : e === "'" ? "'"
            : (e === '{' || e === '}') ? '\\' + e
            : e;
          j += 2;
          continue;
        }
        text += src[j];
        j++;
      }
      if (j >= src.length) throw new LexError('字符串没有结束引号', line, startCol2);
      push('str', text, startCol2, startCol + j + 1);
      i = j + 1;
      continue;
    }

    // 数字
    if (/[0-9]/.test(c) || (c === '.' && /[0-9]/.test(src[i + 1] ?? ''))) {
      const start = i;
      if (c === '0' && (src[i + 1] === 'x' || src[i + 1] === 'X')) {
        i += 2;
        while (i < src.length && /[0-9a-fA-F_]/.test(src[i])) i++;
        const raw = src.slice(start, i).replace(/_/g, '');
        push('num', raw, startCol + start, startCol + i, Number(raw));
        continue;
      }
      while (i < src.length && /[0-9_]/.test(src[i])) i++;
      if (src[i] === '.' && /[0-9]/.test(src[i + 1] ?? '')) {
        i++;
        while (i < src.length && /[0-9_]/.test(src[i])) i++;
      }
      if (src[i] === 'e' || src[i] === 'E') {
        const save = i;
        i++;
        if (src[i] === '+' || src[i] === '-') i++;
        if (/[0-9]/.test(src[i] ?? '')) { while (i < src.length && /[0-9]/.test(src[i])) i++; }
        else i = save;
      }
      const raw = src.slice(start, i).replace(/_/g, '');
      push('num', raw, startCol + start, startCol + i, Number(raw));
      continue;
    }

    // 标识符 / 关键字（含中文）
    if (isIdStart(c)) {
      const start = i;
      while (i < src.length && isIdPart(src[i])) i++;
      const word = src.slice(start, i);
      push(KEYWORDS.has(word) ? 'kw' : 'ident', word, startCol + start, startCol + i);
      continue;
    }

    // 运算符（长的优先）
    let matched = '';
    for (const op of OPERATORS) {
      if (src.startsWith(op, i)) { matched = op; break; }
    }
    if (matched) {
      push('op', matched, startCol + i, startCol + i + matched.length);
      i += matched.length;
      continue;
    }

    if (PUNCS.includes(c)) {
      push('punc', c, startCol + i, startCol + i + 1);
      i++;
      continue;
    }

    throw new LexError(`无法识别的字符「${c}」`, line, startCol + i);
  }

  out.push({ k: 'eof', v: '', line, col: startCol + src.length, endCol: startCol + src.length });
  return out;
}

/** 便于阅读的 Token 描述，用于错误提示 */
export function describeTok(t: Token): string {
  if (t.k === 'eof') return '行尾';
  if (t.k === 'str') return `字符串「${t.v}」`;
  return `「${t.v}」`;
}

/** 取一行里 `//` 之前的有效代码（跳过字符串内的 //） */
export function codePart(line: string): { code: string; comment: string | null } {
  let inStr: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === '\\') { i++; continue; }
      if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'") { inStr = c; continue; }
    if (c === '/' && line[i + 1] === '/') {
      return { code: line.slice(0, i), comment: line.slice(i + 2) };
    }
  }
  return { code: line, comment: null };
}
