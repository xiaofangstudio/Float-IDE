import * as monaco from 'monaco-editor';
import { INSTRUCTIONS, SPECIAL_VARS, KEYWORDS, TYPE_WORDS, SNIPPETS, BUILTIN_METHODS, LANG_ID } from './spec';
import { buildCompletions, capCompletions, completionPosition, visibleCompletions, type CompletionKind } from './completions';
import { contextAt } from './context';
import { isTauriEnv } from '../core/tauri';
import { loadSettings, SETTINGS_EVENT } from '../ui/settings';
import { predictNext, scopeVariables, smartRank, smartCorpus, type SmartModel } from './smartComplete';

/* ── 本地超小补全模型（智能补全）────────────────────────────────────
 * 只用工程里打开的 .co/.cw 学一张内存里的计数表，同一档内按上下文重排候选；
 * 不联网、不上传、不含源码原文，只有词对计数。
 *
 * 关键：**不再每敲一个键就全量重训**。每个文件一张自己的表，
 * 改哪个就只重算哪个（空闲 220 ms 后才算，打字过程中用上一版，手感优先），
 * 单文件成本 ~1 ms，大工程也不会把 CPU 拉起来。 */
let smartEnabled = loadSettings().smartComplete;
let inlineEnabled = loadSettings().inlineSuggest;
if (typeof window !== 'undefined') {
  window.addEventListener(SETTINGS_EVENT, () => {
    const s = loadSettings();
    smartEnabled = s.smartComplete;
    inlineEnabled = s.inlineSuggest;
  });
}

/** 把打开的 Copper 文件登记进增量语料库（内容没变就是一次字符串比较，零解析） */
function feedCorpus(): void {
  for (const m of monaco.editor.getModels()) {
    const name = decodeURIComponent(m.uri.path);
    if (!/\.(co|cw)$/i.test(name)) continue;
    const text = modelText(m).text;
    smartCorpus.setText(name.replace(/\\/g, '/'), text.length > 120_000 ? text.slice(0, 120_000) : text);
  }
}

function currentSmartModel(): SmartModel {
  feedCorpus();
  return smartCorpus.get();
}

/* 插件贡献的片段与关键词。插件系统启动后调用 setPluginContributions 注入，
 * 补全里和内置片段一视同仁 —— 这样 plugin.json 里声明的 snippets 是真的能用，不是摆设。 */
let pluginSnippets: { prefix: string; body: string; description?: string }[] = [];
let pluginKeywords: { label: string; detail?: string; insert?: string }[] = [];

export function setPluginContributions(
  snippets: { prefix: string; body: string; description?: string }[],
  keywords: { label: string; detail?: string; insert?: string }[]
): void {
  pluginSnippets = snippets;
  pluginKeywords = keywords;
}
import { builtinList } from './builtins';
import {
  analyze, classAtLine, classFields, classMethods,
  type Analysis, type OutlineItem
} from './analyze';
import { parseProgram } from './parse';
import { toplevelNames } from './sema';
import { diagnoseAsync } from './diagnoseClient';
import type { Diagnostic } from './ast';

/* Copper（原 CW Script）编辑器支持：Monarch 高亮、AST 驱动的补全 / 悬停 / 签名提示 / 实时诊断 / 大纲。
 *
 * 0.1.0 的补全靠正则扫源码；这里全部换成 analyze() 产出的符号表，
 * 因此类、方法、字段、参数都能真正参与提示。
 */

export { LANG_ID };
export type { OutlineItem };

const BUILTINS = builtinList();

/* ───────────────────────────── 分析缓存 ───────────────────────────── */

const cache = new WeakMap<monaco.editor.ITextModel, { version: number; analysis: Analysis }>();

/** 模型文本 + 行数组，按版本缓存。
 *  以前一次补全里 getValue() 调好几次、split('\n') 再切一遍 150 KB，
 *  每击键几毫秒纯浪费；现在同一版本只取一次。 */
const textCache = new WeakMap<monaco.editor.ITextModel, { version: number; text: string; lines: string[] }>();

function modelText(model: monaco.editor.ITextModel): { text: string; lines: string[] } {
  const version = model.getVersionId();
  const hit = textCache.get(model);
  if (hit && hit.version === version) return hit;
  const text = model.getValue();
  const entry = { version, text, lines: text.split('\n') };
  textCache.set(model, entry);
  return entry;
}

