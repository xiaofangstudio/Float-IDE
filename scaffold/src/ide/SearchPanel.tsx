import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Hit, ProjectIndex, SearchOptions } from './indexer';
import type { EditorHandle } from './EditorPane';

/* 「在文件中查找」工具窗口（PyCharm 里的 Find in Files）。
 * 先预览再替换，绝不一键改全工程——改错了找不回来。 */

type Props = {
  index: ProjectIndex;
  bus: React.MutableRefObject<{ editor?: EditorHandle }>;
  /** 面板被打开/聚焦时触发次数变化 */
  focusSeq: number;
  onClose?: () => void;
};

type Group = { file: string; hits: Hit[] };

const DEFAULT_OPT: SearchOptions = {
  query: '', matchCase: false, wholeWord: false, useRegex: false, sourceOnly: true, limit: 2000
};

export function SearchPanel({ index, bus, focusSeq, onClose }: Props) {
  const [opt, setOpt] = useState<SearchOptions>(DEFAULT_OPT);
  const [input, setInput] = useState('');
  const [replacement, setReplacement] = useState('');
  const [showReplace, setShowReplace] = useState(false);
  const [ran, setRan] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [groups, setGroups] = useState<Group[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [done, setDone] = useState<{ files: number; hits: number } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { setInput(opt.query); }, [opt.query]);

  useEffect(() => {
    if (focusSeq > 0) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusSeq]);

  const doSearch = useCallback((next?: Partial<SearchOptions>): void => {
    const o = { ...opt, ...next, query: next?.query ?? input };
    setOpt(o);
    if (!o.query.trim()) { setGroups([]); setRan(false); setDone(null); return; }
    const t0 = performance.now();
    const res = index.search(o);
    setElapsed(Math.round(performance.now() - t0));
    setTruncated(res.truncated);
    const map = new Map<string, Hit[]>();
    for (const h of res.hits) {
      const arr = map.get(h.file) ?? [];
      arr.push(h);
      map.set(h.file, arr);
    }
    setGroups([...map.entries()].map(([file, hits]) => ({ file, hits })));
    setDone({ files: map.size, hits: res.hits.length });
    setRan(true);
    setCollapsed(new Set());
  }, [opt, input, index]);

  /* 输入停顿自动搜，像 PyCharm 一样 */
  useEffect(() => {
    if (!input) { setGroups([]); setRan(false); setDone(null); return; }
    const t = window.setTimeout(() => doSearch({ query: input }), 260);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [input]);

  const toggle = useCallback((k: 'matchCase' | 'wholeWord' | 'useRegex' | 'sourceOnly'): void => {
    const next = { ...opt, [k]: !opt[k] };
    setOpt(next);
    if (input.trim()) doSearch({ [k]: next[k] } as Partial<SearchOptions>);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opt, input, doSearch]);

  const parts = useMemo(() => {
    // 命中片段切分，用于 <mark> 高亮
    return (h: Hit): { plain: boolean; text: string }[] => {
      const out: { plain: boolean; text: string }[] = [];
      if (h.from > 0) out.push({ plain: true, text: h.text.slice(0, h.from) });
      out.push({ plain: false, text: h.text.slice(h.from, h.to) });
      if (h.to < h.text.length) out.push({ plain: true, text: h.text.slice(h.to) });
      return out;
    };
  }, []);

  const applyReplace = useCallback(async (all: boolean): Promise<void> => {
    if (!input.trim()) return;
    const changes = index.replace({ ...opt, query: input }, replacement);
    if (!changes.length) return;
    const targets = all ? changes : changes.slice(0, 1);
    for (const c of targets) {
      const ed = bus.current.editor;
      // 先让编辑器打开并写入新内容（这样 Monaco 也在同一份内容上）
      if (ed) {
        await ed.openFile(c.file);
        ed.setContent(c.file, c.text);
        await ed.saveFile(c.file);
      }
      index.put(c.file, c.text);
    }
    setDone({ files: targets.length, hits: targets.reduce((n, c) => n + c.count, 0) });
    doSearch({ query: input });
  }, [index, opt, input, replacement, bus, doSearch]);

  const replaceSummary = index.replace({ ...opt, query: input }, replacement);
  const pendingCount = replaceSummary.reduce((n, c) => n + c.count, 0);
  const pendingFiles = replaceSummary.length;

  return (
    <div className="search-panel">
      <div className="sp-head">
        <input
          ref={inputRef}
          className="sp-input"
          placeholder="在工程中查找…（Ctrl+Shift+F）"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') { e.preventDefault(); doSearch({ query: input }); }
            if (e.key === 'Escape') onClose?.();
          }}
        />
        <button className={`sp-opt ${opt.matchCase ? 'on' : ''}`} title="区分大小写" onClick={() => toggle('matchCase')}>Aa</button>
        <button className={`sp-opt ${opt.wholeWord ? 'on' : ''}`} title="全词匹配" onClick={() => toggle('wholeWord')}>W</button>
        <button className={`sp-opt ${opt.useRegex ? 'on' : ''}`} title="正则表达式" onClick={() => toggle('useRegex')}>.*</button>
        <button className={`sp-opt ${opt.sourceOnly ? 'on' : ''}`} title="只在 .cw 源码里搜" onClick={() => toggle('sourceOnly')}>.cw</button>
        <span className="tool-sep" />
        <button className={`sp-opt wide ${showReplace ? 'on' : ''}`} title="显示替换框" onClick={() => setShowReplace((v) => !v)}>替换</button>
      </div>

      {showReplace && (
        <div className="sp-head">
          <input
            className="sp-input"
            placeholder="替换为…"
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
          />
          <span className="sp-count">{pendingFiles} 个文件 / {pendingCount} 处将被修改</span>
          <span className="tool-grow" />
          <button className="sp-btn" disabled={!pendingCount} onClick={() => void applyReplace(false)}>替换首个文件</button>
          <button className="sp-btn primary" disabled={!pendingCount} onClick={() => void applyReplace(true)}>全部替换</button>
        </div>
      )}

      <div className="sp-meta">
        {ran
          ? <>找到 <b>{done?.hits ?? 0}</b> 处，分布在 <b>{done?.files ?? 0}</b> 个文件 · {elapsed}ms{truncated && ' · 结果过多已截断'}</>
          : input ? '搜索中…' : <>已在索引里收录 <b>{index.size}</b> 个文件</>}
        <span className="tool-grow" />
        <span className="sp-tip">点击结果跳转到对应行 · Ctrl+Shift+F 回到这里</span>
      </div>

      <div className="sp-results">
        {groups.map((g) => {
          const open = !collapsed.has(g.file);
          const name = g.file.split('/').pop() ?? g.file;
          const dir = g.file.slice(0, Math.max(0, g.file.length - name.length - 1));
          return (
            <div key={g.file} className="sp-group">
              <div className="sp-file" onClick={() => setCollapsed((s) => {
                const n = new Set(s);
                if (n.has(g.file)) n.delete(g.file); else n.add(g.file);
                return n;
              })}>
                <span className={`sp-caret ${open ? 'open' : ''}`}>▸</span>
                <span className="sp-name">{name}</span>
                <span className="sp-dir">{dir}</span>
                <span className="sp-badge">{g.hits.length}</span>
              </div>
              {open && g.hits.map((h, i) => (
                <div
                  key={i}
                  className="sp-hit"
                  onDoubleClick={() => bus.current.editor?.revealLine(h.file, h.line)}
                  onClick={() => bus.current.editor?.revealLine(h.file, h.line)}
                  title="点击跳转"
                >
                  <span className="sp-ln">{h.line}</span>
                  <span className="sp-text">
                    {parts(h).map((p, k) => p.plain
                      ? <span key={k}>{p.text}</span>
                      : <mark key={k}>{p.text}</mark>)}
                  </span>
                </div>
              ))}
            </div>
          );
        })}
        {ran && groups.length === 0 && <div className="sp-empty">没有找到匹配的内容</div>}
      </div>
    </div>
  );
}
