import React, { useCallback, useEffect, useRef, useState } from 'react';
import * as monaco from 'monaco-editor';
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
import { api } from '../core/tauri';
import type { ProblemItem } from '../core/types';
import { registerFloatLanguage, LANG_ID, validateModel } from '../lang/floatlang';
import { loadSettings, SETTINGS_EVENT, type Settings } from '../ui/settings';
import { askText } from '../ui/prompt';
import { IconClose, IconPlus } from '../ui/icons';

/* Float 编辑器：多标签 + Monaco（.cw 高亮/补全/悬停）+ 断点 + 当前执行行 + Ctrl+S。 */

(self as any).MonacoEnvironment = {
  getWorker(_mid: string, label: string): Worker {
    if (label === 'json') return new jsonWorker();
    return new editorWorker();
  }
};

registerFloatLanguage();

monaco.editor.defineTheme('float-light', {
  base: 'vs',
  inherit: true,
  /* 语法高亮：彩色，但全部取低饱和档（参考 GitHub Light / One Light）。
     彩色的目的是「一眼分出这是字符串还是数字」，不是装饰。 */
  rules: [
    { token: 'comment', foreground: '6e7f92', fontStyle: 'italic' },
    { token: 'comment.skip', foreground: '8d9cad', fontStyle: 'italic' },
    { token: 'keyword', foreground: '0a3d91', fontStyle: 'bold' },
    { token: 'type', foreground: '0a6c8a' },
    { token: 'string', foreground: '146c43' },
    { token: 'number', foreground: '9a4d00' },
    { token: 'identifier', foreground: '1f2b3a' },
    { token: 'entity.name.function', foreground: '0a3d91', fontStyle: 'bold' },
    { token: 'entity.name.class', foreground: '6639ba', fontStyle: 'bold' },
    { token: 'attribute.name', foreground: '0a6c8a' },
    { token: 'variable.predefined', foreground: 'b42318', fontStyle: 'bold' },
    { token: 'operator', foreground: '4a5f75' },
    { token: 'delimiter', foreground: '64798f' }
  ],
  colors: {
    'editor.background': '#ffffff',
    'editor.foreground': '#243244',
    'editorLineNumber.foreground': '#b3c0d0',
    'editorLineNumber.activeForeground': '#1567c6',
    'editor.lineHighlightBackground': '#f4f7fb',
    'editor.selectionBackground': '#d2e3f9',
    'editor.inactiveSelectionBackground': '#e6eef9',
    'editorCursor.foreground': '#1567c6',
    'editorIndentGuide.background1': '#eef2f8',
    'editorIndentGuide.activeBackground1': '#d5dfec',
    'editorGutter.background': '#ffffff',
    'editorWidget.background': '#ffffff',
    'editorWidget.border': '#dee5ef',
    'editorSuggestWidget.background': '#ffffff',
    'editorSuggestWidget.border': '#c4d0e0',
    'editorSuggestWidget.foreground': '#1f2b3a',
    'editorSuggestWidget.selectedBackground': '#e9f1fc',
    'editorSuggestWidget.selectedForeground': '#0a3d91',
    'editorSuggestWidget.selectedIconForeground': '#1567c6',
    'editorSuggestWidget.highlightForeground': '#1567c6',
    'editorSuggestWidget.focusHighlightForeground': '#0a3d91',
    'editorSuggestWidgetStatus.foreground': '#77899f',
    'editorWidget.foreground': '#1f2b3a',
    'editorHoverWidget.background': '#ffffff',
    'editorHoverWidget.border': '#dee5ef',
    'scrollbarSlider.background': '#c8d5e6aa',
    'scrollbarSlider.hoverBackground': '#aabdd4cc',
    'minimap.background': '#ffffff',
    'editorError.foreground': '#b42318',
    'editorWarning.foreground': '#8a5a00',
    'editorInfo.foreground': '#1567c6'
  }
});