/* ── 工程总表（由索引那边灌进来） ──
 * 用户的规矩：先本文件 → 再 import 链一层层翻；整个工程里已经定义过的名字，
 * 就不该在某个文件里被标「没有定义」。所以这里留一个工程级的名字总表，
 * 诊断时和 import 链的结果合起来用。 */
let projectNameTable: Set<string> | null = null;

export function setProjectNames(names: Iterable<string> | null): void {
  projectNameTable = names ? new Set(names) : null;
}

function mergedNames(model: monaco.editor.ITextModel): Set<string> | undefined {
  const imported = importNamesOf(model);
  if (!projectNameTable) return imported;
  if (!imported) return new Set(projectNameTable);
  const merged = new Set(projectNameTable);
  for (const n of imported) merged.add(n);
  return merged;
}

export function analysisOf(model: monaco.editor.ITextModel): Analysis {
  const hit = cache.get(model);
  const version = model.getVersionId();
  if (hit && hit.version === version) return hit.analysis;
  const analysis = analyze(model.uri.path.split('/').pop() || 'main.co', modelText(model).text, mergedNames(model));
  cache.set(model, { version, analysis });
  return analysis;
}

/* ── 跨文件名字总表 ──
 * 「这个文件里没定义」不能直接报错：名字也可能来自它 import 的文件。
 * 顺着 import 把依赖里的顶层定义（变量 / 列表 / 字典 / 函数 / 类）全收进来，
 * 只有总表里也没有，才算真的没定义。依赖还没解析完（读盘是异步的）时返回
 * undefined，这一轮先不报，下次校验再查表。 */
const importNamesCache = new WeakMap<monaco.editor.ITextModel, { key: string; names: Set<string> | null }>();

