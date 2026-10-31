import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../core/tauri';
import { Runner, type CtlState, type OutLine } from '../lang/interp';
import type { ConsoleLine, ProblemItem, ProjectMeta, VarRow } from '../core/types';
import { analyze } from '../lang/analyze';
import { setPluginContributions } from '../lang/floatlang';
import { EditorPane, type EditorHandle, type DocTab } from '../ide/EditorPane';
import { FileTree } from '../ide/FileTree';
import { GitPanel } from '../ide/GitPanel';
import { ConsoleDock } from '../ide/ConsoleDock';
import { SidePanels } from '../ide/SidePanels';
import { CommandPalette, type Command } from '../ide/CommandPalette';
import { ProjectIndex } from '../ide/indexer';
import { SearchPanel } from '../ide/SearchPanel';
import { PluginHost, type PluginCommandEntry } from '../plugins/runtime';
import { PluginPanel } from '../plugins/PluginPanel';
import { McpPanel } from '../mcp/McpPanel';
import { IconPlug, IconServer, IconBook, IconKeyboard, IconLayers } from '../ui/icons';
import { TutorialPage } from '../docs/TutorialPage';
import { Splitter } from '../ide/Splitter';
import {
  IconBranch, IconFile, IconStop, IconExport,
  IconStepInto, IconStepOut, IconStepOver, IconPlay, IconTerminal,
  IconBug, IconImage, IconSearch, IconSettings
} from '../ui/icons';
import { TitleBar } from '../ui/TitleBar';
import { MenuBar, type MenuGroup } from '../ui/MenuBar';
import { ShortcutsDialog } from '../ui/ShortcutsDialog';
import { SettingsDialog } from '../ui/SettingsDialog';
import { AppearanceDialog } from '../ui/AppearanceDialog';

/* Float IDE 工作区：左 IDE（文件/Git/编辑器）+ 右大纲/变量 + 底部控制台/终端。
 * 运行与调试由 Runner（生成器解释器）驱动。 */

type Props = {
  projectDir: string;
  /** 进来就直接打开的文件（双击 .cw 启动时用） */
  initialFile?: string;
  /** 打开工程外的文件时，交给上层重新挂载工作区 */
  onOpenTarget?: (path: string) => void;
  onExit: () => void;
};

