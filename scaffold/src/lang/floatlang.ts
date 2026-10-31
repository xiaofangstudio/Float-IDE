import * as monaco from 'monaco-editor';
import { INSTRUCTIONS, SPECIAL_VARS, KEYWORDS, TYPE_WORDS, SNIPPETS, BUILTIN_METHODS, LANG_ID } from './spec';

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

/* CW Script 编辑器支持：Monarch 高亮、AST 驱动的补全 / 悬停 / 签名提示 / 实时诊断 / 大纲。
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
  const analysis = analyze(model.uri.path.split('/').pop() || 'main.cw', model.getValue());
  cache.set(model, { version, analysis });
  return analysis;
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

  monaco.languages.register({ id: LANG_ID, extensions: ['.cw'], aliases: ['CW Script', 'cwscript'] });

  monaco.languages.setMonarchTokensProvider(LANG_ID, {
    ignoreCase: false,
    keywords: KEYWORDS,
    types: TYPE_WORDS,
    tokenizer: {
      root: [
        [/^--.*$/, 'comment.skip'],
        [/\/\/.*$/, 'comment'],
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
        [/[+\-*/%<>=!&|^~]+/, 'operator'],
        [/[;,.]/, 'delimiter']
      ]
    }
  });

  monaco.languages.setLanguageConfiguration(LANG_ID, {
    comments: { lineComment: '//' },
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
      increaseIndentPattern: /^\s*(?:if|elif|else|while|for|repeat|forever|main loop|wait until|function|fn|class)\b.*$/,
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
    triggerCharacters: ['.', ' ', '('],
    provideCompletionItems: (model, position) => {
      const a = analysisOf(model);
      const K = monaco.languages.CompletionItemKind;
      const S = monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;
      const line = model.getLineContent(position.lineNumber);
      const before = line.slice(0, position.column - 1);
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber, startColumn: word.startColumn,
        endLineNumber: position.lineNumber, endColumn: word.endColumn
      };

      const suggestions: monaco.languages.CompletionItem[] = [];
      const seen = new Set<string>();
      const add = (
        label: string, kind: monaco.languages.CompletionItemKind, insertText: string,
        detail: string, doc: string, snippet = false, sortText?: string
      ): void => {
        if (seen.has(label)) return;
        seen.add(label);
        suggestions.push({
          label, kind, insertText, detail, range, sortText,
          insertTextRules: snippet ? S : undefined,
          documentation: { value: doc }
        });
      };

      /* 成员补全：p. / self. */
      const member = /([A-Za-z_\u4e00-\u9fa5][\w\u4e00-\u9fa5]*)\.\s*[A-Za-z_\u4e00-\u9fa5]*$/.exec(before);
      if (member) {
        const objName = member[1];
        let clsName: string | null = null;
        if (objName === 'self' || objName === 'this') {
          clsName = classAtLine(a, position.lineNumber)?.name ?? null;
        } else {
          const v = a.vars.filter((x) => x.name === objName).pop();
          clsName = v?.cls ?? null;
        }
        // 内建方法兜底（不知道类型时就都列出来，按 owner 标注）
        for (const m of BUILTIN_METHODS) {
          add(m.name, K.Method, `${m.name}($0)`, `${m.owner}.${m.name}()`, both(`${m.owner}.${m.name}()`, m.desc), true, '8');
        }
        if (clsName) {
          for (const f of classFields(a, clsName)) {
            add(f.name, K.Field, f.name, `${clsName}.${f.name}${f.type ? ` · ${f.type}` : ''}`, `字段 \`${f.name}\`${f.type ? `，类型 ${f.type}` : ''}`, false, '1');
          }
          for (const m of classMethods(a, clsName)) {
            add(m.name, K.Method, `${m.name}($0)`, `${clsName}.${m.name}(${m.params.map((p) => p.name).join(', ')})`,
              `方法 \`${m.name}(${m.params.map((p) => p.name).join(', ')})\`${m.retType ? ` → ${m.retType}` : ''}`, true, '2');
          }
        }
        return { suggestions };
      }

      /* 关键字骨架 */
      for (const sn of SNIPPETS) {
        add(sn.label, K.Snippet, sn.body, sn.detail, both(sn.detail, sn.body), true, '5');
      }

      /* 插件贡献的片段（输入前缀就能展开） */
      for (const sn of pluginSnippets) {
        add(sn.prefix, K.Snippet, toMonacoSnippet(sn.body),
          `片段 · ${sn.description ?? sn.prefix}`, both(sn.description ?? sn.prefix, sn.body), true, '5');
      }
      for (const kw of pluginKeywords) {
        add(kw.label, K.Function, kw.insert ?? `${kw.label}($0)`, kw.detail ?? '', both(kw.detail ?? kw.label, kw.insert ?? ''), true, '5');
      }

      /* 本文件函数 */
      for (const f of a.funcs) {
        const sig = `${f.name}(${f.params.map((p) => p.name).join(', ')})`;
        add(f.name, K.Function, `${f.name}($0)`, sig, both(sig, `第 ${f.line} 行定义${f.retType ? `，返回 ${f.retType}` : ''}`), true, '1');
      }

      /* 类 */
      for (const c of a.classes) {
        add(c.name, K.Class, `new ${c.name}($0)`, `class ${c.name}${c.base ? ` : ${c.base}` : ''}`,
          both(`class ${c.name}`, `第 ${c.line} 行定义；用 new ${c.name}(…) 创建实例`), true, '2');
        for (const m of c.methods) {
          add(`${c.name}.${m.name}`, K.Method, `${m.name}($0)`, `${c.name}.${m.name}(${m.params.map((p) => p.name).join(', ')})`,
            `方法 \`${m.name}\``, true, '6');
        }
      }

      /* 变量 */
      for (const v of a.vars) {
        add(v.name, K.Variable, v.name, v.cls ? `${v.cls}` : v.type, `变量 \`${v.name}\`${v.cls ? `，类型 ${v.cls}` : `，类型 ${v.type}`}`, false, '3');
      }

      /* 内建函数 */
      for (const b of BUILTINS) {
        add(b.name, K.Function, `${b.name}($0)`, b.sig, both(b.sig, b.doc), true, '4');
      }

      /* 指令 */
      for (const ins of INSTRUCTIONS) {
        const first = ins.name.split(' ')[0];
        if (!first) continue;
        add(first, K.Keyword, ins.args.includes('(') ? `${first}($0)` : first, ins.args,
          both(`**${ins.name}**`, `${ins.desc}${ins.result ? `\n\n结果写入 \`${ins.result}\`` : ''}`), false, '7');
      }

      /* 特殊变量 */
      for (const sv of SPECIAL_VARS) {
        add(sv.name, K.Constant, sv.name, '特殊变量', `特殊变量 \`${sv.name}\`：${sv.desc}`, false, '0');
      }

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
      const a = analysisOf(model);

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
    source: 'CW Script',
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
  return analyze('main.cw', src).outline;
}

/** 文件统计，状态栏用 */
export function fileStats(src: string): { lines: number; classes: number; funcs: number; errors: number } {
  const a = analyze('main.cw', src);
  return {
    lines: src.split('\n').length,
    classes: a.classes.length,
    funcs: a.funcs.length + a.classes.reduce((n, c) => n + c.methods.length, 0),
    errors: a.diagnostics.filter((d) => d.severity === 'error').length
  };
}
