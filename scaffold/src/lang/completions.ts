/* 补全候选的"纯逻辑"：不碰 monaco，方便单测。
 *
 * 三条铁规矩（都是被用户骂出来的）：
 * 1. **绝不自动补括号**。以前函数补成 `名字($0)`，一按回车就多出个 `(`，
 *    看起来像语法错误，还很误导。现在一律只插名字，参数放在 labelDetail 里看。
 * 2. **什么都要能补到**：本文件变量/函数/类/字段、import 进来的、类型关键字、
 *    内置函数、老式指令、特殊变量、模板片段，一条都不能少。
 * 3. **优先级要分清**：正在用的（本文件符号）排最前，其次类型/关键字，
 *    再是内置，最后才是模板片段。同档里"前缀命中"的排前面。
 */

import type { Analysis } from './analyze';
import type { CursorContext } from './context';
import { BUILTIN_METHODS, INSTRUCTIONS, KEYWORDS, SNIPPETS, SPECIAL_VARS, TYPE_WORDS } from './spec';
import { builtinList } from './builtins';
import { isNamespace } from './stdlib';

export type CompletionKind =
  | 'variable' | 'function' | 'class' | 'method' | 'field'
  | 'keyword' | 'type' | 'constant' | 'snippet' | 'value';

export type Completion = {
  label: string;
  kind: CompletionKind;
  /** 真正插进编辑器的文本。除了模板片段，都等于 label */
  insertText: string;
  /** 列表右侧的灰色小字（一般是签名） */
  labelDetail: string;
  detail: string;
  doc: string;
  /** Monaco 用它排序：档位 + 前缀是否命中 + 名字 */
  sortText: string;
};

export type PluginContrib = {
  snippets?: { prefix: string; body: string; description?: string }[];
  keywords?: { label: string; detail?: string; insert?: string }[];
};

export type CompletionInput = {
  /** 已经打出来的词（前缀），用来排序和过滤 */
  word: string;
  analysis: Analysis;
  /** 光标语义（context.ts 统一算的：成员 / 起名字 / 字符串 / 注释 / 代码） */
  context: CursorContext;
  plugins?: PluginContrib;
};

const BUILTINS = builtinList();

/** 档位：数字越小越靠前 */
const TIER = {
  member: 0,
  local: 1,
  type: 2,
  keyword: 3,
  builtin: 4,
  instruction: 5,
  special: 1,
  plugin: 6,
  snippet: 9
} as const;

/** 前缀命中排前面 */
function sortKey(tier: number, label: string, word: string): string {
  const hit = word && label.toLowerCase().startsWith(word.toLowerCase()) ? '0' : '1';
  return `${tier}${hit}${label.toLowerCase()}`;
}

function paramsOf(p: { name: string }[]): string {
  return p.map((x) => x.name).join(', ');
}

function fieldsOf(a: Analysis, cls: string): { name: string; type: string | null }[] {
  const c = a.classes.find((x) => x.name === cls);
  if (!c) return [];
  const base = c.base ? fieldsOf(a, c.base) : [];
  const own = c.fields.map((f) => ({ name: f.name, type: f.type }));
  const out = [...base];
  for (const f of own) {
    const at = out.findIndex((x) => x.name === f.name);
    if (at >= 0) out[at] = f;
    else out.push(f);
  }
  return out;
}

function methodsOf(a: Analysis, cls: string): { name: string; params: { name: string }[]; retType: string | null }[] {
  const c = a.classes.find((x) => x.name === cls);
  if (!c) return [];
  const base = c.base ? methodsOf(a, c.base) : [];
  const own = c.methods.map((m) => ({ name: m.name, params: m.params, retType: m.retType }));
  const out = [...base];
  for (const m of own) {
    const at = out.findIndex((x) => x.name === m.name);
    if (at >= 0) out[at] = m;
    else out.push(m);
  }
  return out;
}