export function Workspace({ projectDir, initialFile, onOpenTarget, onExit }: Props) {
  const [meta, setMeta] = useState<ProjectMeta | null>(null);
  const [sideTab, setSideTab] = useState<'files' | 'git'>('files');
  const [dockTab, setDockTab] = useState<'console' | 'terminal' | 'search'>('console');
  const [dockOpen, setDockOpen] = useState(true);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [lines, setLines] = useState<ConsoleLine[]>([]);
  const [vars, setVars] = useState<VarRow[]>([]);
  const [ctl, setCtl] = useState<CtlState>('idle');
  const [execPos, setExecPos] = useState<{ file: string; line: number } | null>(null);
  const [askQ, setAskQ] = useState<string | null>(null);
  const [dirtyCount, setDirtyCount] = useState(0);
  const [entrySrc, setEntrySrc] = useState('');
  const [entryFile, setEntryFile] = useState('');
  const [execMode, setExecMode] = useState<'编译执行' | '解释执行'>('编译执行');
  const [activeName, setActiveName] = useState('');
  const [activePath, setActivePath] = useState<string | null>(null);
  const [tabRequest, setTabRequest] = useState<{ tab: 'outline' | 'vars' | 'problems'; seq: number }>({ tab: 'outline', seq: 0 });
  const [cursor, setCursor] = useState<{ line: number; col: number }>({ line: 1, col: 1 });
  const [problems, setProblems] = useState<ProblemItem[]>([]);
  const [runProblems, setRunProblems] = useState<ProblemItem[]>([]);
  /* 可拖动布局（记住上次的位置） */
  const [layout, setLayout] = useState(() => {
    try {
      const raw = window.localStorage.getItem('float.layout');
      if (raw) {
        const l = JSON.parse(raw) as { sideW?: number; rightW?: number; dockH?: number };
        return { sideW: l.sideW ?? 240, rightW: l.rightW ?? 268, dockH: l.dockH ?? 176 };
      }
    } catch { /* 用默认值 */ }
    return { sideW: 240, rightW: 268, dockH: 176 };
  });
  const setPanel = useCallback((k: 'sideW' | 'rightW' | 'dockH', v: number): void => {
    setLayout((prev) => {
      const next = { ...prev, [k]: v };
      try { window.localStorage.setItem('float.layout', JSON.stringify(next)); } catch { /* 忽略 */ }
      return next;
    });
  }, []);

  const bus = useRef<{ editor?: EditorHandle }>({});
  /* 无头测试钩子：只有 ?debug=1 才挂到 window，正常使用不暴露 */
  useEffect(() => {
    if (new URLSearchParams(location.search).get('debug') !== '1') return;
    (window as unknown as { __floatBus?: unknown }).__floatBus = bus.current;
  }, []);

  /* 工程索引：跨文件搜索、跳转定义、重命名都靠它 */
  const indexRef = useRef<ProjectIndex | null>(null);
  if (!indexRef.current) {
    indexRef.current = new ProjectIndex(
      projectDir,
      (p) => api.fsRead(p),
      async (d) => (await api.fsList(d)).map((e) => ({ name: e.name, isDir: e.isDir, relPath: e.relPath }))
    );
  }
  const [indexSize, setIndexSize] = useState(0);
  const [searchFocus, setSearchFocus] = useState(0);
  const [pluginCommands, setPluginCommands] = useState<PluginCommandEntry[]>([]);
  const [pluginMenus, setPluginMenus] = useState<{ command: string; label: string }[]>([]);
  const [pluginTick, setPluginTick] = useState(0);
  const [mcpRunning, setMcpRunning] = useState(false);
  const [mcpPort, setMcpPort] = useState(0);
  const breakpoints = useRef<Map<string, Set<number>>>(new Map());
  const allProblems = React.useMemo(() => {
    const seen = new Set<string>();
    const out: ProblemItem[] = [];
    for (const p of [...runProblems, ...problems]) {
      const key = `${p.file}:${p.line}:${p.message}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(p);
    }
    return out.sort((a, b) => (a.severity === b.severity ? a.line - b.line : a.severity === 'error' ? -1 : 1));
  }, [problems, runProblems]);
  const errorCount = allProblems.filter((p) => p.severity === 'error').length;
  const warnCount = allProblems.filter((p) => p.severity === 'warning').length;

  const runnerRef = useRef<Runner | null>(null);
  const stateRef = useRef<CtlState>('idle');
  stateRef.current = ctl;
  /* 运行互斥：ctl 是异步 setState，连点 F5 时两次检查都会看到 idle 而起两个 Runner。
   * 这里用同步 ref 当闸门，并给每次运行发一个令牌，旧 Runner 的回调一律丢弃。 */
  const busyRef = useRef(false);
  const runTokenRef = useRef(0);

  const pushLine = useCallback((l: OutLine): void => {
    setLines((ls) => [...ls.slice(-500), { kind: l.kind, text: l.text, ts: Date.now() }]);
  }, []);

  /* 工程元信息 + 入口文件内容（大纲用） */
  const reloadEntry = useCallback(async (): Promise<void> => {
    try {
      const raw = await api.fsRead(projectDir + '/project.json');
      const m = JSON.parse(raw) as ProjectMeta;
      setMeta(m);
      document.title = `Float IDE — ${m.name}`;
      const ef = projectDir + '/' + (m.entry ?? 'main.cw');
      setEntryFile(ef);
      try { setEntrySrc(await api.fsRead(ef)); } catch { setEntrySrc(''); }
    } catch {
      // 没有 project.json 也照样能开（用户可能只是随手打开一个放脚本的文件夹）：
      // 入口优先用传进来的那个文件，其次找目录里第一个 .cw
      const base = projectDir.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || '未命名工程';
      let entry = initialFile ? initialFile.split(/[\\/]/).pop() ?? 'main.cw' : 'main.cw';
      if (!initialFile) {
        try {
          const items = await api.fsList(projectDir);
          const cw = items.find((it) => !it.isDir && it.name.toLowerCase().endsWith('.cw'));
          if (cw) entry = cw.name;
        } catch { /* 空目录 */ }
      }
      setMeta({ name: base, entry, version: '0.0.0', kind: 'float' } as ProjectMeta);
      document.title = `Float IDE — ${base}`;
      const ef = projectDir + '/' + entry;
      setEntryFile(ef);
      try { setEntrySrc(await api.fsRead(ef)); } catch { setEntrySrc(''); }
    }
  }, [projectDir, initialFile, pushLine]);

  useEffect(() => { void reloadEntry(); }, [reloadEntry]);

  /* 双击 .cw 进来时，光打开工程还不够，得把那个文件也开成标签页。
   * 编辑器是异步挂载的，等它出现在 bus 上再开（最多等 3 秒）。 */
  useEffect(() => {
    if (!initialFile) return;
    let tries = 0;
    const t = window.setInterval(() => {
      const ed = bus.current.editor;
      tries++;
      if (ed) {
        window.clearInterval(t);
        void ed.openFile(initialFile);
      } else if (tries > 30) {
        window.clearInterval(t);
      }
    }, 100);
    return () => window.clearInterval(t);
  }, [initialFile]);

  /* 工程占用锁：两个 Float IDE 同时改同一个工程是最容易丢代码的场景。
   * 抢不到锁就明确提示（仍可只读查看），不抢别人的锁。 */
  const instanceId = useRef('w' + Date.now().toString(36) + Math.random().toString(36).slice(2, 8));
  const [lockWarn, setLockWarn] = useState<string | null>(null);
  useEffect(() => {
    let beat = 0;
    let held = false;
    (async () => {
      try {
        await api.lockAcquire(projectDir, instanceId.current);
        held = true;
        setLockWarn(null);
      } catch (e) {
        const msg = String(e).replace(/^Error:\s*/, '');
        setLockWarn(msg);
        pushLine({ kind: 'err', text: `⚠ ${msg}。为避免互相覆盖，请先关闭另一个窗口。` });
      }
    })();
    beat = window.setInterval(() => {
      if (!held) return;
      void api.lockTouch(projectDir, instanceId.current).then((ok) => {
        if (!ok) setLockWarn('本窗口的工程锁已被回收（可能长时间未响应），请重新打开工程');
      }).catch(() => { /* 预览环境忽略 */ });
    }, 4000);
    const release = (): void => {
      if (!held) return;
      held = false;
      void api.lockRelease(projectDir, instanceId.current).catch(() => {});
    };
    window.addEventListener('beforeunload', release);
    return () => {
      window.clearInterval(beat);
      window.removeEventListener('beforeunload', release);
      release();
    };
  }, [projectDir, pushLine]);

  const openOneFile = useCallback(async (): Promise<void> => {
    const f = await api.pickOpenFile('打开 CW Script 文件', ['cw']);
    if (!f) return;
    const norm = f.replace(/\\/g, '/');
    const root = projectDir.replace(/\\/g, '/').replace(/\/+$/, '');
    if (norm.startsWith(root + '/')) {
      await bus.current.editor?.openFile(f);
    } else if (onOpenTarget) {
      onOpenTarget(f);
    } else {
      await bus.current.editor?.openFile(f);
    }
  }, [projectDir, onOpenTarget]);

  const onSaved = useCallback((paths: string[]): void => {
    void reloadEntry();
    // 保存后立刻把新内容灌回索引，搜索/跳转不用等下一次全量扫描
    const idx = indexRef.current;
    if (idx) {
      for (const p of paths) {
        const text = bus.current.editor?.getValue(p);
        if (text !== null && text !== undefined) idx.put(p, text);
      }
      setIndexSize(idx.size);
    }
  }, [reloadEntry]);

  /* 打开工程时先建一次索引（异步，不阻塞界面） */
  useEffect(() => {
    const idx = indexRef.current;
    if (!idx) return;
    let alive = true;
    void idx.refresh().then((n) => { if (alive) setIndexSize(n); }).catch(() => {});
    return () => { alive = false; };
  }, [projectDir]);

  /* 导出工程包（.floatzip），与欢迎页「导入工程包」配对 */
  const doExport = useCallback(async (): Promise<void> => {
    const out = await api.pickSaveFile('导出工程包', (meta?.name ?? 'Float工程') + '.cw', 'cw');
    if (!out) return;
    try {
      await api.projectExportZip(projectDir, out);
      pushLine({ kind: 'info', text: `工程包已导出：${out}` });
    } catch (e) {
      pushLine({ kind: 'err', text: `导出失败：${String(e)}` });
    }
  }, [projectDir, meta, pushLine]);

  /* 递归收集工程内全部 .cw 内容，供解释器 import */
  const collectSources = useCallback(async (): Promise<Map<string, string>> => {
    const out = new Map<string, string>();
    const walk = async (dir: string): Promise<void> => {
      let entries;
      try { entries = await api.fsList(dir); } catch { return; }
      for (const e of entries) {
        const p = dir + '/' + e.name;
        if (e.isDir) await walk(p);
        else if (e.name.toLowerCase().endsWith('.cw')) {
          try { out.set(p.replace(/\\/g, '/'), await api.fsRead(p)); } catch { /* 跳过 */ }
        }
      }
    };
    await walk(projectDir);
    return out;
  }, [projectDir]);

  const startRun = useCallback(async (mode: 'run' | 'debug'): Promise<void> => {
    // 同步闸门：哪怕同一帧内被触发多次，也只有第一次能进来
    if (busyRef.current) { pushLine({ kind: 'info', text: '正在启动/运行中，请勿重复触发' }); return; }
    if (!meta) { pushLine({ kind: 'err', text: '工程未加载完成' }); return; }
    busyRef.current = true;
    const token = ++runTokenRef.current;
    const alive = (): boolean => token === runTokenRef.current;
    try {
      // 先掐掉可能还活着的旧 Runner，避免两个脚本同时往控制台写
      const prev = runnerRef.current;
      if (prev) { prev.stop(); runnerRef.current = null; }
      await bus.current.editor?.saveAll();
      if (!alive()) return;
      setDockOpen(true);
      setDockTab('console');
      const sources = await collectSources();
      if (!alive()) return;
      (window as any).__floatSources = sources;
      const entry = projectDir + '/' + (meta.entry ?? 'main.cw');
      const normEntry = entry.replace(/\\/g, '/');
      if (!sources.has(normEntry)) {
        pushLine({ kind: 'err', text: `找不到入口文件：${meta.entry ?? 'main.cw'}` });
        return;
      }
      setLines([]);
      setRunProblems([]);
      setExecMode(mode === 'debug' ? '解释执行' : '编译执行');
      const runner = new Runner(normEntry, {
        output: (l) => { if (alive()) pushLine(l); },
        onError: (info) => { if (!alive()) return; setRunProblems((prev2) => [...prev2, {
          severity: 'error', message: info.message, file: info.file, line: info.line, col: 1, from: 'run'
        }]); },
        onState: (s) => { if (alive()) setCtl(s); },
        onPaused: (node, snapshot) => {
          if (!alive()) return;
          setVars(snapshot);
          if (node) {
            setExecPos({ file: node.file.replace(/\//g, '\\'), line: node.line });
          } else {
            setExecPos(null);
          }
          // wait 暂停自动继续；ask 暂停等用户回答；断点/单步暂停等用户操作
          window.setTimeout(() => {
            if (!alive()) return;
            const r = runnerRef.current;
            if (!r || r.state !== 'paused') return;
            if (r.pauseReason === 'wait') { r.resume(); return; }
            if (r.pauseReason === 'ask') setAskQ(r.askQuestion);
          }, 0);
        },
        onDone: () => {
          if (!alive()) return;   // 旧 Runner 结束后不许清掉新 Runner 的状态
          setExecPos(null);
          setAskQ(null);
          runnerRef.current = null;
        }
      }, projectDir);
      runnerRef.current = runner;
      // 断点同步
      for (const [f, ls] of breakpoints.current) {
        runner.setBreakpoints(f.replace(/\\/g, '/'), [...ls]);
      }
      setAskQ(null);
      runner.start(mode);
    } catch (e) {
      pushLine({ kind: 'err', text: '启动失败：' + String(e) });
    } finally {
      busyRef.current = false;
    }
  }, [meta, projectDir, pushLine, collectSources]);

  const doStop = useCallback((): void => {
    // 作废当前令牌：旧回调不再生效，避免停止后又被异步事件点亮
    runTokenRef.current += 1;
    runnerRef.current?.stop();
    runnerRef.current = null;
    setCtl('idle');
    setExecPos(null);
    setAskQ(null);
  }, []);

  const doStep = useCallback((mode: 'into' | 'over' | 'out'): void => {
    const r = runnerRef.current;
    if (!r || r.state !== 'paused') return;
    if (r.askQuestion !== null) { pushLine({ kind: 'info', text: '先回答控制台里的提问' }); return; }
    r.setStep(mode);
  }, []);

  const doResume = useCallback((): void => {
    const r = runnerRef.current;
    if (!r || r.state !== 'paused') return;
    if (r.askQuestion !== null) return;
    setAskQ(null);
    r.continueRun();
  }, []);

  /* ─── 插件宿主 ─────────────────────────────────────────── */
  /** 跑一段「一次性」脚本：不占主控制台状态，也不污染主工程的源码表 */
  const runCodeQuiet = useCallback(async (code: string): Promise<{ out: string[]; error: string | null }> => {
    const out: string[] = [];
    let error: string | null = null;
    const file = projectDir.replace(/\\/g, '/') + '/__plugin__.cw';
    const sources = new Map<string, string>([[file, code]]);
    await new Promise<void>((done) => {
      let settled = false;
      const finish = (): void => { if (!settled) { settled = true; done(); } };
      const r = new Runner(file, {
        output: (l) => { if (l.kind === 'out') out.push(l.text); else if (l.kind === 'err') error = l.text; },
        onError: (e) => { error = e.message; },
        onState: () => {},
        onPaused: () => { window.setTimeout(() => r.resume(), 0); },
        onDone: finish
      }, projectDir, sources);
      try { r.start('run'); } catch (e) { error = String(e); finish(); }
      window.setTimeout(finish, 8000);
    });
    return { out, error };
  }, [projectDir]);

  const pluginHostRef = useRef<PluginHost | null>(null);
  if (!pluginHostRef.current) {
    pluginHostRef.current = new PluginHost({
      projectDir,
      log: (kind, text) => pushLine({
        kind: kind === 'err' ? 'err' : kind === 'out' ? 'out' : 'info',
        text: `[插件] ${text}`
      }),
      notify: (text) => pushLine({ kind: 'info', text: `[插件] ${text}` }),
      editor: {
        getText: () => bus.current.editor?.activeText() ?? '',
        setText: (t) => bus.current.editor?.replaceActiveText(t),
        insert: (t) => bus.current.editor?.insertAtCursor(t),
        getSelection: () => bus.current.editor?.activeSelection() ?? '',
        getPath: () => bus.current.editor?.currentPath() ?? null,
        getCursor: () => bus.current.editor?.cursor() ?? null
      },
      runCode: runCodeQuiet
    });
  }

  useEffect(() => {
    const host = pluginHostRef.current;
    if (!host) return;
    let alive = true;
    const sync = (): void => {
      if (!alive) return;
      setPluginCommands(host.commandList);
      setPluginMenus(host.menus);
      setPluginTick((n) => n + 1);
      // 插件片段 / 关键词注入编辑器补全，让 contributes.snippets 真的生效
      setPluginContributions(host.snippetList, host.keywordList);
    };
    const off = host.onChange(sync);
    void host.init().then(sync).catch(() => sync());
    return () => { alive = false; off(); };
  }, [projectDir]);

  /* 状态栏的 MCP 灯：3 秒问一次（很轻），只有变化才 setState */
  useEffect(() => {
    let alive = true;
    const tick = (): void => {
      void api.mcpStatus().then((st) => {
        if (!alive) return;
        setMcpRunning((v) => (v === st.running ? v : st.running));
        setMcpPort((v) => (v === st.port ? v : st.port));
      }).catch(() => { /* 预览环境忽略 */ });
    };
    tick();
    const t = window.setInterval(tick, 3000);
    return () => { alive = false; window.clearInterval(t); };
  }, []);

  /* MCP：外部客户端要跑脚本 / 拿诊断 / 要大纲时，转发给这里执行 */
  useEffect(() => {
    let un: (() => void) | undefined;
    let alive = true;
    void (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const off = await listen<{ id: string; tool: string; args: Record<string, unknown>; root: string }>(
          'mcp:invoke',
          (ev) => {
            const { id, tool, args } = ev.payload;
            const reply = (payload: unknown): void => {
              void api.mcpReply(id, JSON.stringify(payload)).catch(() => {});
            };
            void (async () => {
              try {
                if (tool === 'run_script') {
                  const code = String(args.code ?? '');
                  const res = await runCodeQuiet(code);
                  reply({ text: res.error ? `错误：${res.error}` : res.out.join('\n') });
                  return;
                }
                const rel = String(args.path ?? '');
                const abs = projectDir.replace(/\\/g, '/') + '/' + rel.replace(/^\/+/, '');
                const text = await api.fsRead(abs);
                const a = analyze(abs, text);
                if (tool === 'get_diagnostics') {
                  reply({
                    text: JSON.stringify(a.diagnostics.map((d) => ({
                      severity: d.severity, message: d.message,
                      line: d.span.line, col: d.span.col
                    })), null, 2)
                  });
                  return;
                }
                // get_outline
                reply({
                  text: JSON.stringify({
                    functions: a.funcs.map((f) => ({ name: f.name, line: f.line, params: f.params.map((p) => p.name), owner: f.owner })),
                    classes: a.classes.map((c) => ({ name: c.name, line: c.line, base: c.base, methods: c.methods.map((m) => m.name), fields: c.fields.map((f) => f.name) })),
                    variables: a.vars.map((v) => ({ name: v.name, type: v.type, line: v.line }))
                  }, null, 2)
                });
              } catch (e) {
                reply({ error: String(e) });
              }
            })();
          }
        );
        if (alive) un = off; else off();
      } catch { /* 预览环境没有 Tauri 事件 */ }
    })();
    return () => { alive = false; un?.(); };
  }, [projectDir, runCodeQuiet]);

  /* ─── 代码导航：跳转定义 / 重命名 / 快速开文件 ─── */
  const gotoDefinition = useCallback((): void => {
    const ed = bus.current.editor;
    const idx = indexRef.current;
    const p = ed?.currentPath();
    if (!ed || !idx || !p) return;
    const pos = ed.cursor();
    if (!pos) return;
    const d = idx.definition(p, pos.line, pos.col);
    if (!d) { pushLine({ kind: 'info', text: '光标下没有可跳转的定义' }); return; }
    if (d.file === p.replace(/\\/g, '/') && d.line === pos.line) {
      pushLine({ kind: 'info', text: `这里就是 ${d.name} 的定义处` });
      return;
    }
    ed.revealLine(d.file, d.line);
    pushLine({ kind: 'info', text: `跳转到 ${d.kind} ${d.name}（${d.file.split(/[\\/]/).pop()} 第 ${d.line} 行）` });
  }, [pushLine]);

  const [rename, setRename] = useState<{ name: string; files: number; count: number } | null>(null);
  const [renameTo, setRenameTo] = useState('');

  const startRename = useCallback((): void => {
    const ed = bus.current.editor;
    const idx = indexRef.current;
    const p = ed?.currentPath();
    if (!ed || !idx || !p) return;
    const pos = ed.cursor();
    if (!pos) return;
    const word = idx.wordAt(p, pos.line, pos.col);
    if (!word) { pushLine({ kind: 'info', text: '光标要放在要改名的名字上' }); return; }
    const refs = idx.references(word);
    setRename({ name: word, files: new Set(refs.map((r) => r.file)).size, count: refs.length });
    setRenameTo(word);
  }, [pushLine]);

  const applyRename = useCallback(async (): Promise<void> => {
    const idx = indexRef.current;
    const ed = bus.current.editor;
    const r = rename;
    if (!idx || !ed || !r) return;
    const to = renameTo.trim();
    if (!to || to === r.name) { setRename(null); return; }
    if (!/^[A-Za-z_\u4e00-\u9fa5][\w\u4e00-\u9fa5]*$/.test(to)) {
      pushLine({ kind: 'err', text: `「${to}」不是合法的名字（只能用中英文、数字、下划线，且不能以数字开头）` });
      return;
    }
    const changes = idx.rename(r.name, to);
    for (const c of changes) {
      await ed.openFile(c.file);
      ed.setContent(c.file, c.text);
      await ed.saveFile(c.file);
      idx.put(c.file, c.text);
    }
    setIndexSize(idx.size);
    pushLine({ kind: 'info', text: `已把 ${r.name} 改名为 ${to}：${changes.length} 个文件 / ${changes.reduce((n, c) => n + c.count, 0)} 处` });
    setRename(null);
  }, [rename, renameTo, pushLine]);

  const openSearch = useCallback((): void => {
    setDockOpen(true);
    setDockTab('search');
    setSearchFocus((n) => n + 1);
  }, []);

  /** 把教程/插件里的示例代码开成一个真文件（放在 .float/samples 下，不进索引也不污染源码树） */
  const openCodeAsTempFile = useCallback(async (code: string, name: string): Promise<void> => {
    const root = projectDir.replace(/\\/g, '/').replace(/\/+$/, '');
    const stamp = Date.now().toString(36);
    const safe = name.replace(/[\\/:*?"<>|]/g, '_');
    const file = `${root}/.float/samples/${stamp}_${safe}`;
    try {
      await api.fsWrite(file, code);
      await bus.current.editor?.openFile(file);
    } catch (e) {
      pushLine({ kind: 'err', text: '打开示例失败：' + String(e) });
    }
  }, [projectDir, pushLine]);

  /* ─── 一整页的界面：全都开成标签页，不再用悬浮窗 ─── */
  const openTutorial = useCallback((): void => {
    bus.current.editor?.openDoc({
      id: 'tutorial',
      title: '学习中心',
      icon: <IconBook size={12} />,
      node: (
        <TutorialPage
          onOpenInEditor={(code, name) => {
            void openCodeAsTempFile(code, name);
          }}
          onRun={(code) => { void runCodeQuiet(code).then((r) => {
            for (const line of r.out) pushLine({ kind: 'out', text: line });
            if (r.error) pushLine({ kind: 'err', text: r.error });
            setDockOpen(true);
            setDockTab('console');
          }); }}
        />
      )
    });
  }, [pushLine, runCodeQuiet]);

  const openPluginsPage = useCallback((): void => {
    const host = pluginHostRef.current;
    if (!host) return;
    bus.current.editor?.openDoc({
      id: 'plugins',
      title: '插件',
      icon: <IconPlug size={12} />,
      node: (
        <PluginPanel
          host={host}
          onChanged={() => {
            setPluginCommands(host.commandList);
            setPluginMenus(host.menus);
          }}
          onClose={() => bus.current.editor?.closeDoc('plugins')}
          embedded
        />
      )
    });
  }, []);

  const openMcpPage = useCallback((): void => {
    bus.current.editor?.openDoc({
      id: 'mcp',
      title: 'MCP 服务器',
      icon: <IconServer size={12} />,
      node: <McpPanel projectDir={projectDir} onClose={() => bus.current.editor?.closeDoc('mcp')} embedded />
    });
  }, [projectDir]);

  const openSettingsPage = useCallback((): void => {
    bus.current.editor?.openDoc({
      id: 'settings',
      title: '偏好设置',
      icon: <IconLayers size={12} />,
      node: <SettingsDialog onClose={() => bus.current.editor?.closeDoc('settings')} onAppearance={() => openAppearancePage()} embedded />
    });
  }, []);

  const openShortcutsPage = useCallback((): void => {
    bus.current.editor?.openDoc({
      id: 'shortcuts',
      title: '快捷键',
      icon: <IconKeyboard size={12} />,
      node: <ShortcutsDialog onClose={() => bus.current.editor?.closeDoc('shortcuts')} embedded />
    });
  }, []);

  const openAppearancePage = useCallback((): void => {
    bus.current.editor?.openDoc({
      id: 'appearance',
      title: '外观与背景',
      icon: <IconImage size={12} />,
      node: <AppearanceDialog onClose={() => bus.current.editor?.closeDoc('appearance')} embedded />
    });
  }, []);

  /* 快速开文件：Ctrl+P */
  const [quickOpen, setQuickOpen] = useState<{ q: string; list: string[]; sel: number } | null>(null);

  /* F 键 */
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      const mod = e.ctrlKey || e.metaKey;
      const ed = bus.current.editor;

      // Esc 关掉当前的临时浮层（快速打开 / 命令面板）——
      // 不能只靠输入框自己的 onKeyDown，焦点一跑就关不掉了
      if (e.key === 'Escape' || e.code === 'Escape') {
        if (quickOpen) { e.preventDefault(); setQuickOpen(null); return; }
        if (paletteOpen) { e.preventDefault(); setPaletteOpen(false); return; }
      }

      // 运行与调试
      if (e.code === 'F5') { e.preventDefault(); if (e.shiftKey) doStop(); else void startRun('run'); return; }
      if (e.code === 'F6') { e.preventDefault(); void startRun('debug'); return; }
      if (e.code === 'F8') { e.preventDefault(); doResume(); return; }
      if (e.code === 'F9') { e.preventDefault(); ed?.toggleBreakpoint(); pushLine({ kind: 'info', text: '已在当前行切换断点' }); return; }
      if (e.code === 'F10') { e.preventDefault(); doStep('over'); return; }
      if (e.code === 'F11') { e.preventDefault(); if (e.shiftKey) doStep('out'); else doStep('into'); return; }
      if (e.code === 'F1') { e.preventDefault(); openShortcutsPage(); return; }

      // 视图
      if (e.altKey && e.code === 'Digit1') { e.preventDefault(); setSideTab('files'); return; }
      if (e.altKey && e.code === 'Digit2') { e.preventDefault(); setSideTab('git'); return; }
      if (e.altKey && e.code === 'Digit3') { e.preventDefault(); setTabRequest((t) => ({ tab: 'outline', seq: t.seq + 1 })); return; }
      if (mod && e.code === 'Backquote') { e.preventDefault(); setDockOpen((v) => !v); return; }
      if (mod && e.shiftKey && e.code === 'KeyF') { e.preventDefault(); openSearch(); return; }
      if (mod && e.shiftKey && e.code === 'KeyH') { e.preventDefault(); openTutorial(); return; }
      // 注意要排除 Shift：Ctrl+Shift+P 是命令面板，不能被快速开文件抢走
      if (mod && !e.shiftKey && e.code === 'KeyP') { e.preventDefault(); setQuickOpen({ q: '', list: [], sel: 0 }); return; }
      // Ctrl+Space：手动唤出补全（IDE 标准键位）
      if (mod && e.code === 'Space') { e.preventDefault(); ed?.runAction('editor.action.triggerSuggest'); return; }
      // Ctrl+/ 之外再给一个「参数提示」
      if (mod && e.shiftKey && e.code === 'Space') { e.preventDefault(); ed?.runAction('editor.action.triggerParameterHints'); return; }
      if (e.code === 'F12') { e.preventDefault(); gotoDefinition(); return; }
      if (e.code === 'F2') { e.preventDefault(); startRename(); return; }

      // 编辑与文件
      if (mod && e.shiftKey && e.code === 'KeyP') { e.preventDefault(); setPaletteOpen(true); return; }
      if (mod && e.shiftKey && e.code === 'KeyE') { e.preventDefault(); void doExport(); return; }
      if (mod && e.code === 'KeyW') {
        e.preventDefault();
        const p = ed?.currentPath();
        if (p) ed?.closeTab(p);
        return;
      }
      if (mod && (e.code === 'Equal' || e.code === 'NumpadAdd')) { e.preventDefault(); ed?.zoom(1); return; }
      if (mod && (e.code === 'Minus' || e.code === 'NumpadSubtract')) { e.preventDefault(); ed?.zoom(-1); return; }
      if (mod && e.code === 'Digit0') { e.preventDefault(); ed?.zoom(0); return; }
      if (mod && e.altKey && e.code === 'KeyS') { e.preventDefault(); openSettingsPage(); return; }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [startRun, doResume, doStep, doStop, doExport, pushLine, gotoDefinition, startRename, openSearch, openTutorial, openShortcutsPage, openSettingsPage, quickOpen, paletteOpen]);

  /* F9：提示断点用法（行级断点由编辑器 gutter 点击切换） */
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.code !== 'F9') return;
      e.preventDefault();
      pushLine({ kind: 'info', text: '断点：点击编辑器行号左侧空白处切换' });
    };
    return () => window.removeEventListener('keydown', onKey);
  }, [pushLine]);

  const running = ctl === 'running' || ctl === 'paused';
  const stateText = ctl === 'idle' ? '就绪'
    : ctl === 'running' ? '运行中'
    : ctl === 'paused' ? (askQ !== null ? '等待回答' : '已暂停（断点/单步）')
    : '已结束';

  const commands: Command[] = [
    { id: 'run', title: '运行脚本', hint: 'F5', run: () => void startRun('run') },
    { id: 'debug', title: '调试运行（断点生效）', hint: 'F6', run: () => void startRun('debug') },
    { id: 'stop', title: '停止脚本', hint: 'Shift+F5', run: doStop },
    { id: 'resume', title: '调试：继续执行', hint: 'F8', run: doResume },
    { id: 'over', title: '调试：单步跳过', hint: 'F10', run: () => doStep('over') },
    { id: 'into', title: '调试：单步进入', hint: 'F11', run: () => doStep('into') },
    { id: 'out', title: '调试：单步跳出', hint: 'Shift+F11', run: () => doStep('out') },
    { id: 'save', title: '保存全部文件', hint: 'Ctrl+S', run: () => void bus.current.editor?.saveAll() },
    { id: 'files', title: '打开资源管理器面板', run: () => setSideTab('files') },
    { id: 'git', title: '打开源代码管理（Git）', run: () => setSideTab('git') },
    { id: 'console', title: '显示控制台', hint: 'Ctrl+`', run: () => { setDockOpen(true); setDockTab('console'); } },
    { id: 'term', title: '打开终端', hint: 'Ctrl+`', run: () => { setDockOpen(true); setDockTab('terminal'); } },
    { id: 'reveal', title: '在文件资源管理器中显示工程目录', run: () => void api.openPath(projectDir).catch(() => {}) },
    { id: 'export', title: '导出工程包（.floatzip）', run: () => void doExport() },
    { id: 'clear', title: '清空控制台', run: () => setLines([]) },
    { id: 'exit', title: '关闭工程，返回欢迎页', run: () => { doStop(); onExit(); } },
    { id: 'tutorial', title: '学习中心（CW Script 教程）', hint: 'Ctrl+Shift+H', run: () => openTutorial() },
    { id: 'plugins', title: '插件管理', run: () => openPluginsPage() },
    { id: 'mcp', title: 'MCP 服务器（让外部 AI 用这个工程）', run: () => openMcpPage() },
    ...pluginCommands.map((c) => ({
      id: `plugin.${c.id}`,
      title: c.title,
      hint: c.category,
      run: () => { void pluginHostRef.current?.execute(c.id); }
    }))
  ];

  const menus: MenuGroup[] = [
    {
      label: '文件',
      items: [
        { label: '打开文件…', hint: 'Ctrl+O', run: () => void openOneFile() },
        { sep: true, label: 's0' },
        { label: '保存全部', hint: 'Ctrl+S', run: () => void bus.current.editor?.saveAll() },
        { label: '导出工程包', hint: 'Ctrl+Shift+E', run: () => void doExport() },
        { sep: true, label: 's1' },
        { label: '关闭当前文件', hint: 'Ctrl+W', run: () => { const p = bus.current.editor?.currentPath(); if (p) bus.current.editor?.closeTab(p); } },
        { label: '关闭工程，返回欢迎页', run: () => { doStop(); onExit(); } }
      ]
    },
    {
      label: '编辑',
      items: [
        { label: '撤销', hint: 'Ctrl+Z', run: () => bus.current.editor?.runAction('undo') },
        { label: '重做', hint: 'Ctrl+Y', run: () => bus.current.editor?.runAction('redo') },
        { sep: true, label: 's1' },
        { label: '查找', hint: 'Ctrl+F', run: () => bus.current.editor?.runAction('actions.find') },
        { label: '替换', hint: 'Ctrl+H', run: () => bus.current.editor?.runAction('editor.action.startFindReplaceAction') },
        { label: '跳到行', hint: 'Ctrl+G', run: () => bus.current.editor?.runAction('editor.action.gotoLine') },
        { label: '注释 / 取消注释', hint: 'Ctrl+/', run: () => bus.current.editor?.runAction('editor.action.commentLine') },
        { sep: true, label: 's2' },
        { label: '命令面板', hint: 'Ctrl+Shift+P', run: () => setPaletteOpen(true) }
      ]
    },
    {
      label: '视图',
      items: [
        { label: '资源管理器', hint: 'Alt+1', run: () => setSideTab('files') },
        { label: '源代码管理', hint: 'Alt+2', run: () => setSideTab('git') },
        { label: '大纲', hint: 'Alt+3', run: () => setTabRequest((t) => ({ tab: 'outline', seq: t.seq + 1 })) },
        { label: '变量', run: () => setTabRequest((t) => ({ tab: 'vars', seq: t.seq + 1 })) },
        { sep: true, label: 's1' },
        { label: dockOpen ? '隐藏控制台' : '显示控制台', hint: 'Ctrl+`', run: () => setDockOpen((v) => !v) },
        { label: '打开终端', run: () => { setDockOpen(true); setDockTab('terminal'); } },
        { sep: true, label: 's2' },
        { label: '外观与背景…', run: () => openAppearancePage() },
        { label: '放大字号', hint: 'Ctrl+=', run: () => bus.current.editor?.zoom(1) },
        { label: '缩小字号', hint: 'Ctrl+-', run: () => bus.current.editor?.zoom(-1) }
      ]
    },
    {
      label: '运行',
      items: [
        { label: '运行', hint: 'F5', run: () => void startRun('run') },
        { label: '调试', hint: 'F6', run: () => void startRun('debug') },
        { sep: true, label: 's1' },
        { label: '继续', hint: 'F8', disabled: ctl !== 'paused', run: doResume },
        { label: '单步跳过', hint: 'F10', disabled: ctl !== 'paused', run: () => doStep('over') },
        { label: '单步进入', hint: 'F11', disabled: ctl !== 'paused', run: () => doStep('into') },
        { label: '单步跳出', hint: 'Shift+F11', disabled: ctl !== 'paused', run: () => doStep('out') },
        { label: '切换断点', hint: 'F9', run: () => { bus.current.editor?.toggleBreakpoint(); pushLine({ kind: 'info', text: '已在当前行切换断点' }); } },
        { sep: true, label: 's2' },
        { label: '停止', hint: 'Shift+F5', disabled: ctl === 'idle' || ctl === 'done', run: doStop }
      ]
    },
    {
      label: '工具',
      items: [
        { label: '在文件中查找', hint: 'Ctrl+Shift+F', run: openSearch },
        { label: '快速打开文件', hint: 'Ctrl+P', run: () => setQuickOpen({ q: '', list: [], sel: 0 }) },
        { label: '跳转到定义', hint: 'F12', run: gotoDefinition },
        { label: '重命名符号', hint: 'F2', run: startRename },
        { sep: true, label: 't1' },
        ...pluginMenus.map((m) => ({
          label: m.label,
          run: () => { void pluginHostRef.current?.execute(m.command); }
        })),
        ...(pluginMenus.length ? [{ sep: true, label: 't2' }] : []),
        { label: '插件管理…', run: () => openPluginsPage() },
        { label: 'MCP 服务器…', run: () => openMcpPage() }
      ]
    },
    {
      label: '帮助',
      items: [
        { label: '学习中心（CW Script 教程）', hint: 'Ctrl+Shift+H', run: () => openTutorial() },
        { sep: true, label: 'h1' },
        { label: '快捷键一览', hint: 'F1', run: () => openShortcutsPage() },
        { label: '偏好设置…', hint: 'Ctrl+Alt+S', run: () => openSettingsPage() },
        { label: '关于 Float IDE', run: () => pushLine({ kind: 'info', text: 'Float IDE · 小方工作室 · CW Script' }) }
      ]
    }
  ];

  return (
    <div className="ws">
      <TitleBar
        title="Float IDE"
        subtitle={meta?.name ?? '未打开工程'}
        center={<span className="tb-file">{activeName || '未打开文件'}</span>}
        right={<button className="tb-link" title="偏好设置（Ctrl+Alt+S）" onClick={() => openSettingsPage()}><IconSettings size={13} /> 设置</button>}
      />

      <MenuBar menus={menus} />
      {lockWarn && (
        <div className="lock-warn">
          <span className="dot warn" />
          {lockWarn}
          <button className="cbtn ghost" onClick={() => setLockWarn(null)}>我知道了</button>
        </div>
      )}
      <div className="ws-tools">
        <div className="tg">
          <button className="tool run" onClick={() => void startRun('run')} title="运行（F5 · 编译执行）"><IconPlay size={15} /></button>
          <button className="tool debug" onClick={() => void startRun('debug')} title="调试（F6 · 解释执行，可断点单步）"><IconBug size={15} /></button>
        </div>
        <span className="tool-sep" />
        <div className="tg">
          <button className="tool" disabled={ctl !== 'paused'} onClick={doResume} title="继续（F8）"><IconPlay size={12} /></button>
          <button className="tool" disabled={ctl !== 'paused'} onClick={() => doStep('over')} title="单步跳过（F10）"><IconStepOver size={14} /></button>
          <button className="tool" disabled={ctl !== 'paused'} onClick={() => doStep('into')} title="单步进入（F11）"><IconStepInto size={14} /></button>
          <button className="tool" disabled={ctl !== 'paused'} onClick={() => doStep('out')} title="单步跳出（Shift+F11）"><IconStepOut size={14} /></button>
          <button className="tool danger" disabled={ctl === 'idle' || ctl === 'done'} onClick={doStop} title="停止（Shift+F5）"><IconStop size={12} /></button>
        </div>
        <span className="tool-sep" />
        <button className={`tool ${dockOpen && dockTab === 'terminal' ? 'on' : ''}`} onClick={() => { setDockOpen(true); setDockTab('terminal'); }} title="终端（Ctrl+`）"><IconTerminal size={15} /></button>
        <button className="tool" onClick={openSearch} title="在文件中查找（Ctrl+Shift+F）"><IconSearch size={15} /></button>
        <button className="tool book" onClick={() => openTutorial()} title="学习中心（Ctrl+Shift+H）"><IconBook size={15} /></button>
        <button className="tool" onClick={() => openPluginsPage()} title="插件管理">
          <IconPlug size={15} />
          {pluginCommands.length > 0 && <span className="tool-badge">{pluginTick > 0 ? pluginCommands.length : 0}</span>}
        </button>
        <button className="tool" onClick={() => openMcpPage()} title="MCP 服务器"><IconServer size={15} /></button>
        <button className="tool" onClick={() => openAppearancePage()} title="外观与背景"><IconImage size={15} /></button>
        <button className="tool" onClick={() => openSettingsPage()} title="偏好设置（Ctrl+Alt+S）"><IconSettings size={15} /></button>
        <button className="tool" onClick={() => void doExport()} title="导出工程包（.floatzip）"><IconExport size={14} /></button>
        <span className="tool-grow" />
        <span className={`tool-state ${ctl}`}>{stateText}</span>
        <button className="tool wide" onClick={() => { doStop(); onExit(); }} title="关闭工程，返回欢迎页">返回</button>
      </div>

      <div className="ws-main">
        <div className="ws-left">
          <div className="rail">
            <button className={`rail-btn ${sideTab === 'files' ? 'active' : ''}`} title="资源管理器" onClick={() => setSideTab('files')}>
              <IconFile size={17} />
            </button>
            <button className={`rail-btn ${sideTab === 'git' ? 'active' : ''}`} title="源代码管理（Git）" onClick={() => setSideTab('git')}>
              <IconBranch size={17} />
            </button>
            <div className="rail-grow" />
            <button className="rail-btn" title="外观与背景" onClick={() => openAppearancePage()}>
              <IconImage size={17} />
            </button>
          </div>
          <div className="side-wrap" style={{ width: layout.sideW }}>
            {sideTab === 'files'
              ? <FileTree projectDir={projectDir} activePath={activePath} bus={bus} onDirtyTree={() => {}} />
              : <GitPanel projectDir={projectDir} onOpenDiff={(rel, kind) => void openGitDiff(rel, kind)} />}
          </div>
          <Splitter dir="x" title="拖动调整侧栏宽度" onDrag={(d) => setPanel('sideW', Math.min(420, Math.max(150, layout.sideW + d)))} />
          <div className="ws-editor-col">
            <EditorPane
              projectDir={projectDir}
              bus={bus}
              onSaved={onSaved}
              onDirtyChange={setDirtyCount}
              breakpoints={breakpoints}
              execPos={execPos}
              onCursor={(line, col) => setCursor({ line, col })}
              onProblems={(items) => setProblems(items)}
              onActiveFile={(name) => {
                setActiveName(name);
                setActivePath(bus.current.editor?.currentPath() ?? null);
              }}
            />
          </div>
        </div>
        <Splitter dir="x" title="拖动调整大纲宽度" onDrag={(d) => setPanel('rightW', Math.min(520, Math.max(190, layout.rightW - d)))} />
        <SidePanels
          source={entrySrc}
          file={entryFile}
          onReveal={(line) => bus.current.editor?.revealLine(entryFile, line)}
          vars={vars}
          running={running}
          width={layout.rightW}
          tabRequest={tabRequest}
          problems={allProblems}
          onOpenProblem={(p) => bus.current.editor?.revealLine(p.file, p.line)}
        />
      </div>

      {dockOpen && (
        <Splitter dir="y" title="拖动调整控制台高度" onDrag={(d) => setPanel('dockH', Math.min(560, Math.max(120, layout.dockH - d)))} />
      )}
      {dockOpen && (
        <ConsoleDock
          lines={lines}
          askQuestion={askQ}
          onAnswer={(a) => {
            const r = runnerRef.current;
            setAskQ(null);
            pushLine({ kind: 'info', text: `> ${a}` });
            r?.resume(a);
          }}
          running={running}
          stateText={stateText}
          tab={dockTab}
          onTab={setDockTab}
          projectDir={projectDir}
          onClear={() => setLines([])}
          height={layout.dockH}
          searchNode={indexRef.current ? (
            <SearchPanel
              index={indexRef.current}
              bus={bus}
              focusSeq={searchFocus}
              onClose={() => setDockTab('console')}
            />
          ) : null}
        />
      )}

      <div className="ws-status">
        <span className="pip">
          <span className={`dot ${ctl === 'paused' ? 'idle' : ctl === 'idle' ? 'idle' : ''}`} />
          {stateText}
        </span>
        <span className="mode">{execMode}</span>
        <span className="grow" />
        {errorCount > 0
          ? <button className="status-link" onClick={() => setTabRequest((t) => ({ tab: 'problems', seq: t.seq + 1 }))}>
              <span className="dot warn" />{errorCount} 个错误
            </button>
          : <span className="pip"><span className="dot" />无错误</span>}
        {warnCount > 0 && (
          <button className="status-link" onClick={() => setTabRequest((t) => ({ tab: 'problems', seq: t.seq + 1 }))}>
            {warnCount} 个警告
          </button>
        )}
        <span className="sb-click" title="在文件中查找（Ctrl+Shift+F）" onClick={openSearch}>
          索引 {indexSize} 个文件
        </span>
        <span className="sb-click" title="插件管理" onClick={() => openPluginsPage()}>
          {pluginCommands.length > 0 ? `${pluginCommands.length} 个插件命令` : '无插件命令'}
        </span>
        <span className="sb-click" title="MCP 服务器" onClick={() => openMcpPage()}>
          MCP {mcpRunning ? `:${mcpPort}` : '未启动'}
        </span>
        <span>{entryFile.split(/[\\/]/).pop() || '未打开文件'}</span>
        <span>行 {cursor.line}，列 {cursor.col}</span>
        <span>UTF-8</span>
        <span>LF</span>
        <span>CW Script 0.10</span>
      </div>

      {rename && (
        <div className="modal-mask" onMouseDown={() => setRename(null)}>
          <div className="rename-box" onMouseDown={(e) => e.stopPropagation()}>
            <div className="rename-title">重命名符号 <b>{rename.name}</b></div>
            <input
              autoFocus
              className="rename-input"
              value={renameTo}
              onChange={(e) => setRenameTo(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') { e.preventDefault(); void applyRename(); }
                if (e.key === 'Escape') setRename(null);
              }}
            />
            <div className="rename-hint">
              将同时修改 <b>{rename.files}</b> 个文件中的 <b>{rename.count}</b> 处引用（字符串和注释不会被改到）。
            </div>
            <div className="rename-actions">
              <button className="btn" onClick={() => setRename(null)}>取消</button>
              <button className="btn primary" onClick={() => void applyRename()}>重命名</button>
            </div>
          </div>
        </div>
      )}

      {quickOpen && (
        <div className="modal-mask" onMouseDown={() => setQuickOpen(null)}>
          <div className="quickopen" onMouseDown={(e) => e.stopPropagation()}>
            <input
              autoFocus
              className="quickopen-input"
              placeholder="输入文件名快速打开（支持模糊匹配）"
              value={quickOpen.q}
              onChange={(e) => {
                const q = e.target.value;
                const list = indexRef.current?.filesMatching(q) ?? [];
                setQuickOpen({ q, list, sel: 0 });
              }}
              onKeyDown={(e) => {
                if (e.key === 'Escape') { setQuickOpen(null); return; }
                if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  setQuickOpen((o) => o ? { ...o, sel: Math.min(o.list.length - 1, o.sel + 1) } : o);
                  return;
                }
                if (e.key === 'ArrowUp') {
                  e.preventDefault();
                  setQuickOpen((o) => o ? { ...o, sel: Math.max(0, o.sel - 1) } : o);
                  return;
                }
                if (e.key === 'Enter') {
                  const pick = quickOpen.list[quickOpen.sel];
                  if (pick) { void bus.current.editor?.openFile(pick); setQuickOpen(null); }
                }
              }}
            />
            <div className="quickopen-list">
              {(quickOpen.q ? quickOpen.list : (indexRef.current?.filesMatching('', 12) ?? [])).map((f, i) => {
                const name = f.split('/').pop() ?? f;
                return (
                  <div
                    key={f}
                    className={`quickopen-item ${i === quickOpen.sel ? 'on' : ''}`}
                    onMouseEnter={() => setQuickOpen((o) => o ? { ...o, sel: i } : o)}
                    onClick={() => { void bus.current.editor?.openFile(f); setQuickOpen(null); }}
                  >
                    <span className="qo-name">{name}</span>
                    <span className="qo-dir">{f.slice(0, Math.max(0, f.length - name.length - 1))}</span>
                  </div>
                );
              })}
              {quickOpen.q && quickOpen.list.length === 0 && <div className="quickopen-empty">没有匹配的文件（索引里有 {indexSize} 个）</div>}
            </div>
          </div>
        </div>
      )}

      {/* 浮层只留给临时信息：命令面板、快速打开、重命名、输入框。
          教程 / 插件 / MCP / 设置 / 快捷键 / 外观 都是标签页，不再浮在这里。 */}
      {paletteOpen && <CommandPalette commands={commands} onClose={() => setPaletteOpen(false)} />}
    </div>
  );

  /* Git diff 胶水 */
  async function openGitDiff(rel: string, kind: 'worktree' | 'index'): Promise<void> {
    const root = projectDir.replace(/[\\/]+$/, '');
    const real = root + '\\' + rel.replace(/\//g, '\\');
    let base = '';
    let current = '';
    try {
      if (kind === 'worktree') {
        const idx = await api.gitRun(projectDir, ['show', ':' + rel]);
        base = idx.code === 0 ? idx.stdout : '';
        try { current = await api.fsRead(real); } catch { current = ''; }
      } else {
        const head = await api.gitRun(projectDir, ['show', 'HEAD:' + rel]);
        base = head.code === 0 ? head.stdout : '';
        const idx = await api.gitRun(projectDir, ['show', ':' + rel]);
        current = idx.code === 0 ? idx.stdout : '';
      }
    } catch { /* 空基线 */ }
    bus.current.editor?.openDiff({ title: `${rel}（${kind === 'index' ? '已暂存' : '工作区改动'}）`, base, current, lang: 'cwscript' });
  }
}