monaco.editor.defineTheme('float-dark', {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'comment', foreground: '6a7d92', fontStyle: 'italic' },
    { token: 'comment.skip', foreground: '56697d', fontStyle: 'italic' },
    { token: 'keyword', foreground: '7cb0f0', fontStyle: 'bold' },
    { token: 'type', foreground: '6ec8d8' },
    { token: 'string', foreground: '7ec699' },
    { token: 'number', foreground: 'd9a15c' },
    { token: 'identifier', foreground: 'cfdae7' },
    { token: 'entity.name.function', foreground: '8ec8ff', fontStyle: 'bold' },
    { token: 'entity.name.class', foreground: 'c3a6ff', fontStyle: 'bold' },
    { token: 'attribute.name', foreground: '7fd0e0' },
    { token: 'variable.predefined', foreground: 'f2777a', fontStyle: 'bold' },
    { token: 'operator', foreground: '93a8bd' },
    { token: 'delimiter', foreground: '7f95ab' }
  ],
  colors: {
    'editor.background': '#121c27',
    'editor.foreground': '#d6e0ec',
    'editorLineNumber.foreground': '#46586c',
    'editorLineNumber.activeForeground': '#6fb6ff',
    'editor.lineHighlightBackground': '#17232f',
    'editor.selectionBackground': '#1b3348',
    'editor.inactiveSelectionBackground': '#182838',
    'editorCursor.foreground': '#6fb6ff',
    'editorIndentGuide.background1': '#1d2a37',
    'editorIndentGuide.activeBackground1': '#2c3d4e',
    'editorGutter.background': '#121c27',
    'editorWidget.background': '#16222f',
    'editorWidget.border': '#24323f',
    'editorSuggestWidget.background': '#16222f',
    'editorSuggestWidget.border': '#3a4f66',
    'editorSuggestWidget.foreground': '#cfdae7',
    'editorSuggestWidget.selectedBackground': '#1b3348',
    'editorSuggestWidget.selectedForeground': '#a8d4ff',
    'editorSuggestWidget.selectedIconForeground': '#6fb6ff',
    'editorSuggestWidget.highlightForeground': '#6fb6ff',
    'editorSuggestWidget.focusHighlightForeground': '#a8d4ff',
    'editorSuggestWidgetStatus.foreground': '#6d8095',
    'editorHoverWidget.background': '#16222f',
    'editorHoverWidget.border': '#24323f',
    'scrollbarSlider.background': '#2a3a4a88',
    'scrollbarSlider.hoverBackground': '#3a4d61aa',
    'minimap.background': '#121c27',
    'editorError.foreground': '#f2777a',
    'editorWarning.foreground': '#d9a441',
    'editorInfo.foreground': '#6fb6ff'
  }
});

export type DiffSpec = { title: string; base: string; current: string; lang: string };

/** 一个「文档标签页」：不是文件，而是一个整页界面（教程 / 插件 / MCP / 设置…）。
 *  IDE 里这些东西应该是标签页，不是浮在编辑器上的卡片。 */
export type DocTab = {
  id: string;
  title: string;
  /** 标签上的图标元素 */
  icon: React.ReactNode;
  /** 页面内容 */
  node: React.ReactNode;
  /** 同一个 id 再打开时是否刷新内容（默认刷新） */
  reuse?: boolean;
};

export type EditorHandle = {
  openFile: (path: string) => Promise<void>;
  closeTab: (path: string) => void;
  saveAll: () => Promise<number>;
  /** 单文件保存（外部改动内容后调用，例如全局替换） */
  saveFile: (path: string) => Promise<boolean>;
  /** 直接用新内容覆盖某个已打开文件的缓冲区 */
  setContent: (path: string, text: string) => void;
  revealLine: (path: string, line: number) => void;
  dirtyCount: () => number;
  getValue: (path: string) => string | null;
  refreshBpDecor: () => void;
  openDiff: (spec: DiffSpec) => void;
  closeDiff: () => void;
  /** 执行 Monaco 内置动作（撤销/查找/跳行/注释…） */
  runAction: (id: string) => void;
  /** 字号缩放，delta 为 +1 / -1 / 0（0 = 重置） */
  zoom: (delta: number) => void;
  /** 在光标行切换断点 */
  toggleBreakpoint: () => void;
  /** 当前激活文件路径 */
  currentPath: () => string | null;
  /** 光标所在行列（代码导航用） */
  cursor: () => { line: number; col: number } | null;
  /** 当前编辑器全文（插件 API 用） */
  activeText: () => string;
  /** 当前选区文本 */
  activeSelection: () => string;
  /** 在光标处插入文本 */
  insertAtCursor: (text: string) => void;
  /** 用新内容替换当前编辑器全文 */
  replaceActiveText: (text: string) => void;
  /** 新建文件（标签栏 + 按钮用） */
  newFile: () => Promise<void>;
  /** 打开（或切到）一个文档标签页 */
  openDoc: (doc: DocTab) => void;
  /** 关闭文档标签页 */
  closeDoc: (id: string) => void;
  /** 当前是否停在某个文档页上 */
  activeDocId: () => string | null;
};