function importNamesOf(model: monaco.editor.ITextModel): Set<string> | undefined {
  const code = modelText(model).text;
  if (!/^[\t ]*import[ \t]+/m.test(code)) return undefined;

  const srcs: string[] = [];
  const seen = new Set<string>();
  let complete = true;
  const walk = (src: string, depth: number): void => {
    // 名字查找顺序就是用户的直觉：先本文件 → 再看 import → import 的文件里再 import 的，逐层往里翻。
    // 以前只翻 4 层，链式库（a 引 b 引 c 引 d）第 5 层开始的符号就被当成"没定义"。
    if (depth > 8) return;
    const re = /^[\t ]*import[ \t]+["']?([^"'\s;)]+)/gm;
    let m: RegExpExecArray | null;
    while ((m = re.exec(src)) !== null) {
      const want = m[1];
      if (seen.has(want)) continue;
      seen.add(want);
      const text = findImported(model, want);
      if (!text) { complete = false; continue; }
      srcs.push(text);
      walk(text, depth + 1);
    }
  };
  walk(code, 0);

  const key = `${model.getVersionId()}|${[...seen].join(',')}`;
  const hit = importNamesCache.get(model);
  if (hit && hit.key === key) return hit.names ?? undefined;
  if (!complete) {
    importNamesCache.set(model, { key, names: null });
    return undefined;
  }
  const names = new Set<string>();
  for (const src of srcs) {
    try {
      for (const n of toplevelNames(parseProgram('dep.co', src).body)) names.add(n);
    } catch { /* 依赖自己语法坏了不归这里管 */ }
  }
  importNamesCache.set(model, { key, names });
  return names;
}

/* ── import 进来的符号 ──
 * 补全只看当前模型的话，库文件里定义的函数就补不出来。
 * 这里顺着 import 把库也分析一遍：优先用编辑器里已经打开的模型（没存盘也算），
 * 没打开就去读一次盘并缓存，下次补全就有了。 */

const diskCache = new Map<string, string>();
const pending = new Set<string>();

function modelDir(model: monaco.editor.ITextModel): string {
  const p = decodeURIComponent(model.uri.path);
  const i = p.lastIndexOf('/');
  return i > 0 ? p.slice(0, i) : '';
}

function findImported(model: monaco.editor.ITextModel, want: string): string | null {
  const base = (want.split(/[\\/]/).pop() || want).toLowerCase();
  const wantPath = want.replace(/\\/g, '/').toLowerCase();
  for (const m of monaco.editor.getModels()) {
    const p = decodeURIComponent(m.uri.path).replace(/\\/g, '/').toLowerCase();
    const name = p.split('/').pop() || '';
    if (name === base || name === `${base}.co` || name === `${base}.cw` || p.endsWith(`/${wantPath}`)) return modelText(m).text;
  }
  const dir = modelDir(model);
  const key = `${dir}/${want}`.replace(/\\/g, '/');
  const hit = diskCache.get(key);
  if (hit) return hit || null;
  if (!pending.has(key)) {
    pending.add(key);
    void (async () => {
      try {
        if (!isTauriEnv()) return;
        const { api } = await import('../core/tauri');
        const text = await api.fsRead(key);
        diskCache.set(key, text);
      } catch {
        diskCache.set(key, '');
      } finally {
        pending.delete(key);
      }
    })();
  }
  return null;
}

/* ── 补全专用的"符号表分析" ──
 * 补全只需要变量 / 函数 / 类 / 大纲，不需要三路诊断。
 * 而且允许**落后一次击键**：版本变了的瞬间先用上一版（打字途中名字还没打完，
 * 差一个字符不影响候选），300 ms 空闲后再补算 —— 这样每敲一个键都不会整文件重分析。
 * 100 KB 的方岛物语：整份分析 70 ms，符号表版一次十来毫秒，而且不再是每击键一次。 */
const symbolsCache = new WeakMap<monaco.editor.ITextModel, { version: number; a: Analysis }>();
const refreshing = new WeakSet<monaco.editor.ITextModel>();
const SYMBOL_IDLE_MS = 300;

function symbolsOf(model: monaco.editor.ITextModel, force = false): Analysis {
  const version = model.getVersionId();
  const hit = symbolsCache.get(model);
  if (hit && hit.version === version && !force) return hit.a;
  if (hit && !force) {
    if (!refreshing.has(model)) {
      refreshing.add(model);
      setTimeout(() => {
        refreshing.delete(model);
        symbolsOf(model, true);
      }, SYMBOL_IDLE_MS);
    }
    return hit.a;
  }
  const name = model.uri.path.split('/').pop() || 'main.co';
  const a = analyze(name, modelText(model).text, undefined, { symbolsOnly: true });
  symbolsCache.set(model, { version, a });
  return a;
}

/* 依赖文件的分析缓存：按「文件名 + 内容指纹」存，LRU 48。
 * 以前每敲一个键就把 import 进来的文件重新 parse 一遍 —— 多文件工程就是这么烫起来的。 */
const depCache = new Map<string, { key: string; a: Analysis }>();
const DEP_CACHE_MAX = 48;

function hashText(s: string): string {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(36);
}

function depAnalysis(name: string, src: string): { a: Analysis; key: string } {
  const key = `${src.length}|${hashText(src)}`;
  const hit = depCache.get(name);
  if (hit && hit.key === key) {
    depCache.delete(name); depCache.set(name, hit); // LRU touch
    return hit;
  }
  const a = analyze(name, src);
  const entry = { key, a };
  depCache.set(name, entry);
  if (depCache.size > DEP_CACHE_MAX) depCache.delete(depCache.keys().next().value as string);
  return entry;
}

/** 合并结果按 (本文件版本 + 依赖指纹) 缓存。
 *  而且**绝不原地改** analysisOf 的缓存对象：以前这里往 a.vars 里 push，
 *  缓存的分析结果会被越塞越多，补全一次改一次，越用越慢。 */
const mergedCache = new WeakMap<monaco.editor.ITextModel, { base: Analysis; key: string; a: Analysis }>();

/** 当前文件 + 它 import 进来的文件，合成一份符号表 */
export function analysisWithImports(model: monaco.editor.ITextModel): Analysis {
  const base = symbolsOf(model);
  const code = modelText(model).text;
  if (!/^[\t ]*import[ \t]+/m.test(code)) return base;

  const deps: { name: string; src: string }[] = [];
  const seen = new Set<string>();
  const collect = (src: string, depth: number): void => {
    if (depth > 4) return;
    const re = /^[\t ]*import[ \t]+["']?([^"'\s;)]+)/gm;
    let m: RegExpExecArray | null;
    while ((m = re.exec(src)) !== null) {
      const want = m[1];
      if (seen.has(want)) continue;
      seen.add(want);
      const text = findImported(model, want);
      if (!text) continue;
      deps.push({ name: want, src: text });
      collect(text, depth + 1);
    }
  };
  collect(code, 0);
  if (deps.length === 0) return base;

  const key = deps.map((d) => `${d.name}:${hashText(d.src)}`).join(',');
  const hit = mergedCache.get(model);
  if (hit && hit.base === base && hit.key === key) return hit.a;
  /* 浅拷贝：只复制数组外壳，符号对象本身共享（只读） */
  const a: Analysis = { ...base, funcs: [...base.funcs], classes: [...base.classes], vars: [...base.vars] };
  for (const d of deps) {
    const sub = depAnalysis(d.name, d.src).a;
    for (const f of sub.funcs) {
      if (!a.funcs.some((x) => x.name === f.name)) a.funcs.push({ ...f, doc: `来自 ${d.name}` });
    }
    for (const c of sub.classes) {
      if (!a.classes.some((x) => x.name === c.name)) a.classes.push(c);
    }
    for (const v of sub.vars) {
      if (!a.vars.some((x) => x.name === v.name)) a.vars.push(v);
    }
  }
  mergedCache.set(model, { base, key, a });
  return a;
}

/* ───────────────────────────── 注册 ───────────────────────────── */

let registered = false;

/** 把插件里的 $1 / ${1:占位} 写法转成 Monaco 片段语法 */
function toMonacoSnippet(body: string): string {
  return body.replace(/\$(\d+)(?![:\d])/g, '\${$1}');
}

export function registerFloatLanguage(): void {
  if (registered) return;
  registered = true;

  monaco.languages.register({ id: LANG_ID, extensions: ['.co', '.cw'], aliases: ['Copper', 'CW Script', 'cwscript'] });

  monaco.languages.setMonarchTokensProvider(LANG_ID, {
    ignoreCase: false,
    keywords: KEYWORDS,
    types: TYPE_WORDS,
    tokenizer: {
      root: [
        // 块注释：顶行 /** 开块，*/ 闭合（可跨行；不闭合吃到文件末尾，解析器给 warning）。
        // 行内的 /** 会被解析器报错，这里按行注释上色兜底。
        [/^\/\*\*/, { token: 'comment', next: '@blockComment' }],
        // 行注释：/* 到行尾（不用闭合）；字符串里的 /* 不算（Monarch 先匹配字符串规则）。
        // `--` 是自减运算符、`//` 是整除运算符，都按 operator 上色，不当注释。
        [/\/\*.*$/, 'comment'],
        [/"(?:[^"\\]|\\.)*"/, 'string'],
        [/'(?:[^'\\]|\\.)*'/, 'string'],
        [/__last_[a-z]+/, 'variable.predefined'],
        [/\b(?:ans|self|this|super)\b/, 'variable.predefined'],
        [/(function|fn|Function|func)(\s+)([A-Za-z_\u4e00-\u9fa5]\w*)/, ['keyword', 'white', 'entity.name.function']],
        [/(class)(\s+)([A-Za-z_\u4e00-\u9fa5]\w*)/, ['keyword', 'white', 'entity.name.class']],
        [/\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/, 'number'],
        [/(\.)([A-Za-z_\u4e00-\u9fa5]\w*)/, ['delimiter', 'attribute.name']],
        [
          /[A-Za-z_\u4e00-\u9fa5]\w*/,
          { cases: { '@types': 'type', '@keywords': 'keyword', '@default': 'identifier' } }
        ],
        [/[{}()[\]]/, '@brackets'],
        [/[+\-*/%<>=!&|^~?]+/, 'operator'],
        [/[;,.]/, 'delimiter']
      ],
      blockComment: [
        [/[^*]+/, 'comment'],
        [/\*\//, { token: 'comment', next: '@pop' }],
        [/[\s\S]/, 'comment']
      ]
    }
  });

  monaco.languages.setLanguageConfiguration(LANG_ID, {
    // 行注释 /*（到行尾不用闭合）+ 块注释 /** ＊/（顶行开块，闭合可省）
    comments: { lineComment: '/*', blockComment: ['/**', '*/'] },
    brackets: [['{', '}'], ['(', ')'], ['[', ']']],
    autoClosingPairs: [
      { open: '{', close: '}' }, { open: '(', close: ')' }, { open: '[', close: ']' },
      { open: '"', close: '"', notIn: ['string'] }, { open: "'", close: "'", notIn: ['string'] }
    ],
    surroundingPairs: [
      { open: '{', close: '}' }, { open: '(', close: ')' }, { open: '[', close: ']' },
      { open: '"', close: '"' }, { open: "'", close: "'" }
    ],
    folding: { offSide: true },
    indentationRules: {
      increaseIndentPattern: /^\s*(?:if|elif|else|while|for|repeat|forever|main loop|wait until|function|fn|class)\b.*$|.*\{\s*$/,
      decreaseIndentPattern: /^\s*\}/
    }
  });

  registerCompletion();
  registerInline();
  registerHover();
  registerSignatureHelp();
}

/* ───────────────────────────── 补全 ───────────────────────────── */

function registerCompletion(): void {
  monaco.languages.registerCompletionItemProvider(LANG_ID, {
    triggerCharacters: ['.'],
    provideCompletionItems: (model, position, context) => {
      const a = analysisWithImports(model);
      const text = modelText(model);
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber, startColumn: word.startColumn,
        endLineNumber: position.lineNumber, endColumn: word.endColumn
      };

      /* 光标语义只问一处：context.ts 用词法 + AST 判断
       * （在注释里？在字符串里？在起名字？在 `对象.` 后面？） */
      const ctx = contextAt(text.text, position.lineNumber, position.column, a, text.lines);

      // import "…" 里补工程里的文件名
      if (ctx.kind === 'importPath') {
        const seenName = new Set<string>();
        const files: string[] = [];
        for (const m of monaco.editor.getModels()) {
          const name = decodeURIComponent(m.uri.path).split('/').pop() ?? '';
          if (!/\.(co|cw)$/i.test(name) || seenName.has(name)) continue;
          seenName.add(name);
          files.push(name);
        }
        files.sort();
        return {
          suggestions: files.map((name, i) => ({
            label: name,
            kind: monaco.languages.CompletionItemKind.File,
            insertText: name,
            filterText: name,
            sortText: `${i}`.padStart(4, '0'),
            range: {
              startLineNumber: position.lineNumber,
              startColumn: position.column - ctx.typed.length,
              endLineNumber: position.lineNumber,
              endColumn: position.column
            },
            documentation: { value: `引入 \`${name}\`` }
          }))
        };
      }
      // 字符串里 / 注释里不弹
      if (ctx.kind === 'string' || ctx.kind === 'comment') return { suggestions: [] };
      // 正在起名字：别拿 90 多个候选添乱
      if (ctx.kind === 'naming') return { suggestions: [] };

      /* 自动弹出（打字触发）且还没打出一个词时也不弹：
       * 否则每敲一个空格/括号就刷新一次上百条候选，
       * 上/下方向键会被候选列表抢走，光标根本没法一行一行走。
       * 手动 Ctrl+Space 或者刚打了 `.` 时照常弹。 */
      const ctxKind = (context as { triggerKind?: number } | undefined)?.triggerKind;
      const explicit = ctxKind === 1; // CompletionTriggerKind.Invoke
      if (!explicit && ctx.kind !== 'member' && word.word === '') return { suggestions: [] };

      const allLines = text.lines;
      const lineBefore = (allLines[position.lineNumber - 1] ?? '').slice(0, position.column - 1);
      const scopeVars = scopeVariables(a, allLines, position.lineNumber);

      const items = buildCompletions({
        word: word.word,
        analysis: a,
        context: ctx,
        position: completionPosition(lineBefore, word.word),
        scopeVars,
        /* 光标之前才存在的变量参与候选；后面才声明的先不列（候选少一半，也不用每键重排） */
        maxLine: position.lineNumber,
        plugins: { snippets: pluginSnippets, keywords: pluginKeywords }
      });

      /* 模板片段是一整段骨架（`if else` / `for range`），误选一次就把整行换掉。
       * 前缀太短时只给变量和关键字，规则见 completions.ts 的 visibleCompletions。 */
      let candidates = visibleCompletions(items, word.word);
      if (smartEnabled) {
        candidates = smartRank(currentSmartModel(), candidates, lineBefore, scopeVars, word.word);
      }
      candidates = capCompletions(candidates);

      const kinds: Record<CompletionKind, monaco.languages.CompletionItemKind> = {
        variable: monaco.languages.CompletionItemKind.Variable,
        function: monaco.languages.CompletionItemKind.Function,
        class: monaco.languages.CompletionItemKind.Class,
        method: monaco.languages.CompletionItemKind.Method,
        field: monaco.languages.CompletionItemKind.Field,
        keyword: monaco.languages.CompletionItemKind.Keyword,
        type: monaco.languages.CompletionItemKind.TypeParameter,
        constant: monaco.languages.CompletionItemKind.Constant,
        snippet: monaco.languages.CompletionItemKind.Snippet,
        value: monaco.languages.CompletionItemKind.Value
      };
      const S = monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;
      const suggestions = candidates.map((it) => ({
        label: it.labelDetail ? { label: it.label, detail: ` ${it.labelDetail}`, description: '' } : it.label,
        kind: kinds[it.kind],
        insertText: it.insertText,
        detail: it.detail,
        documentation: { value: both(it.detail, it.doc) },
        /* filterText 必须给：不然 Monaco 会拿 insertText（模板片段那一坨）去匹配。
         * 模板 label 里带空格（`if else` / `for range`），直接拿 label 当过滤词，
         * 用户刚打个 `if` 就会冒出整段模板，被回车选中就把整行换掉了。
         * 去掉空格后只有打全 `ifelse` / `forrange` 才会出现；想用模板还能 Ctrl+Space 手动选。 */
        filterText: it.kind === 'snippet' ? it.label.replace(/\s+/g, '') : it.label,
        sortText: it.sortText,
        range,
        // 只有模板片段才按 snippet 插入（带 ${1:xxx} 占位）
        insertTextRules: it.kind === 'snippet' ? S : undefined
      }));
      return { suggestions };
    }
  });
}

