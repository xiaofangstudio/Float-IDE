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
  '++', '--', '//', '=>',
  '+', '-', '*', '/', '%', '<', '>', '!', '~', '&', '|', '^', '=', '?', ':'
];

const PUNCS = '()[]{},.;';

export class LexError extends Error {
  constructor(message: string, public line: number, public col: number) {
    super(message);
    this.name = 'LexError';
  }
}

/* 中文输入法打出来的全角符号：**代码区一律按半角处理**，字符串内容不动。
 * 不处理的话，用户写 `print（"a"）` 只会看到「无法识别的字符「（」」——
 * 从屏幕上看两个括号几乎一模一样，根本猜不到是全角。
 * 只映射"代码里一定当符号用"的字符；全角数字/全角字母不映射（那些更可能是笔误）。 */
const FULLWIDTH: Record<string, string> = {
  '＋': '+', '－': '-', '＊': '*', '／': '/', '％': '%', '＝': '=', '＜': '<', '＞': '>',
  '！': '!', '＆': '&', '｜': '|', '＾': '^', '～': '~', '〜': '~',
  '（': '(', '）': ')', '［': '[', '］': ']', '｛': '{', '｝': '}',
  '，': ',', '；': ';', '：': ':', '？': '?', '。': '.',
  '“': '"', '”': '"', '‘': "'", '’': "'"
};

/** 全角符号 → 半角；其他字符原样返回 */
export const normChar = (c: string): string => FULLWIDTH[c] ?? c;

/** 从 at 开始，归一化后是否等于 want（多字符运算符也能跨全角匹配） */
const startsWithNorm = (s: string, at: number, want: string): boolean => {
  if (at + want.length > s.length) return false;
  for (let k = 0; k < want.length; k++) {
    if (normChar(s[at + k]) !== want[k]) return false;
  }
  return true;
};

/** 全角区 / 中文标点区：报错时用来给"换成半角"的提示 */
const isFullwidthRange = (c: string): boolean => /[\uFF00-\uFFEF\u3000-\u303F]/.test(c);

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
    const raw = src[i];
    const c = normChar(raw);   // 全角符号在这里归一成半角，字符串内容不受影响

    if (c === ' ' || c === '\t' || c === '\u3000') { i++; continue; }
    if (c === '\r') { i++; continue; }

    // 注意：`//` 不是注释，是整除运算符（注释在 codePart 里按 `/*` 剥掉）

    // 字符串
    if (c === '"' || c === "'") {
      const quote = c;
      const startCol2 = startCol + i;
      let j = i + 1;
      let text = '';
      let depth = 0; // {…} 插值里允许出现引号
      while (j < src.length && !(normChar(src[j]) === quote && depth === 0)) {
        if (normChar(src[j]) === '{') depth++;
        else if (normChar(src[j]) === '}' && depth > 0) depth--;
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
      // and / or / not 是 && / || / ! 的关键字写法（AST 层同一个节点，短路语义一致）。
      // 前面是 '.' 时不转换：obj.not / obj.and 是普通成员名。
      const wordOp = word === 'and' ? '&&' : word === 'or' ? '||' : word === 'not' ? '!' : null;
      const prev = out[out.length - 1];
      if (wordOp && !(prev && prev.k === 'op' && prev.v === '.')) {
        push('op', wordOp, startCol + start, startCol + i);
        continue;
      }
      push(KEYWORDS.has(word) ? 'kw' : 'ident', word, startCol + start, startCol + i);
      continue;
    }

    // 运算符（长的优先）。按归一化字符逐位比，`＝` 这类全角写法也能匹配 `=`
    let matched = '';
    for (const op of OPERATORS) {
      if (startsWithNorm(src, i, op)) { matched = op; break; }
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

    throw new LexError(
      `无法识别的字符「${raw}」${isFullwidthRange(raw) ? '——这是全角/中文标点，代码里要写半角' : ''}`,
      line,
      startCol + i
    );
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

/** 取一行的有效代码，把 `/*` 注释剥掉（跳过字符串里的 `/*`）。
 *  注释只有一种写法：`/*` 开头、到行尾结束（不用闭合）。
 *  老写法 `--` 注释已取消：`--` 现在只可能是自减运算符，
 *  撞上老注释的形状（左边行首/空白、右边空白/行尾）就报错并给出改法。
 *  legacyLine 传行号时才报错（编辑器上下文推断传 0，不在这里报）。
 *  **`//` 不是注释**，它是整除运算符。 */
export function codePart(line: string, legacyLine = 0): { code: string; comment: string | null } {
  let inStr: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === '\\') { i++; continue; }
      if (normChar(c) === inStr) inStr = null;
      continue;
    }
    /* 中文引号也要当成字符串边界，否则 `print(“a /* b”)` 会把注释切在半截 */
    const n = normChar(c);
    if (n === '"' || n === "'") { inStr = n; continue; }
    if (n === '/' && normChar(line[i + 1] ?? '') === '*') {
      return { code: line.slice(0, i), comment: line.slice(i + 2) };
    }
    if (n === '-' && normChar(line[i + 1] ?? '') === '-') {
      const leftOk = i === 0 || /\s/.test(line[i - 1]);
      const after = line[i + 2];
      const rightOk = after === undefined || /\s/.test(after);
      if (leftOk && rightOk) {
        if (legacyLine > 0) {
          throw new LexError(
            '注释的 -- 写法已取消：注释改成 /*（从 /* 写到行尾，不用闭合）',
            legacyLine,
            i + 1
          );
        }
        // legacyLine = 0：编辑器上下文路径，不当注释、也不报错，整段当代码
        return { code: line, comment: null };
      }
    }
  }
  return { code: line, comment: null };
}