export function buildCompletions(input: CompletionInput): Completion[] {
  const { analysis: a, word } = input;
  const out: Completion[] = [];
  const seen = new Set<string>();

  const push = (
    label: string, kind: CompletionKind, tier: number,
    detail: string, doc: string, labelDetail = '', insertText?: string
  ): void => {
    if (seen.has(label)) return;
    seen.add(label);
    out.push({
      label, kind,
      // 只有片段会带模板；其余一律只插名字（不自动加括号）
      insertText: insertText ?? label,
      labelDetail,
      detail,
      doc,
      sortText: sortKey(tier, label, word)
    });
  };

  /* ── 成员补全：p. / self.（目标和类型由 context 统一给出） ── */
  if (input.context.kind === 'member') {
    const target = input.context.object;
    const cls = input.context.cls;
    /* `os.` / `data.` 这种标准库前缀：只列这个库的函数，
     * 不要混进 str/list 的方法（`os.push()` 看着就离谱）。 */
    if (isNamespace(target)) {
      const prefix = `${target}.`;
      for (const b of BUILTINS) {
        if (!b.name.startsWith(prefix)) continue;
        push(b.name.slice(prefix.length), 'function', TIER.member, b.sig, b.doc,
          b.sig.slice(b.name.length) || '()');
      }
      return out;
    }
    if (cls) {
      for (const f of fieldsOf(a, cls)) {
        push(f.name, 'field', TIER.member, `${cls}.${f.name}${f.type ? ` · ${f.type}` : ''}`,
          `字段 \`${f.name}\`${f.type ? `，类型 ${f.type}` : ''}`);
      }
      for (const m of methodsOf(a, cls)) {
        push(m.name, 'method', TIER.member + 1 < TIER.local ? TIER.member : TIER.local,
          `${cls}.${m.name}(${paramsOf(m.params)})`,
          `方法 \`${m.name}(${paramsOf(m.params)})\`${m.retType ? ` → ${m.retType}` : ''}`,
          `(${paramsOf(m.params)})`);
      }
    }
    /* 内建方法：基本类型（str/list/dict/ptr）只给对应那几类，免得 `.` 之后
     * 一下子甩出三四十条不相干的方法；类实例则全列出来（排在自身方法后面），
     * 因为它可能被当成参数传给内建函数。 */
    const wantOwners = cls ? null : ownersFor(input.context.type);
    for (const m of BUILTIN_METHODS) {
      if (wantOwners && !wantOwners.has(m.owner)) continue;
      push(m.name, 'method', TIER.builtin, `${m.owner}.${m.name}()`, m.desc, '()');
    }
    return out;
  }

  /* ── 本文件符号 ── */
  for (const v of a.vars) {
    push(v.name, 'variable', TIER.local,
      v.cls ? v.cls : v.type,
      `变量 \`${v.name}\`${v.cls ? `，类型 ${v.cls}` : `，类型 ${v.type}`}（第 ${v.line} 行）`);
  }
  for (const f of a.funcs) {
    if (f.owner) continue; // 方法不在这里列，走 `对象.`
    push(f.name, 'function', TIER.local,
      `${f.name}(${paramsOf(f.params)})${f.retType ? ` → ${f.retType}` : ''}`,
      `函数 \`${f.name}(${paramsOf(f.params)})\`${f.retType ? ` → ${f.retType}` : ''}（第 ${f.line} 行）`,
      `(${paramsOf(f.params)})`);
  }
  for (const c of a.classes) {
    push(c.name, 'class', TIER.type,
      `class ${c.name}${c.base ? ` : ${c.base}` : ''}`,
      `类 \`${c.name}\`（第 ${c.line} 行）；用 new ${c.name}() 创建实例`);
  }
  for (const sv of SPECIAL_VARS) {
    push(sv.name, 'constant', TIER.special, '特殊变量', `特殊变量 \`${sv.name}\`：${sv.desc}`);
  }

  /* ── 类型与关键字 ── */
  for (const t of TYPE_WORDS) {
    push(t, 'type', TIER.type, '类型', `类型 \`${t}\``);
  }
  for (const k of KEYWORDS) {
    push(k, 'keyword', TIER.keyword, '关键字', `关键字 \`${k}\``);
  }

  /* ── 内置函数 ── */
  for (const b of BUILTINS) {
    push(b.name, 'function', TIER.builtin, b.sig, b.doc, b.sig.slice(b.name.length) || '()');
  }

  /* ── 老式指令（写文件 / 调试 这类） ── */
  for (const ins of INSTRUCTIONS) {
    const first = ins.name.split(' ')[0];
    if (!first) continue;
    push(first, 'keyword', TIER.instruction, ins.args,
      `${ins.desc}${ins.result ? `\n\n结果写入 \`${ins.result}\`` : ''}`);
  }

  /* ── 插件贡献 ── */
  for (const kw of input.plugins?.keywords ?? []) {
    push(kw.label, 'function', TIER.plugin, kw.detail ?? '', kw.detail ?? kw.label);
  }

  /* ── 模板片段放最后：它们是"整套骨架"，只有主动选才该生效 ── */
  for (const sn of input.plugins?.snippets ?? []) {
    push(sn.prefix, 'snippet', TIER.snippet, `片段 · ${sn.description ?? sn.prefix}`,
      sn.description ?? sn.prefix, '', sn.body);
  }
  for (const sn of SNIPPETS) {
    // 和关键字同名（function / class / while）时加个后缀，
    // 免得列表里两条一模一样的名字：一条只插名字，一条插整套模板
    const label = seen.has(sn.label) ? `${sn.label}（模板）` : sn.label;
    push(label, 'snippet', TIER.snippet, sn.detail, sn.detail, '', sn.body);
  }

  return out;
}

/** 变量属于哪个"内建类型家族"，用来过滤内建方法 */
/** 变量属于哪个"内建类型家族"，用来过滤内建方法（类型由 context 推断好传进来） */
function ownersFor(t: string | null): Set<string> {
  if (t === 'str') return new Set(['str']);
  if (t === 'list') return new Set(['list']);
  if (t === 'dict') return new Set(['dict']);
  if (t === 'ptr') return new Set(['ptr']);
  // 类型不明（var / 参数 / 猜不出来）：全都列，别让人补不到
  return new Set(['str', 'list', 'dict', 'ptr']);
}