function both(title: string, body: string): string {
  return `\`\`\`float\n${title}\n\`\`\`\n\n${body}`;
}

/* ───────────────────────────── 行内续写 ─────────────────────────────
 * 光标停在行尾（刚打完一个空格）时，给"下一个词"的灰色幽灵建议：
 * 数据来源就是同一张本地计数表（不联网、不含源码原文），一次查表 O(1)。
 * Tab 接受、Esc 忽略；置信度不够就什么都不出，不会每敲一键就闪一下。 */
function registerInline(): void {
  const langs = monaco.languages as unknown as {
    registerInlineCompletionsProvider?: (
      id: string,
      provider: {
        provideInlineCompletions: (
          model: monaco.editor.ITextModel,
          position: monaco.Position
        ) => { items: { insertText: string; range: monaco.IRange }[] } | null;
        freeInlineCompletions?: () => void;
      }
    ) => void;
  };
  if (!langs.registerInlineCompletionsProvider) return; // 老版 Monaco 没有这个 API 就不注册

  langs.registerInlineCompletionsProvider(LANG_ID, {
    provideInlineCompletions: (model, position) => {
      if (!smartEnabled || !inlineEnabled) return { items: [] };
      /* 只有"行尾"才猜：光标后面还有内容就不适合插词 */
      const line = model.getLineContent(position.lineNumber);
      if (position.column - 1 !== line.length) return { items: [] };
      const lineBefore = line.slice(0, position.column - 1);
      if (!/\s$/.test(lineBefore)) return { items: [] };

      feedCorpus();
      const word = predictNext(smartCorpus.get(), lineBefore);
      if (!word) return { items: [] };
      return {
        items: [{
          insertText: word,
          range: {
            startLineNumber: position.lineNumber, startColumn: position.column,
            endLineNumber: position.lineNumber, endColumn: position.column
          }
        }]
      };
    },
    freeInlineCompletions: () => { /* 单条短文本，无需释放 */ }
  });
}

