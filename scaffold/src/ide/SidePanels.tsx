import React, { useEffect, useMemo, useState } from 'react';
import { extractOutline } from '../lang/floatlang';
import type { ProblemItem, VarRow } from '../core/types';

/* 右侧面板：大纲（组标记 / 函数 / 顶层变量）+ 变量查看器（调试快照）。 */

type Props = {
  source: string;
  file: string;
  onReveal: (line: number) => void;
  vars: VarRow[];
  running: boolean;
  width?: number;
  /** 外部请求切换标签（Alt+3 / 菜单 / 状态栏） */
  tabRequest?: { tab: 'outline' | 'vars' | 'problems'; seq: number };
  /** 问题面板数据 */
  problems?: ProblemItem[];
  onOpenProblem?: (p: ProblemItem) => void;
};

export function SidePanels({ source, file, onReveal, vars, running, width, tabRequest, problems, onOpenProblem }: Props) {
  const [tab, setTab] = useState<'outline' | 'vars' | 'problems'>('outline');
  const outline = useMemo(() => extractOutline(source), [source]);

  useEffect(() => {
    if (tabRequest) setTab(tabRequest.tab);
  }, [tabRequest?.seq]);

  return (
    <div className="side-panels" style={width ? { width } : undefined}>
      <div className="sp-tabs">
        <div className={`sp-tab ${tab === 'outline' ? 'active' : ''}`} onClick={() => setTab('outline')}>大纲</div>
        <div className={`sp-tab ${tab === 'vars' ? 'active' : ''}`} onClick={() => setTab('vars')}>变量</div>
        <div className={`sp-tab ${tab === 'problems' ? 'active' : ''}`} onClick={() => setTab('problems')}>
          问题{(problems?.length ?? 0) > 0 ? ` ${problems?.length}` : ''}
        </div>
      </div>
      <div className="sp-body">
        {tab === 'outline' ? (
          <>
            <div className="muted" style={{ fontSize: 11, marginBottom: 6 }}>{file.split(/[\\/]/).pop()}</div>
            {outline.length === 0 && <div className="muted">暂无可导航内容（组标记、函数、顶层变量会出现在这里）</div>}
            {outline.map((it, i) => {
              const styleClass = it.icon === 'class' ? 'cls'
                : it.icon === 'method' ? 'fn'
                : it.icon === 'field' ? 'vr'
                : it.icon;
              return (
                <div
                  key={i}
                  className={`outline-item ${styleClass}`}
                  style={{ paddingLeft: 8 + it.depth * 13 }}
                  onClick={() => onReveal(it.line)}
                  title={`跳到第 ${it.line} 行`}
                >
                  {it.icon === 'group' && <span className="outline-mark">组</span>}
                  {it.icon === 'class' && <span className="outline-mark fn">cls</span>}
                  {(it.icon === 'func' || it.icon === 'method') && <span className="outline-mark fn">fn</span>}
                  {it.icon === 'field' && <span className="outline-mark vr">·</span>}
                  {it.icon === 'var' && <span className="outline-mark vr">var</span>}
                  {it.icon === 'section' && <span className="outline-mark sec">//</span>}
                  <span>{it.text}</span>
                </div>
              );
            })}
          </>
        ) : tab === 'problems' ? (
          <>
            {(problems?.length ?? 0) === 0 && <div className="muted">没有发现问题。语法错误、未定义变量、运行时报错都会出现在这里。</div>}
            {problems?.map((p, i) => (
              <div key={`${p.file}:${p.line}:${i}`} className={`prob-item ${p.severity}`} onClick={() => onOpenProblem?.(p)}>
                <span className="prob-dot" />
                <span className="prob-main">
                  <b>{p.message}</b>
                  <i>{p.file.split(/[\\/]/).pop()} · 第 {p.line} 行{p.from === 'run' ? ' · 运行' : ''}</i>
                </span>
              </div>
            ))}
          </>
        ) : (
          <>
            {!running && vars.length === 0 && (
              <div className="muted">运行或调试时，变量与特殊变量（__last_* / ans）会实时出现在这里。</div>
            )}
            {vars.length === 0 && running && <div className="muted">还没有变量。</div>}
            {vars.map((v) => (
              <div className="kv" key={v.name}>
                <span><code className="inline">{v.name}</code></span>
                <span style={{ textAlign: 'right' }}>
                  <b className="mono" style={{ fontSize: 11.5 }}>{v.type}</b>
                  <span className="mono" style={{ marginLeft: 8, userSelect: 'text' }}>{v.value}</span>
                </span>
              </div>
            ))}
          </>
        )}
      </div>
    </div>
  );
}