/* ─────────────────────── 块注释预处理 ───────────────────────
 *
 * 注释有两种写法（2026-11-17 新增块注释）：
 *   「/* 文字」        行注释，到行尾，不用闭合（codePart 逐行剥，行为不变）
 *   「/** 文字 ＊/」   块注释，跨行；闭合符可以省——省了就一口气吃到文件末尾（给 warning）
 *
 * 三条边界规则：
 *   1. 块注释开头必须顶行（前面只允许空白）。行内写它直接报错——
 *      行内已经有行注释了，再让块注释行内开块，会把半行代码吞掉。
 *   2. 不闭合不报错，但发 warning：块注释从第 N 行开始没有闭合，已读到文件末尾。
 *      既不拦你，也不让你在静默里丢一屏代码。
 *   3. 不支持嵌套，块里的第二个块注释开头当普通文字；块里的字符串也不特殊，
 *      星杠一到就闭合。
 *
 * 实现：按字符扫整份源码，字符串感知，把块注释区间的字符替换成空格
 * （行列号原样保住，诊断和续行合并都不受影响），再交给原来的逐行流程。
 */

export type BlockCommentMark = { line: number; col: number; message: string };

export type BlockCommentScan = {
  /** 预处理后的源码：块注释区间的字符替换成空格，换行原样保留 */
  text: string;
  warnings: BlockCommentMark[];
  errors: BlockCommentMark[];
};

export function stripBlockComments(src: string): BlockCommentScan {
  const warnings: BlockCommentMark[] = [];
  const errors: BlockCommentMark[] = [];
  const lines = src.split('\n');
  const outLines: string[] = [];
  let inBlock = false;
  let blockStart: { line: number; col: number } | null = null;

  for (let li = 0; li < lines.length; li++) {
    const line = lines[li];
    const cs = Array.from(line);
    // 先放原字符，块注释区间再逐位覆写成空格——没写到的位置保持原样
    const buf: string[] = cs.slice();
    let i = 0;
    // 本行在块注释开始之前是否出现过非空白字符（顶行判定用）
    let sawContent = false;

    while (i < cs.length) {
      const c = cs[i];
      if (inBlock) {
        // 块里只找 */（字符串不特殊、/** 不嵌套），找到为止；找不到整行都是注释
        if (c === '*' && cs[i + 1] === '/') {
          buf[i] = ' ';
          buf[i + 1] = ' ';
          i += 2;
          inBlock = false;
          blockStart = null;
          continue; // */ 后面同一行还能有代码，继续扫
        }
        buf[i] = ' ';
        i += 1;
        continue;
      }
      // 字符串感知（中文引号和 codePart 一样归一）：字符串里的块注释开头不算
      {
        const n = normChar(c);
        if (n === '"' || n === "'") {
          const q = n;
          buf[i] = c;
          i += 1;
          while (i < cs.length) {
            if (cs[i] === '\\') { buf[i] = cs[i]; if (i + 1 < cs.length) { buf[i + 1] = cs[i + 1]; } i += 2; continue; }
            buf[i] = cs[i];
            if (normChar(cs[i]) === q) { i += 1; break; }
            i += 1;
          }
          sawContent = true;
          continue;
        }
      }
      if (c === '/' && cs[i + 1] === '*' && cs[i + 2] === '*') {
        const topOfLine = !sawContent;
        if (topOfLine) {
          // 顶行开块：/** 三个字符替换成空格，进入块状态
          buf[i] = ' '; buf[i + 1] = ' '; buf[i + 2] = ' ';
          i += 3;
          inBlock = true;
          blockStart = { line: li + 1, col: i - 2 };
          continue;
        }
        // 行内 /**：报错，这里不再替换——后面的 codePart 会把它当行注释剥到行尾
        errors.push({
          line: li + 1,
          col: i + 1,
          message: '行内的 /** 不支持开块注释（会把半行代码吞掉）——顶格写 /** 才是块注释，行内注释用 /* 到行尾'
        });
        sawContent = true;
        i += 3;
        continue;
      }
      if (c === '/' && cs[i + 1] === '*') {
        // 行内 /* 行注释：留给 codePart 处理，这里只标记"本行有内容"
        sawContent = true;
        buf[i] = c;
        i += 1;
        continue;
      }
      if (!/\s/.test(c)) sawContent = true;
      buf[i] = c;
      i += 1;
    }
    outLines.push(buf.join(''));
  }

  if (inBlock && blockStart) {
    warnings.push({
      line: blockStart.line,
      col: blockStart.col,
      message: `块注释从第 ${blockStart.line} 行开始没有闭合（*/），已读到文件末尾`
    });
  }
  return { text: outLines.join('\n'), warnings, errors };
}