/* ───────────────────────────── 悬停 ───────────────────────────── */

function registerHover(): void {
  monaco.languages.registerHoverProvider(LANG_ID, {
    provideHover: (model, position) => {
      const word = model.getWordAtPosition(position);
      if (!word) return null;
      const name = word.word;
      const lineText = model.getLineContent(position.lineNumber);
      const a = analysisWithImports(model);

      const method = BUILTIN_METHODS.find((m) => m.name === name);
      if (method && /\.\s*$/.test(lineText.slice(0, word.startColumn - 1))) {
        return {
          contents: [
            { value: `**${method.owner}.${method.name}(…)**（内建方法）` },
            { value: method.desc }
          ]
        };
      }

      const sv = SPECIAL_VARS.find((s) => s.name === name);
      if (sv) {
        return { contents: [{ value: `**特殊变量** \`${sv.name}\`` }, { value: sv.desc }] };
      }

      /* `os.readfile` 这种带点名字：Monaco 的 word 只圈到 `readfile`，
       * 所以往前看一眼有没有 `库名.` 前缀，有就按完整名查。 */
      const prefixText = lineText.slice(0, word.startColumn - 1);
      const nsPrefix = /([A-Za-z_]\w*)\.\s*$/.exec(prefixText);
      const qualified = nsPrefix ? `${nsPrefix[1]}.${name}` : null;
      const b = BUILTINS.find((x) => x.name === (qualified ?? name));
      if (b) {
        return {
          contents: [
            { value: `**${b.sig}**（内建 · ${b.category}）` },
            { value: b.doc },
            { value: b.writes ? `单独成行书写时结果写入 \`${b.writes}\`` : '只能在表达式里使用' }
          ]
        };
      }

      const fn = a.funcs.find((f) => f.name === name) ?? a.classes.flatMap((c) => c.methods).find((m) => m.name === name);
      if (fn) {
        const sig = `${fn.name}(${fn.params.map((p) => `${p.name}${p.def ? ' = …' : ''}`).join(', ')})`;
        return {
          contents: [
            { value: `**${sig}**${fn.retType ? ` → ${fn.retType}` : ''}` },
            { value: fn.owner ? `方法，属于 ${fn.owner}，第 ${fn.line} 行` : `函数，第 ${fn.line} 行定义` }
          ]
        };
      }

      const cls = a.classes.find((c) => c.name === name);
      if (cls) {
        return {
          contents: [
            { value: `**class ${cls.name}**${cls.base ? ` extends ${cls.base}` : ''}` },
            { value: `字段：${cls.fields.map((f) => f.name).join('、') || '无'}` },
            { value: `方法：${cls.methods.map((m) => m.name + '()').join('、') || '无'}` }
          ]
        };
      }

      const v = a.vars.filter((x) => x.name === name).pop();
      if (v) {
        return { contents: [{ value: `**变量** \`${name}\`` }, { value: `类型：${v.cls ?? v.type}，第 ${v.line} 行` }] };
      }

      /* 指令：单字或双字 */
      const after = lineText.slice(word.endColumn - 1).trim().split(/\s+/)[0] ?? '';
      const before = lineText.slice(0, word.startColumn - 1).trim().split(/\s+/).pop() ?? '';
      const candidates = [`${before} ${name}`.trim(), `${name} ${after}`.trim(), name];
      const doc = candidates.map((c) => INSTRUCTIONS.find((i) => i.name === c)).find(Boolean);
      if (doc) {
        return {
          contents: [
            { value: `**${doc.name}**（${doc.category}）` },
            { value: doc.desc },
            { value: `\`${doc.args}\`` },
            ...(doc.result ? [{ value: `结果写入 \`${doc.result}\`` }] : [])
          ]
        };
      }

      if (KEYWORDS.includes(name)) {
        return { contents: [{ value: `**关键字** \`${name}\`` }] };
      }
      return null;
    }
  });
}

