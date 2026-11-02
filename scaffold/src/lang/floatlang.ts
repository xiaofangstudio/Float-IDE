import * as monaco from 'monaco-editor';
import { INSTRUCTIONS, SPECIAL_VARS, KEYWORDS, TYPE_WORDS, SNIPPETS, BUILTIN_METHODS, LANG_ID } from './spec';
import { buildCompletions, type CompletionKind } from './completions';
import { contextAt } from './context';
import { isTauriEnv } from '../core/tauri';

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

export function analysisOf(model: monaco.editor.ITextModel): Analysis {
  const hit = cache.get(model);
  const version = model.getVersionId();
  if (hit && hit.version === version) return hit.analysis;
  const analysis = analyze(model.uri.path.split('/').pop() || 'main.co', model.getValue());
  cache.set(model, { version, analysis });
  return analysis;
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
    if (name === base || name === `${base}.co` || name === `${base}.cw` || p.endsWith(`/${wantPath}`)) return m.getValue();
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

/** 当前文件 + 它 import 进来的文件，合成一份符号表 */
export function analysisWithImports(model: monaco.editor.ITextModel): Analysis {
  const a = analysisOf(model);
  const seen = new Set<string>();
  const visit = (code: string, depth: number): void => {
    if (depth > 4) return;
    const re = /^[\t ]*import[ \t]+["']?([^"'\s;)]+)/gm;
    let m: RegExpExecArray | null;
    while ((m = re.exec(code)) !== null) {
      const want = m[1];
      if (seen.has(want)) continue;
      seen.add(want);
      const src = findImported(model, want);
      if (!src) continue;
      const sub = analyze(want, src);
      for (const f of sub.funcs) {
        if (!a.funcs.some((x) => x.name === f.name)) a.funcs.push({ ...f, doc: `来自 ${want}` });
      }
      for (const c of sub.classes) {
        if (!a.classes.some((x) => x.name === c.name)) a.classes.push(c);
      }
      for (const v of sub.vars) {
        if (!a.vars.some((x) => x.name === v.name)) a.vars.push(v);
      }
      visit(src, depth + 1);
    }
  };
  visit(model.getValue(), 0);
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
        // 注释只有 `--`（后面要跟空白或行尾，`i--` 自减不受影响）。
        // `//` 不是注释，它是整除运算符，下面按 operator 上色。
        [/(^|\s)--(?=\s|$).*$/, 'comment'],
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
      ]
    }
  });

  monaco.languages.setLanguageConfiguration(LANG_ID, {
    // 注释只有 `--`（`//` 是整除运算符，拿它当注释会直接把代码变成算式）
    comments: { lineComment: '--' },
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
  registerHover();
  registerSignatureHelp();
}

/* ───────────────────────────── 补全 ───────────────────────────── */

function registerCompletion(): void {
  monaco.languages.registerCompletionItemProvider(LANG_ID, {
    triggerCharacters: ['.'],
    provideCompletionItems: (model, position, context) => {
      const a = analysisWithImports(model);
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber, startColumn: word.startColumn,
        endLineNumber: position.lineNumber, endColumn: word.endColumn
      };

      /* 光标语义只问一处：context.ts 用词法 + AST 判断
       * （在注释里？在字符串里？在起名字？在 `对象.` 后面？） */
      const ctx = contextAt(model.getValue(), position.lineNumber, position.column, a);

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

      const items = buildCompletions({
        word: word.word,
        analysis: a,
        context: ctx,
        plugins: { snippets: pluginSnippets, keywords: pluginKeywords }
      });

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
      const suggestions = items.map((it) => ({
        label: it.labelDetail ? { label: it.label, detail: ` ${it.labelDetail}`, description: '' } : it.label,
        kind: kinds[it.kind],
        insertText: it.insertText,
        detail: it.detail,
        documentation: { value: both(it.detail, it.doc) },
        // filterText 必须给：不然 Monaco 会拿 insertText（模板片段那一坨）去匹配
        filterText: it.label,
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

      const b = BUILTINS.find((x) => x.name === name);
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

      const b = BUILTINS.find((x) => x.name === simple);
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
  const markers: monaco.editor.IMarkerData[] = a.diagnostics.map((d) => ({
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

/* ───────────────────────────── 大纲 ───────────────────────────── */

/** 供右侧大纲面板使用：AST 驱动，包含类 / 方法 / 字段的层级 */
export function extractOutline(src: string): OutlineItem[] {
  return analyze('main.co', src).outline;
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
