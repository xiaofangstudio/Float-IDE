import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../core/tauri';
import { askConfirm, askText } from '../ui/prompt';
import { IconBranch, IconCloudDown, IconCloudUp, IconHistory, IconPlus, IconRefresh } from '../ui/icons';

/* 源代码管理：状态、暂存、提交、diff、分支、历史、push/pull、remote。
 * 底层是系统 git.exe（git_run），此处负责解析与编排。 */

type Props = {
  projectDir: string;
  onOpenDiff: (relPath: string, kind: 'worktree' | 'index') => void;
  onChanged?: () => void;
};

type Change = { rel: string; x: string; y: string; untracked: boolean };

type LogRow = { hash: string; author: string; date: string; subject: string };

const relReal = (projectDir: string, rel: string): string =>
  projectDir.replace(/[\\/]+$/, '') + '\\' + rel.replace(/\//g, '\\');

function parseStatus(stdout: string): { branch: string; remoteLine: string; staged: Change[]; unstaged: Change[]; untracked: Change[] } {
  const lines = stdout.replace(/\r/g, '').split('\n').filter(Boolean);
  let branch = '';
  let remoteLine = '';
  const staged: Change[] = [];
  const unstaged: Change[] = [];
  const untracked: Change[] = [];
  for (const line of lines) {
    if (line.startsWith('## ')) {
      const rest = line.slice(3);
      const dot = rest.indexOf('...');
      branch = dot >= 0 ? rest.slice(0, dot) : rest.split(' ')[0];
      remoteLine = line.slice(3);
      continue;
    }
    const x = line[0];
    const y = line[1];
    let rel = line.slice(3);
    if (rel.includes(' -> ')) rel = rel.split(' -> ')[1].replace(/^\x22|\x22$/g, '');
    if (x === '?' && y === '?') untracked.push({ rel, x, y, untracked: true });
    else if (x === '!' || y === '!') { /* 忽略 */ }
    else {
      if (x !== ' ' && x !== '?') staged.push({ rel, x, y, untracked: false });
      if (y !== ' ' && y !== '?') unstaged.push({ rel, x, y, untracked: false });
      else if ((x === 'M' || x === 'A' || x === 'D') && y === ' ') { /* 已全部暂存 */ }
    }
  }
  return { branch, remoteLine, staged, unstaged, untracked };
}

const statusLabel = (c: Change): string => {
  const m: Record<string, string> = { M: '修改', A: '新增', D: '删除', R: '重命名', C: '复制', U: '冲突', T: '类型变更' };
  return m[c.untracked ? 'A' : (c.y !== ' ' ? c.y : c.x)] ?? '变更';
};

export function GitPanel({ projectDir, onOpenDiff, onChanged }: Props) {
  const [available, setAvailable] = useState<boolean | null>(null);
  const [isRepo, setIsRepo] = useState<boolean | null>(null);
  const [branch, setBranch] = useState('');
  const [remoteLine, setRemoteLine] = useState('');
  const [staged, setStaged] = useState<Change[]>([]);
  const [unstaged, setUnstaged] = useState<Change[]>([]);
  const [untracked, setUntracked] = useState<Change[]>([]);
  const [msg, setMsg] = useState('');
  const [commitMsg, setCommitMsg] = useState('');
  const [section, setSection] = useState<'none' | 'history' | 'branches' | 'remotes'>('none');
  const [logs, setLogs] = useState<LogRow[]>([]);
  const [branches, setBranches] = useState<string[]>([]);
  const [remotes, setRemotes] = useState<string[]>([]);
  const [statText, setStatText] = useState<string | null>(null);
  const busyRef = useRef(false);

  const git = useCallback(async (args: string[]): Promise<{ code: number; stdout: string; stderr: string }> => {
    return api.gitRun(projectDir, args);
  }, [projectDir]);

  const refresh = useCallback(async (): Promise<void> => {
    if (available === null) setAvailable(await api.gitAvailable().catch(() => false));
    const probe = await git(['rev-parse', '--is-inside-work-tree']);
    const inRepo = probe.code === 0 && probe.stdout.trim() === 'true';
    setIsRepo(inRepo);
    if (!inRepo) return;
    const st = await git(['status', '--porcelain=v1', '-b', '-uall']);
    const p = parseStatus(st.stdout);
    setBranch(p.branch);
    setRemoteLine(p.remoteLine);
    setStaged(p.staged);
    setUnstaged(p.unstaged);
    setUntracked(p.untracked);
  }, [git, available]);

  useEffect(() => { void refresh(); }, [refresh]);

  const run = useCallback(async (args: string[], okText: string): Promise<boolean> => {
    if (busyRef.current) return false;
    busyRef.current = true;
    try {
      const r = await git(args);
      if (r.code === 0) {
        setMsg(okText);
        await refresh();
        onChanged?.();
        return true;
      }
      setMsg((r.stderr || r.stdout || '执行失败').trim());
      return false;
    } finally {
      busyRef.current = false;
      window.setTimeout(() => setMsg((m) => (busyRef.current ? m : '')), 4000);
    }
  }, [git, refresh, onChanged]);

  const openChange = async (c: Change, kind: 'worktree' | 'index'): Promise<void> => {
    await refresh();
    onOpenDiff(c.rel, kind);
  };

  const loadHistory = async (): Promise<void> => {
    const r = await git(['log', '-n', '40', '--pretty=format:%h%x09%an%x09%ad%x09%s', '--date=short']);
    if (r.code !== 0) { setLogs([]); setMsg(r.stderr.trim()); return; }
    setLogs(r.stdout.replace(/\r/g, '').split('\n').filter(Boolean).map((line) => {
      const [hash, author, date, ...rest] = line.split('\t');
      return { hash, author, date, subject: rest.join('\t') };
    }));
  };

  const loadBranches = async (): Promise<void> => {
    const r = await git(['branch', '--format=%(refname:short)']);
    setBranches(r.stdout.replace(/\r/g, '').split('\n').map((s) => s.trim()).filter(Boolean));
  };

  const loadRemotes = async (): Promise<void> => {
    const r = await git(['remote', '-v']);
    setRemotes(r.stdout.replace(/\r/g, '').split('\n').filter(Boolean));
  };

  const toggleSection = async (s: 'history' | 'branches' | 'remotes'): Promise<void> => {
    const next = section === s ? 'none' : s;
    setSection(next);
    if (next === 'history') await loadHistory();
    if (next === 'branches') await loadBranches();
    if (next === 'remotes') await loadRemotes();
  };

  const initRepo = async (): Promise<void> => {
    if (await run(['init'], '已初始化 Git 仓库')) {
      // 给新仓库一个默认分支名，避免 master/main 歧义提示
      await git(['symbolic-ref', 'HEAD', 'refs/heads/main']);
      await refresh();
    }
  };

  const commit = async (): Promise<void> => {
    if (!commitMsg.trim()) { setMsg('请先填写提交说明'); return; }
    if (await run(['commit', '-m', commitMsg.trim()], '提交完成')) setCommitMsg('');
  };

  const commitAll = async (): Promise<void> => {
    if (!commitMsg.trim()) { setMsg('请先填写提交说明'); return; }
    if (await run(['add', '-A'], '已暂存全部更改')) {
      if (await run(['commit', '-m', commitMsg.trim()], '提交完成')) setCommitMsg('');
    }
  };

  const push = async (): Promise<void> => {
    if (await run(['push'], '推送完成')) return;
    // 无上游时尝试 -u origin <branch>
    if (branch) {
      setMsg('尝试建立上游跟踪分支……');
      await run(['push', '-u', 'origin', branch], '推送完成（已建立上游跟踪）');
    }
  };

  const addRemote = async (): Promise<void> => {
    const url = await askText('添加远程仓库', { placeholder: 'https://github.com/用户名/仓库.git' });
    if (!url) return;
    await run(['remote', 'add', 'origin', url], '远程 origin 已添加');
    await loadRemotes();
  };

  const newBranch = async (): Promise<void> => {
    const name = await askText('新建分支', { placeholder: '分支名称，如 feature/ui' });
    if (!name) return;
    if (await run(['checkout', '-b', name.trim()], `已创建并切换到 ${name.trim()}`)) await loadBranches();
  };

  const showStat = async (hash: string): Promise<void> => {
    const r = await git(['show', hash, '--stat', '--format=提交：' + hash + '%n作者：%an%n日期：%ad%n%n    %s%n', '--date=format:%Y-%m-%d %H:%M']);
    setStatText(r.stdout || r.stderr);
  };

  const row = (c: Change, kind: 'worktree' | 'index', actions: React.ReactNode): React.ReactNode => (
    <div className="git-row" key={c.rel + kind} onClick={() => void openChange(c, kind)} title={c.rel}>
      <span className="git-badge">{statusLabel(c)}</span>
      <span className="git-path">{c.rel.split('/').pop()}</span>
      <span className="git-actions" onClick={(e) => e.stopPropagation()}>{actions}</span>
    </div>
  );

  if (available === null) return <div className="explorer"><div className="explorer-body muted">检测 Git……</div></div>;

  if (!available) {
    return (
      <div className="explorer">
        <div className="explorer-head"><span>源代码管理</span></div>
        <div className="explorer-body muted">
          未检测到 Git。请安装 Git for Windows 并确保 git 在 PATH 中，然后重启 Float IDE。
        </div>
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className="explorer">
        <div className="explorer-head"><span>源代码管理</span></div>
        <div className="explorer-body">
          <p className="muted" style={{ lineHeight: 1.8 }}>此工程还不是 Git 仓库。初始化后即可进行暂存、提交、分支与推送。</p>
          <button className="btn primary" style={{ width: '100%' }} onClick={() => void initRepo()}>初始化仓库</button>
          {msg && <div className="git-msg">{msg}</div>}
        </div>
      </div>
    );
  }

  const total = staged.length + unstaged.length + untracked.length;

  return (
    <div className="explorer">
      <div className="explorer-head">
        <span>源代码管理</span>
        <span style={{ flex: 1 }} />
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="刷新状态" onClick={() => void refresh()}><IconRefresh size={12} /></button>
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="推送" onClick={() => void push()}><IconCloudUp size={13} /></button>
        <button className="icon-btn" style={{ width: 22, height: 22 }} title="拉取" onClick={() => void run(['pull'], '拉取完成')}><IconCloudDown size={13} /></button>
      </div>

      <div className="git-branch" onClick={() => void toggleSection('branches')}>
        <IconBranch size={13} />
        <span>{branch || '（无分支）'}</span>
        <span className="git-ahead">{remoteLine.includes('ahead') ? '↑' + (remoteLine.match(/ahead (\d+)/)?.[1] ?? '') : ''}</span>
        <span style={{ flex: 1 }} />
        <span className="muted" style={{ fontSize: 11 }}>切换</span>
      </div>

      <div className="git-commit">
        <textarea
          className="git-msg-input"
          placeholder="提交说明（Ctrl+Enter 提交全部）"
          rows={2}
          value={commitMsg}
          onChange={(e) => setCommitMsg(e.target.value)}
          onKeyDown={(e) => { if (e.ctrlKey && e.key === 'Enter') void commitAll(); }}
        />
        <div style={{ display: 'flex', gap: 6 }}>
          <button className="btn" style={{ flex: 1, fontSize: 12, padding: '6px 8px' }} disabled={staged.length === 0} onClick={() => void commit()}>提交已暂存</button>
          <button className="btn primary" style={{ flex: 1, fontSize: 12, padding: '6px 8px' }} disabled={total === 0} onClick={() => void commitAll()}>全部提交</button>
        </div>
      </div>

      <div className="explorer-body">
        {total === 0 && <div className="muted" style={{ padding: '8px 4px' }}>工作区干净，没有待提交的更改。</div>}

        {staged.length > 0 && (
          <>
            <div className="git-sec">暂存的更改（{staged.length}）</div>
            {staged.map((c) => row(c, 'index', (
              <>
                <button className="icon-btn" style={{ width: 20, height: 20 }} title="取消暂存"
                  onClick={() => void run(['restore', '--staged', '--', c.rel], '已取消暂存')}>−</button>
              </>
            )))}
          </>
        )}

        {unstaged.length > 0 && (
          <>
            <div className="git-sec">更改（{unstaged.length}）</div>
            {unstaged.map((c) => row(c, 'worktree', (
              <>
                <button className="icon-btn" style={{ width: 20, height: 20 }} title="暂存"
                  onClick={() => void run(['add', '--', c.rel], '已暂存')}>+</button>
                <button className="icon-btn" style={{ width: 20, height: 20 }} title="放弃更改"
                  onClick={() => { void (async () => { if (await askConfirm(`放弃「${c.rel}」的未暂存更改？`, { hint: '改动会直接丢弃，无法恢复。', okText: '放弃更改', danger: true })) void run(['restore', '--', c.rel], '已放弃更改'); })(); }}>↺</button>
              </>
            )))}
          </>
        )}

        {untracked.length > 0 && (
          <>
            <div className="git-sec">未跟踪（{untracked.length}）</div>
            {untracked.map((c) => row(c, 'worktree', (
              <button className="icon-btn" style={{ width: 20, height: 20 }} title="暂存"
                onClick={() => void run(['add', '--', c.rel], '已暂存')}><IconPlus size={11} /></button>
            )))}
          </>
        )}

        <div className="git-sec clickable" onClick={() => void toggleSection('history')}>
          <IconHistory size={12} /> 提交历史
        </div>
        {section === 'history' && (
          <div className="git-list">
            {logs.length === 0 && <div className="muted" style={{ padding: '4px' }}>还没有提交</div>}
            {logs.map((l) => (
              <div className="git-log-row" key={l.hash} onClick={() => void showStat(l.hash)} title="点击查看改动文件">
                <span className="git-hash">{l.hash}</span>
                <span className="git-subject">{l.subject}</span>
                <span className="git-meta">{l.author} · {l.date}</span>
              </div>
            ))}
          </div>
        )}

        <div className="git-sec clickable" onClick={() => void toggleSection('branches')}>
          <IconBranch size={12} /> 分支
        </div>
        {section === 'branches' && (
          <div className="git-list">
            {branches.map((b) => (
              <div className="git-row" key={b}>
                <span className="git-badge">{b === branch ? '当前' : ''}</span>
                <span className="git-path">{b}</span>
                <span className="git-actions">
                  {b !== branch && (
                    <>
                      <button className="icon-btn" style={{ width: 20, height: 20 }} title="切换到此分支"
                        onClick={() => void run(['checkout', b], `已切换到 ${b}`)}>切</button>
                      <button className="icon-btn" style={{ width: 20, height: 20 }} title="删除分支"
                        onClick={() => { void (async () => { if (await askConfirm(`删除分支 ${b}？`, { okText: '删除', danger: true })) void run(['branch', '-d', b], '分支已删除'); })(); }}>×</button>
                    </>
                  )}
                </span>
              </div>
            ))}
            <button className="btn ghost" style={{ fontSize: 12, width: '100%' }} onClick={() => void newBranch()}>+ 新建分支</button>
          </div>
        )}

        <div className="git-sec clickable" onClick={() => void toggleSection('remotes')}>
          远程仓库
        </div>
        {section === 'remotes' && (
          <div className="git-list">
            {remotes.length === 0 && <div className="muted" style={{ padding: '4px' }}>还没有配置远程</div>}
            {remotes.map((r) => <div className="git-log-row" key={r}><span className="git-subject">{r}</span></div>)}
            <button className="btn ghost" style={{ fontSize: 12, width: '100%' }} onClick={() => void addRemote()}>+ 添加远程 origin</button>
          </div>
        )}

        {msg && <div className="git-msg">{msg}</div>}
      </div>

      {statText !== null && (
        <div className="modal-mask" onClick={() => setStatText(null)}>
          <div className="modal" style={{ width: 520 }} onClick={(e) => e.stopPropagation()}>
            <h3>提交详情</h3>
            <pre className="git-stat">{statText}</pre>
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <button className="btn" onClick={() => setStatText(null)}>关闭</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