/* ───────────────────────────── 签名提示 ───────────────────────────── */

function registerSignatureHelp(): void {
  monaco.languages.registerSignatureHelpProvider(LANG_ID, {
    signatureHelpTriggerCharacters: ['(', ','],
    provideSignatureHelp: (model, position) => {
      const a = analysisOf(model);
      const text = model.getValueInRange(new monaco.Range(1, 1, position.lineNumber, position.column));

      let depth = 0;
      let open = -1;
      for (let i = text.length - 1; i >= 0; i--) {
        const c = text[i];
        if (c === ')') depth++;
        else if (c === '(') {
          if (depth === 0) { open = i; break; }
          depth--;
        }
      }
      if (open < 0) return null;

      const head = text.slice(0, open);
      const nm = /([A-Za-z_\u4e00-\u9fa5][\w.]*)\s*$/.exec(head);
      if (!nm) return null;
      const full = nm[1];
      const simple = full.includes('.') ? full.split('.').pop() as string : full;

      let label = '';
      let params: string[] = [];
      let doc = '';

      const b = BUILTINS.find((x) => x.name === full) ?? BUILTINS.find((x) => x.name === simple);
      if (b) {
        label = b.sig;
        params = paramNamesFromSig(b.sig);
        doc = b.doc;
      } else {
        const cands = [
          ...a.funcs.filter((f) => f.name === simple),
          ...a.classes.flatMap((c) => c.methods.filter((m) => m.name === simple)),
          ...a.classes.filter((c) => c.name === simple).flatMap((c) => (c.methods.find((m) => m.name === 'init') ? [c.methods.find((m) => m.name === 'init')!] : []))
        ];
        const fn = cands[0];
        if (!fn) return null;
        label = `${fn.name}(${fn.params.map((p) => `${p.name}${p.def ? ' = …' : ''}`).join(', ')})`;
        params = fn.params.map((p) => p.name);
        doc = fn.owner ? `${fn.owner} 的方法，第 ${fn.line} 行定义` : `第 ${fn.line} 行定义`;
      }

      let active = 0;
      let d = 0;
      for (let i = open + 1; i < text.length; i++) {
        const c = text[i];
        if (c === '(') d++;
        else if (c === ')') d--;
        else if (c === ',' && d === 0) active++;
      }

      return {
        value: {
          signatures: [{
            label,
            documentation: doc,
            parameters: params.map((p) => ({ label: p }))
          }],
          activeSignature: 0,
          activeParameter: Math.min(active, Math.max(0, params.length - 1))
        },
        dispose: () => { /* 无需清理 */ }
      };
    }
  });
}

