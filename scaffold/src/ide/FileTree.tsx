import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../core/tauri';
import { askConfirm, askText } from '../ui/prompt';
import { IconFile, IconFolder, IconPlus, IconTrash, IconRefresh } from '../ui/icons';
import type { FileEntry } from '../core/types';

/* 资源管理器：工程文件树（懒加载展开）、右键菜单、新建/重命名/删除。 */

type TreeNode = { name: string; path: string; isDir: boolean };

type Props = {
  projectDir: string;
  /** 当前编辑器打开的文件，用来在树里高亮 */
  activePath?: string | null;
  bus: React.MutableRefObject<{ editor?: { openFile: (p: string) => Promise<void>; closeTab: (p: string) => void } }>;
  onDirtyTree?: () => void;
};

type Menu = { x: number; y: number; node: TreeNode | null } | null;

/** 文件类型 → 图标颜色（只用来快速扫视，不求花哨） */
function fileKind(name: string): string {
  const n = name.toLowerCase();
  if (n.endsWith('.cw')) return 'cw';
  if (n.endsWith('.json')) return 'json';
  if (n.endsWith('.md')) return 'md';
  return 'other';
}

export function FileTree({ projectDir, activePath, bus, onDirtyTree }: Props) {
  const [children, setChildren] = useState<Map<string, TreeNode[]>>(new Map());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState<Menu>(null);
  const [err, setErr] = useState('');
  const refreshRef = useRef<() => void>(() => {});

  const sortNodes = (list: FileEntry[], base: string): TreeNode[] =>
    list
      .map((e) => ({ name: e.name, path: base + '/' + e.name, isDir: e.isDir }))
      .sort((a, b) => (a.isDir === b.isDir ? a.name.localeCompare(b.name, 'zh') : a.isDir ? -1 : 1));

  const loadDir = useCallback(async (dir: string): Promise<TreeNode[]> => {
    try {
      const entries = await api.fsList(dir);
      return sortNodes(entries, dir);
    } catch (e) {
      setErr(String(e));
      return [];
    }
  }, []);

  const refresh = useCallback((): void => {
    void (async () => {
      const rootNodes = await loadDir(projectDir);
      setChildren((m) => {
        const n = new Map(m);
        n.set(projectDir, rootNodes);
        return n;
      });
      onDirtyTree?.();
    })();
  }, [projectDir, loadDir, onDirtyTree]);

  refreshRef.current = refresh;
  useEffect(() => { refresh(); }, [refresh]);

  const toggleDir = async (node: TreeNode): Promise<void> => {
    const wasOpen = expanded.has(node.path);
    setExpanded((s) => {
      const n = new Set(s);
      if (wasOpen) n.delete(node.path); else n.add(node.path);
      return n;
    });
    if (!wasOpen && !children.has(node.path)) {
      const list = await loadDir(node.path);
      setChildren((m) => {
        const n = new Map(m);
        n.set(node.path, list);
        return n;
      });
    }
  };

  const newFileIn = async (dir: string, isDir: boolean): Promise<void> => {
    const name = await askText(isDir ? '新建文件夹' : '新建文件', {
      placeholder: isDir ? '文件夹名称' : '文件名，如 tools.cw',
      hint: isDir ? '' : '不写后缀会自动补 .cw',
      validate: (v) => (/[\\/:*?"<>|]/.test(v) ? '名称里不能有 \\ / : * ? " < > |' : null)
    });
    if (!name) return;
    // 提示里说了「不写后缀会自动补 .cw」，就得真的补
    const finalName = isDir || /\.[A-Za-z0-9]{1,8}$/.test(name) ? name : name + '.cw';
    const p = dir + '/' + finalName;
    try {
      if (isDir) await api.fsMkdir(p);
      else {
        await api.fsWrite(
          p,
          /\.cw$/i.test(finalName)
            ? '// 新建的 Float Script 文件\n\nprint("你好，Float Script")\n'
            : ''
        );
      }
      if (dir !== projectDir && !expanded.has(dir)) {
        setExpanded((s) => new Set(s).add(dir));
        const list = await loadDir(dir);
        setChildren((m) => { const n = new Map(m); n.set(dir, list); return n; });
      }
      refresh();
      if (!isDir) void bus.current.editor?.openFile(p);
    } catch (e) { setErr('创建失败：' + String(e)); }
  };

  const renameEntry = async (node: TreeNode): Promise<void> => {
    const name = await askText(`重命名「${node.name}」`, {
      value: node.name,
      validate: (v) => (/[\\/:*?"<>|]/.test(v) ? '名称里不能有 \\ / : * ? " < > |' : null)
    });
    if (!name || name === node.name) return;
    const to = node.path.slice(0, node.path.length - node.name.length) + name;
    try {
      await api.fsRename(node.path, to);
      bus.current.editor?.closeTab(node.path);
      refresh();
    } catch (e) { setErr('重命名失败：' + String(e)); }
  };

  const deleteEntry = async (node: TreeNode): Promise<void> => {
    const yes = await askConfirm(`删除「${node.name}」？`, {
      hint: node.isDir ? '目录会连同里面的所有内容一起删除，无法恢复。' : '删除后无法恢复。',
      okText: '删除', danger: true
    });
    if (!yes) return;
    try {
      await api.fsDelete(node.path);
      bus.current.editor?.closeTab(node.path);
      refresh();
    } catch (e) { setErr('删除失败：' + String(e)); }
  };

  const renderNode = (node: TreeNode, depth: number): React.ReactNode => {
    const isOpen = expanded.has(node.path);
    const isActive = !node.isDir && activePath
      && node.path.replace(/\\/g, '/') === activePath.replace(/\\/g, '/');
    return (
      <React.Fragment key={node.path}>
        <div
          className={`tree-item ${depth === 0 && node.isDir ? 'root' : ''} ${isActive ? 'active' : ''}`}
          style={{ paddingLeft: 4 + depth * 14 }}
          onClick={() => (node.isDir ? void toggleDir(node) : void bus.current.editor?.openFile(node.path))}
          onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, node }); }}
          title={node.path}
        >
          {/* 缩进引导线：每一层一条竖线，层级一眼可辨 */}
          {Array.from({ length: depth }, (_, k) => (
            <span key={k} className="ti-guide" />
          ))}
          {/* 展开三角：目录才有，文件位置留白对齐 */}
          <span className={`ti-caret ${node.isDir ? '' : 'blank'} ${isOpen ? 'open' : ''}`}>
            {node.isDir && (
              <svg width="9" height="9" viewBox="0 0 9 9">
                <path d="M3 1.8L6.2 4.5 3 7.2" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            )}
          </span>
          {node.isDir
            ? <span className={`fi dir ${isOpen ? 'open' : ''}`}><IconFolder size={13} /></span>
            : <span className={`fi ${fileKind(node.name)}`}><IconFile size={13} /></span>}
          <span className="ti-name">{node.name}</span>
        </div>
        {node.isDir && isOpen && (children.get(node.path) ?? []).map((c) => renderNode(c, depth + 1))}
      </React.Fragment>
    );
  };

  const rootNodes = children.get(projectDir) ?? [];

  return (
    <div className="explorer" onClick={() => menu && setMenu(null)}>
      <div className="explorer-head">
        <span>资源管理器</span>
        <span style={{ flex: 1 }} />
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="刷新" onClick={() => refreshRef.current()}>
          <IconRefresh size={12} />
        </button>
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="新建文件" onClick={() => void newFileIn(projectDir, false)}>
          <IconPlus size={12} />
        </button>
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="新建文件夹" onClick={() => void newFileIn(projectDir, true)}>
          <IconFolder size={12} />
        </button>
      </div>
      <div className="explorer-body">
        {rootNodes.map((n) => renderNode(n, 0))}
      </div>
      {err && <div className="compile-status err">{err}</div>}
      {menu?.node && (
        <div className="ctx-menu" style={{ left: menu.x, top: menu.y }} onClick={() => setMenu(null)}>
          {!menu.node.isDir && (
            <div className="ctx-item" onClick={() => void bus.current.editor?.openFile(menu.node!.path)}>打开</div>
          )}
          {menu.node.isDir && (
            <>
              <div className="ctx-item" onClick={() => void newFileIn(menu.node!.path, false)}>新建文件</div>
              <div className="ctx-item" onClick={() => void newFileIn(menu.node!.path, true)}>新建文件夹</div>
            </>
          )}
          <div className="ctx-item" onClick={() => void renameEntry(menu.node!)}>重命名</div>
          <div className="ctx-item danger" onClick={() => void deleteEntry(menu.node!)}>删除</div>
        </div>
      )}
    </div>
  );
}