type Props = {
  projectDir: string;
  bus: React.MutableRefObject<{ editor?: EditorHandle }>;
  /** 保存完成的路径列表，供工程索引增量更新 */
  onSaved: (paths: string[]) => void;
  onDirtyChange?: (n: number) => void;
  /** 断点行集合：key = 文件路径 */
  breakpoints: React.MutableRefObject<Map<string, Set<number>>>;
  /** 当前执行位置（调试暂停时） */
  execPos: { file: string; line: number } | null;
  /** 光标位置（状态栏用） */
  onCursor?: (line: number, col: number) => void;
  /** 当前文件的诊断列表（状态栏与问题面板用） */
  onProblems?: (items: ProblemItem[]) => void;
  /** 当前激活文件名（标题栏用） */
  onActiveFile?: (name: string) => void;
};

export function EditorPane({ projectDir, bus, onSaved, onDirtyChange, breakpoints, execPos, onCursor, onProblems, onActiveFile }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const diffHostRef = useRef<HTMLDivElement>(null);
  const edRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const diffEdRef = useRef<monaco.editor.IDiffEditor | null>(null);
  const modelsRef = useRef<Map<string, monaco.editor.ITextModel>>(new Map());
  const bpDecorRef = useRef<Set<string>>(new Set());
  const execDecorRef = useRef<Set<string>>(new Set());
  const diffModelsRef = useRef<{ original: monaco.editor.ITextModel; modified: monaco.editor.ITextModel } | null>(null);

  const [tabs, setTabs] = useState<string[]>([]);
  const [activeTab, setActiveTab] = useState<string | null>(null);
  /* 文档标签页（教程/插件/MCP/设置…）。和文件标签并存，点哪个显示哪个。 */
  const [docTabs, setDocTabs] = useState<DocTab[]>([]);
  const [activeDoc, setActiveDoc] = useState<string | null>(null);
  const activeDocRef = useRef<string | null>(null);
  activeDocRef.current = activeDoc;
  const [dirty, setDirty] = useState<Set<string>>(new Set());
  const [diffSpec, setDiffSpec] = useState<DiffSpec | null>(null);
  const [status, setStatus] = useState('');
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  const cursorCb = useRef(onCursor);
  cursorCb.current = onCursor;
  const problemsCb = useRef(onProblems);
  problemsCb.current = onProblems;

  /* ── 写安全状态（防频繁操作把代码写坏）──────────────────────
   * versionRef：磁盘上这份文件的 mtime，写入前要校验，避免覆盖别人改过的版本。
   * savingRef ：正在写盘的路径，禁止重复并发写同一文件（Ctrl+S 连点 / 自动保存撞车）。
   * conflictRef：写冲突时挂起的路径，交给用户在「覆盖 / 重载」里选，绝不自动决定。 */
  const versionRef = useRef<Map<string, number>>(new Map());
  const savingRef = useRef<Set<string>>(new Set());
  const [conflict, setConflict] = useState<{ path: string; name: string } | null>(null);
  const conflictRef = useRef(conflict);
  conflictRef.current = conflict;

  /** 把当前模型的诊断数量报给状态栏 */
  const reportProblems = useCallback((m: monaco.editor.ITextModel, path: string): void => {
    const marks = monaco.editor.getModelMarkers({ resource: m.uri });
    const items: ProblemItem[] = marks.map((k) => ({
      severity: k.severity === monaco.MarkerSeverity.Error ? 'error'
        : k.severity === monaco.MarkerSeverity.Warning ? 'warning' : 'info',
      message: k.message,
      file: path,
      line: k.startLineNumber,
      col: k.startColumn,
      code: typeof k.code === 'string' ? k.code : undefined,
      from: 'edit'
    }));
    problemsCb.current?.(items);
  }, []);

  const relOf = useCallback((p: string) => {
    const norm = p.replace(/\\/g, '/').replace(/\/+$/, '');
    const root = projectDir.replace(/\\/g, '/').replace(/\/+$/, '');
    return norm.startsWith(root + '/') ? norm.slice(root.length + 1) : norm;
  }, [projectDir]);

  const uriOf = useCallback((path: string) => monaco.Uri.parse('file:///project/' + relOf(path)), [relOf]);

  useEffect(() => { onDirtyChange?.(dirty.size); }, [dirty, onDirtyChange]);

  useEffect(() => {
    const applyTheme = (): void => {
      monaco.editor.setTheme(document.documentElement.dataset.theme === 'dark' ? 'float-dark' : 'float-light');
    };
    const init = loadSettings();
    const onSettings = (e: Event): void => {
      const st = (e as CustomEvent<Settings>).detail;
      edRef.current?.updateOptions({
        fontSize: st.fontSize,
        tabSize: st.tabSize,
        wordWrap: st.wordWrap ? 'on' : 'off',
        suggest: { preview: st.suggestDocs, showStatusBar: true, showInlineDetails: true }
      });
      fontSizeRef.current = st.fontSize;
      applyTheme();
    };
    window.addEventListener(SETTINGS_EVENT, onSettings);
    // 主题切换：无论是设置面板改的，还是外面直接改 data-theme，都同步到 Monaco
    const obs = new MutationObserver(applyTheme);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    window.setTimeout(applyTheme, 0);
    applyTheme();
    void init;
    return () => {
      window.removeEventListener(SETTINGS_EVENT, onSettings);
      obs.disconnect();
    };
  }, []);

  useEffect(() => {
    if (!hostRef.current || !diffHostRef.current) return;
    const st = loadSettings();
    fontSizeRef.current = st.fontSize;
    const ed = monaco.editor.create(hostRef.current, {
      theme: document.documentElement.dataset.theme === 'dark' ? 'float-dark' : 'float-light',
      automaticLayout: true,
      fontSize: st.fontSize,
      tabSize: st.tabSize,
      wordWrap: st.wordWrap ? 'on' : 'off',
      fontFamily: 'Consolas, "Courier New", monospace',
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      padding: { top: 10 },
      glyphMargin: true,
      lineNumbersMinChars: 3,
      renderLineHighlight: 'all',
      suggestSelection: 'first',
      /* 补全弹窗：开右侧文档预览 + 底部状态条 + 行内类型，信息给足 */
      suggest: {
        preview: true,
        showStatusBar: true,
        showInlineDetails: true,
        insertMode: 'replace',
        snippetsPreventQuickSuggestions: false
      },
      quickSuggestions: { other: true, comments: false, strings: true },
      quickSuggestionsDelay: 80,
      acceptSuggestionOnEnter: 'on',
      tabCompletion: 'on',
      wordBasedSuggestions: 'off',
      snippetSuggestions: 'top'
    });
    edRef.current = ed;
    const diffEd = monaco.editor.createDiffEditor(diffHostRef.current, {
      theme: document.documentElement.dataset.theme === 'dark' ? 'float-dark' : 'float-light',
      automaticLayout: true,
      readOnly: true,
      fontSize: 13,
      renderSideBySide: true,
      minimap: { enabled: false }
    });
    diffEdRef.current = diffEd;

    // 点击行号左侧空白 → 切换断点
    ed.onDidChangeCursorPosition((e) => {
      cursorCb.current?.(e.position.lineNumber, e.position.column);
    });
    ed.onMouseDown((e) => {
      if (e.target.type === monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN && e.target.position) {
        const path = activePathRef.current;
        const line = e.target.position.lineNumber;
        if (!path) return;
        const set = breakpoints.current.get(path) ?? new Set<number>();
        if (set.has(line)) set.delete(line); else set.add(line);
        breakpoints.current.set(path, set);
        refreshBpDecor();
      }
    });
    return () => { ed.dispose(); diffEd.dispose(); edRef.current = null; diffEdRef.current = null; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshBpDecor = useCallback((): void => {
    const ed = edRef.current;
    if (!ed) return;
    const path = activePathRef.current;
    const lines = path ? [...(breakpoints.current.get(path) ?? [])] : [];
    const old = bpDecorRef.current;
    if (old.size) ed.removeDecorations([...old]);
    const ids = ed.deltaDecorations([], lines.map((l) => ({
      range: new monaco.Range(l, 1, l, 1),
      options: {
        isWholeLine: false,
        glyphMarginClassName: 'bp-dot',
        glyphMarginHoverMessage: { value: '断点：点击左侧空白可切换' }
      }
    })));
    bpDecorRef.current = new Set(ids);
  }, [breakpoints]);

  const activePathRef = useRef<string | null>(null);
  activePathRef.current = diffSpec ? null : activeTab;

  useEffect(() => { refreshBpDecor(); }, [activeTab, refreshBpDecor]);

  /* 执行行高亮 */
  useEffect(() => {
    const ed = edRef.current;
    if (!ed) return;
    if (execDecorRef.current.size) ed.removeDecorations([...execDecorRef.current]);
    execDecorRef.current = new Set();
    if (!execPos) return;
    // 打开对应文件
    void openFile(execPos.file).then(() => {
      const e2 = edRef.current;
      if (!e2) return;
      const ids = e2.deltaDecorations([], [{
        range: new monaco.Range(execPos.line, 1, execPos.line, 1),
        options: { isWholeLine: true, className: 'exec-line', linesDecorationsClassName: 'exec-arrow' }
      }]);
      execDecorRef.current = new Set(ids);
      e2.revealLineInCenter(execPos.line);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [execPos?.file, execPos?.line]);

  const fontSizeRef = useRef(14);

  const runAction = useCallback((id: string): void => {
    const ed = edRef.current;
    if (!ed) return;
    ed.focus();
    void ed.getAction(id)?.run();
  }, []);

  const zoom = useCallback((delta: number): void => {
    const next = delta === 0 ? 14 : Math.min(28, Math.max(10, fontSizeRef.current + delta));
    fontSizeRef.current = next;
    edRef.current?.updateOptions({ fontSize: next });
  }, []);

  const toggleBreakpoint = useCallback((): void => {
    const ed = edRef.current;
    const path = activePathRef.current;
    const pos = ed?.getPosition();
    if (!ed || !path || !pos) return;
    const set = breakpoints.current.get(path) ?? new Set<number>();
    if (set.has(pos.lineNumber)) set.delete(pos.lineNumber); else set.add(pos.lineNumber);
    breakpoints.current.set(path, set);
    refreshBpDecor();
  }, [breakpoints, refreshBpDecor]);

  /**
   * 保存单个文件。要点：
   *  - 同一文件同时只允许一个写盘请求在飞（连点 Ctrl+S / 自动保存撞车直接跳过）；
   *  - 写前记下内容快照，写完只有「期间没再改动」才清掉脏标记，
   *    否则保留脏标记，避免把用户刚敲进去的代码标成已保存而丢掉；
   *  - 走 fsWriteChecked 校验 mtime：磁盘被别的进程/另一个窗口改过 → 挂起冲突，不覆盖。
   */
  const saveOne = useCallback(async (path: string): Promise<boolean> => {
    if (savingRef.current.has(path)) return false;
    const m = modelsRef.current.get(path);
    if (!m) return false;
    // 已挂起冲突的文件不再自动写，等用户决定
    if (conflictRef.current?.path === path) return false;
    const content = m.getValue();
    const expect = versionRef.current.get(path) ?? 0;
    savingRef.current.add(path);
    try {
      const st = await api.fsWriteChecked(path, content, expect);
      versionRef.current.set(path, st.mtimeMs);
      // 只有内容仍是刚才那份，才认为这个版本已落盘
      if (m.getValue() === content) {
        setDirty((s) => { const n = new Set(s); n.delete(path); return n; });
      }
      return true;
    } catch (e) {
      if (String(e).includes('CONFLICT')) {
        setConflict({ path, name: path.split(/[\\/]/).pop() ?? path });
        setStatus('');
      } else {
        setStatus('保存失败：' + String(e));
      }
      return false;
    } finally {
      savingRef.current.delete(path);
    }
  }, []);

  const saveAll = useCallback(async (): Promise<number> => {
    const paths = [...dirtyRef.current];
    const done: string[] = [];
    for (const p of paths) {
      if (await saveOne(p)) done.push(p);
    }
    const ok = done.length;
    if (ok > 0) {
      setStatus((s) => (s.startsWith('保存失败') ? s : `已保存 ${ok} 个文件`));
      onSaved(done);
      window.setTimeout(() => setStatus((s) => (s.startsWith('已保存') ? '' : s)), 2600);
    }
    return ok;
  }, [onSaved, saveOne]);

  const saveAllRef = useRef(saveAll);
  saveAllRef.current = saveAll;
  const saveOneRef = useRef(saveOne);
  saveOneRef.current = saveOne;

  /** 文件被别的进程改过时的两种处理：覆盖 或 丢弃本地改动重新加载 */
  const resolveConflict = useCallback(async (how: 'overwrite' | 'reload'): Promise<void> => {
    const c = conflictRef.current;
    setConflict(null);
    if (!c) return;
    const m = modelsRef.current.get(c.path);
    if (how === 'overwrite') {
      // 用户确认覆盖：以磁盘当前版本为准强制写
      const st = await api.fsStat(c.path).catch(() => ({ mtimeMs: 0, size: 0, exists: false }));
      versionRef.current.set(c.path, st.mtimeMs);
      await saveOne(c.path);
      setStatus('已覆盖保存');
    } else if (m) {
      const text = await api.fsRead(c.path).catch(() => null);
      if (text !== null) {
        m.setValue(text);
        const st = await api.fsStat(c.path).catch(() => ({ mtimeMs: 0, size: 0, exists: false }));
        versionRef.current.set(c.path, st.mtimeMs);
        setDirty((s) => { const n = new Set(s); n.delete(c.path); return n; });
        setStatus('已放弃本地改动，重新加载磁盘版本');
      }
    }
    window.setTimeout(() => setStatus(''), 2600);
  }, [saveOne]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.ctrlKey && e.code === 'KeyS') { e.preventDefault(); void saveAllRef.current(); }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, []);

  const closeDiff = useCallback((): void => {
    setDiffSpec(null);
    if (diffModelsRef.current) {
      diffModelsRef.current.original.dispose();
      diffModelsRef.current.modified.dispose();
      diffModelsRef.current = null;
    }
    diffEdRef.current?.setModel(null);
  }, []);

  const openDiff = useCallback((spec: DiffSpec): void => {
    const de = diffEdRef.current;
    if (!de) return;
    if (diffModelsRef.current) {
      diffModelsRef.current.original.dispose();
      diffModelsRef.current.modified.dispose();
    }
    const original = monaco.editor.createModel(spec.base, spec.lang);
    const modified = monaco.editor.createModel(spec.current, spec.lang);
    diffModelsRef.current = { original, modified };
    de.setModel({ original, modified });
    setDiffSpec(spec);
  }, []);

  const notifyActive = useCallback((p: string | null): void => {
    onActiveFile?.(p ? (p.split(/[\\/]/).pop() ?? '') : '');
  }, [onActiveFile]);

  const openFile = useCallback(async (path: string): Promise<void> => {
    if (diffSpec) closeDiff();
    let model = modelsRef.current.get(path);
    if (!model) {
      try {
        const text = await api.fsRead(path);
        const lang = path.toLowerCase().endsWith('.json') ? 'json' : LANG_ID;
        model = monaco.editor.createModel(text, lang, uriOf(path));
        const m = model;
        let timer = 0;
        let saveTimer = 0;
        // 记下磁盘版本，后续写盘用它做冲突校验
        void api.fsStat(path).then((st) => { if (st.exists) versionRef.current.set(path, st.mtimeMs); }).catch(() => {});
        m.onDidChangeContent(() => {
          setDirty((s) => new Set(s).add(path));
          if (timer) window.clearTimeout(timer);
          timer = window.setTimeout(() => { validateModel(m); reportProblems(m, path); }, 280);
          // 自动保存（设置里可关）——走和手动保存同一条串行通道，避免互相踩
          if (loadSettings().autoSave && path.toLowerCase().endsWith('.cw')) {
            if (saveTimer) window.clearTimeout(saveTimer);
            saveTimer = window.setTimeout(() => {
              void saveOneRef.current(path).then((done) => {
                if (done) setStatus('已自动保存');
              });
            }, 900);
          }
        });
        modelsRef.current.set(path, model);
        if (lang === LANG_ID) { validateModel(model); reportProblems(model, path); }
      } catch (e) { setStatus('打开失败：' + String(e)); return; }
    }
    setTabs((t) => (t.includes(path) ? t : [...t, path]));
    setActiveTab(path);
    setActiveDoc(null);   // 切回文件视图
    notifyActive(path);
    edRef.current?.setModel(model);
  }, [diffSpec, closeDiff, uriOf, notifyActive]);

  const openDoc = useCallback((doc: DocTab): void => {
    setDocTabs((list) => {
      const idx = list.findIndex((d) => d.id === doc.id);
      if (idx >= 0) {
        const next = [...list];
        next[idx] = doc;   // 复用已有标签，但内容换成最新的
        return next;
      }
      return [...list, doc];
    });
    setActiveDoc(doc.id);
    // 切到文档页时把编辑器模型解绑，避免 Monaco 在隐藏容器里算布局
    setDiffSpec(null);
  }, []);

  const closeDoc = useCallback((id: string): void => {
    setDocTabs((list) => {
      const next = list.filter((d) => d.id !== id);
      if (activeDocRef.current === id) {
        // 关掉当前页：优先退回上一个文档页，否则回文件标签
        const fallback = next[next.length - 1]?.id ?? null;
        setActiveDoc(fallback);
      }
      return next;
    });
  }, []);

  const closeTab = useCallback((path: string): void => {
    setTabs((t) => {
      const next = t.filter((p) => p !== path);
      if (activeTab === path) {
        const n = next[next.length - 1] ?? null;
        setActiveTab(n);
        if (n && !diffSpec) {
          const m = modelsRef.current.get(n);
          if (m) edRef.current?.setModel(m);
        } else if (!n && !diffSpec) edRef.current?.setModel(null);
      }
      return next;
    });
  }, [activeTab, diffSpec]);

  /** 标签栏「+」：在工程根新建一个 .cw 文件并打开 */
  const newFile = useCallback(async (): Promise<void> => {
    const name = await askText('新建文件', {
      placeholder: '文件名，如 tools.cw',
      hint: '不写后缀会自动补 .cw',
      validate: (v) => (/[\\/:*?"<>|]/.test(v) ? '名称里不能有 \\ / : * ? " < > |' : null)
    });
    if (!name) return;
    const finalName = /\.[A-Za-z0-9]{1,8}$/.test(name) ? name : name + '.cw';
    const root = projectDir.replace(/\\/g, '/').replace(/\/+$/, '');
    const path = `${root}/${finalName}`;
    try {
      await api.fsWrite(path, /\.cw$/i.test(finalName)
        ? '// 新建的 Float Script 文件\n\nprint("你好，Float Script")\n'
        : '');
      await openFile(path);
    } catch (e) {
      setStatus('新建失败：' + String(e));
    }
  }, [projectDir, openFile]);

  /** 外部（全局替换 / 重命名）改写缓冲区后，同步 mtime 再走一次保存 */
  const setContent = useCallback((path: string, text: string): void => {
    const m = modelsRef.current.get(path);
    if (!m) return;
    m.setValue(text);
    // setValue 会触发脏标记，这里紧接着刷新版本，保存时不会被当成"被外部改动"
    setDirty((s) => new Set(s).add(path));
    void api.fsStat(path).then((st) => { if (st.exists) versionRef.current.set(path, st.mtimeMs); }).catch(() => {});
  }, []);

  const revealLine = useCallback((path: string, line: number): void => {
    void openFile(path).then(() => edRef.current?.revealLineInCenter(line));
  }, [openFile]);

  useEffect(() => {
    (async () => {
      try {
        const raw = await api.fsRead(projectDir + '/project.json');
        const entry = (JSON.parse(raw) as any).entry as string;
        if (entry) void openFile(projectDir + '/' + entry.replace(/\\/g, '/'));
      } catch { /* 无 project.json 时静默 */ }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectDir]);

  useEffect(() => {
    bus.current.editor = {
      openFile, closeTab, saveAll, saveFile: saveOne, setContent, revealLine,
      dirtyCount: () => dirtyRef.current.size,
      getValue: (p) => modelsRef.current.get(p)?.getValue() ?? null,
      refreshBpDecor,
      openDiff, closeDiff,
      runAction, zoom, toggleBreakpoint,
      openDoc, closeDoc, newFile,
      activeDocId: () => activeDocRef.current,
      currentPath: () => activePathRef.current,
      cursor: () => {
        const pos = edRef.current?.getPosition();
        return pos ? { line: pos.lineNumber, col: pos.column } : null;
      },
      activeText: () => modelsRef.current.get(activePathRef.current ?? '')?.getValue() ?? '',
      activeSelection: () => {
        const ed2 = edRef.current;
        const sel = ed2?.getSelection();
        if (!ed2 || !sel) return '';
        return ed2.getModel()?.getValueInRange(sel) ?? '';
      },
      insertAtCursor: (text: string) => {
        const ed2 = edRef.current;
        const pos = ed2?.getPosition();
        if (!ed2 || !pos) return;
        ed2.executeEdits('plugin', [{ range: new monaco.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column), text, forceMoveMarkers: true }]);
        ed2.focus();
      },
      replaceActiveText: (text: string) => {
        const ed2 = edRef.current;
        const model = ed2?.getModel();
        if (!ed2 || !model) return;
        model.setValue(text);
      }
    };
    return () => { delete bus.current.editor; };
  }, [bus, openFile, closeTab, saveAll, saveOne, setContent, revealLine, refreshBpDecor, openDiff, closeDiff, runAction, zoom, toggleBreakpoint, openDoc, closeDoc, newFile]);

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, minHeight: 0 }}>
      <div className="tree-tabs">
        {/* 文档标签排在前面：教程/插件/MCP 这些是"常驻页面" */}
        {docTabs.map((d) => (
          <div
            key={`doc:${d.id}`}
            className={`tree-tab doc-tab ${activeDoc === d.id ? 'active' : ''}`}
            onClick={() => { closeDiff(); setActiveDoc(d.id); }}
            title={d.title}
          >
            <span className="tab-icon">{d.icon}</span>
            {d.title}
            <span className="tab-x" onClick={(e) => { e.stopPropagation(); closeDoc(d.id); }}>
              <IconClose size={11} />
            </span>
          </div>
        ))}
        {tabs.map((t) => (
          <div
            key={t}
            className={`tree-tab ${activeDoc === null && activeTab === t && !diffSpec ? 'active' : ''}`}
            onClick={() => { closeDiff(); void openFile(t); }}
            title={t}
          >
            {dirty.has(t) && <span className="dirty-dot" />}
            {t.split(/[\\/]/).pop()}
            <span className="tab-x" onClick={(e) => { e.stopPropagation(); closeTab(t); }}>
              <IconClose size={11} />
            </span>
          </div>
        ))}
        {diffSpec && (
          <div className="tree-tab active diff-tab">
            {diffSpec.title}
            <span className="tab-x" onClick={(e) => { e.stopPropagation(); closeDiff(); }}>
              <IconClose size={11} />
            </span>
          </div>
        )}
        {/* 新建标签：鼠标点这里就能新建文件，不用记 Ctrl+N */}
        <button className="tab-new" title="新建文件（Ctrl+N）" onClick={() => void newFile()}>
          <IconPlus size={12} />
        </button>
      </div>
      <div style={{ position: 'relative', flex: 1, minHeight: 0 }}>
        <div
          className="editor-host"
          ref={hostRef}
          style={{ display: diffSpec || activeDoc ? 'none' : 'block', height: '100%' }}
        />
        <div className="editor-host" ref={diffHostRef} style={{ display: diffSpec ? 'block' : 'none', height: '100%' }} />
        {diffSpec && <div className="diff-hint">对比视图（只读）：左侧基线，右侧当前内容。</div>}
        {/* 文档页面：占满编辑区 */}
        {activeDoc && (
          <div className="doc-host">
            {docTabs.find((d) => d.id === activeDoc)?.node ?? (
              <div className="doc-missing">这个页面已经关闭了</div>
            )}
          </div>
        )}
      </div>
      {conflict && (
        <div className="conflict-bar">
          <span className="dot warn" />
          <b>{conflict.name}</b> 已被另一个程序（或另一个 Float IDE 窗口）修改，为避免覆盖，本次保存已暂停。
          <button className="cbtn" onClick={() => void resolveConflict('overwrite')}>仍要覆盖</button>
          <button className="cbtn ghost" onClick={() => void resolveConflict('reload')}>放弃我的改动，重新加载</button>
        </div>
      )}
      {(status || dirty.size > 0) && !conflict && (
        <div className={`compile-status ${status.startsWith('保存失败') ? 'err' : 'ok'}`}>
          {status || `未保存的更改：${dirty.size} 个文件（Ctrl+S 保存全部）`}
        </div>
      )}
    </div>
  );
}