function paramNamesFromSig(sig: string): string[] {
  const inner = /\((.*)\)/.exec(sig)?.[1] ?? '';
  if (!inner.trim()) return [];
  return inner.split(',').map((s) => s.trim().split(/[ =]/)[0]).filter(Boolean);
}

/* ───────────────────────────── 诊断 ───────────────────────────── */

export function validateModel(model: monaco.editor.ITextModel): void {
  if (model.getLanguageId() !== LANG_ID) return;
  const a = analysisOf(model);
  applyDiagnostics(model, a.diagnostics, model.getVersionId());
}

/** 把诊断画到编辑器上；version 用来丢弃过期结果（worker 回来时文件可能又改过了） */
function applyDiagnostics(model: monaco.editor.ITextModel, diagnostics: Diagnostic[], version: number): void {
  if (model.isDisposed() || model.getVersionId() !== version) return;
  const markers: monaco.editor.IMarkerData[] = diagnostics.map((d) => ({
    severity: d.severity === 'error' ? monaco.MarkerSeverity.Error
      : d.severity === 'warning' ? monaco.MarkerSeverity.Warning
      : monaco.MarkerSeverity.Info,
    message: d.message,
    code: d.code,
    source: 'Copper',
    startLineNumber: d.span.line,
    startColumn: Math.max(1, d.span.col),
    endLineNumber: d.span.endLine,
    endColumn: Math.max(d.span.col + 1, d.span.endCol)
  }));
  monaco.editor.setModelMarkers(model, 'float-script', markers);
}

/** 后台校验：解析 + 三路诊断丢给 worker，主线程不卡。
 *  worker 不可用时退回同步（结果一样，只是占主线程）。 */
export async function validateModelAsync(model: monaco.editor.ITextModel): Promise<void> {
  if (model.getLanguageId() !== LANG_ID || model.isDisposed()) return;
  const version = model.getVersionId();
  const text = modelText(model).text;
  const names = mergedNames(model);
  const res = await diagnoseAsync(
    model.uri.path.split('/').pop() || 'main.co',
    text,
    names
  );
  applyDiagnostics(model, res.diagnostics, version);
}

/* ───────────────────────────── 大纲 ───────────────────────────── */

/** 供右侧大纲面板使用：AST 驱动，包含类 / 方法 / 字段的层级 */
export function extractOutline(src: string): OutlineItem[] {
  // 只要大纲，不要诊断：symbolsOnly 走的是快路径（四成时间省下来）
  return analyze('main.co', src, undefined, { symbolsOnly: true }).outline;
}

/** 文件统计，状态栏用 */
export function fileStats(src: string): { lines: number; classes: number; funcs: number; errors: number } {
  const a = analyze('main.co', src);
  return {
    lines: src.split('\n').length,
    classes: a.classes.length,
    funcs: a.funcs.length + a.classes.reduce((n, c) => n + c.methods.length, 0),
    errors: a.diagnostics.filter((d) => d.severity === 'error').length
  };
}
