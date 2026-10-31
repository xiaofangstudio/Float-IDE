import React, { useEffect, useMemo, useState } from 'react';
import { extractOutline } from '../lang/floatlang';
import type { ProblemItem, ReportRow, VarRow } from '../core/types';

/* 右侧面板：大纲（组标记 / 函数 / 顶层变量）+ 变量查看器（调试快照）。 */

type Props = {
  source: string;
  file: string;
  onReveal: (line: number) => void;
  vars: VarRow[];
  running: boolean;
  width?: number;
  /** 外部请求切换标签（Alt+3 / 菜单 / 状态栏） */
  tabRequest?: { tab: 'outline' | 'vars' | 'problems' | 'report'; seq: number };
  /** 问题面板数据 */
  problems?: ProblemItem[];
  /** 报告面板：脚本里 `报告 xxx` 写给这里 */
  reports?: ReportRow[];
  onClearReports?: () => void;
  onOpenProblem?: (p: ProblemItem) => void;
};

/** 问题面板顶部：几错几警，一眼看到"有活要干" */
function ProblemHeader({ items }: { items: ProblemItem[] }) {
  const errs = items.filter((i) => i.severity === 'error').length;
  const warns = items.filter((i) => i.severity === 'warning').length;
  const infos = items.filter((i) => i.severity === 'info').length;
  if (items.length === 0) return null;
  return (
    <div className="prob-head">
      {errs > 0 && <span className="prob-count err">{errs} 个错误</span>}
      {warns > 0 && <span className="prob-count warn">{warns} 个警告</span>}
      {infos > 0 && <span className="prob-count info">{infos} 条提示</span>}
    </div>
  );
}

export function SidePanels({ source, file, onReveal, vars, running, width, tabRequest, problems, onOpenProblem, reports, onClearReports }: Props) {
  const [tab, setTab] = useState<'outline' | 'vars' | 'problems' | 'report'>('outline');
  const outline = useMemo(() => extractOutline(source), [source]);
  // 变量分成两组：脚本自己的变量，和语言自带的特殊变量（ans / __last_*）
  const isSpecial = (n: string): boolean => n === 'ans' || n.startsWith('__');
  const normalVars = useMemo(() => vars.filter((v) => !isSpecial(v.name)), [vars]);
  const specialVars = useMemo(() => vars.filter((v) => isSpecial(v.name)), [vars]);

  useEffect(() => {
    if (tabRequest) setTab(tabRequest.tab);
  }, [tabRequest?.seq]);

  return (
    <div className="side-panels" style={width ? { width } : undefined}>
      <div className="sp-tabs">
        <div className={`sp-tab ${tab === 'outline' ? 'active' : ''}`} onClick={() => setTab('outline')}>大纲</div>
        <div className={`sp-tab ${tab === 'vars' ? 'active' : ''}`} onClick={() => setTab('vars')}>变量</div>
        <div className={`sp-tab ${tab === 'report' ? 'active' : ''}`} onClick={() => setTab('report')}>
          报告{(reports?.length ?? 0) > 0 ? ` ${reports?.length}` : ''}
        </div>
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
            <ProblemHeader items={problems ?? []} />
            {(problems?.length ?? 0) === 0 && (
              <div className="muted" style={{ lineHeight: 1.7 }}>
                没有发现问题。语法错误、没定义的变量、运行时报错都会出现在这里，
                点一条就跳到出错的那一行。
              </div>
            )}
            {problems?.map((p, i) => {
              const srcTag = p.from === 'run' ? '运行' : p.code?.startsWith('undefined') ? '语义' : '语法';
              return (
                <div
                  key={`${p.file}:${p.line}:${p.col}:${i}`}
                  className={`prob-item ${p.severity}`}
                  onClick={() => onOpenProblem?.(p)}
                  title="点击跳到这一行"
                >
                  <span className="prob-dot" />
                  <span className="prob-main">
                    {/* 原因 / 什么问题 */}
                    <b>{p.message}</b>
                    {/* 第几行、第几列、哪个文件、哪一类 */}
                    <i>
                      第 {p.line} 行
                      {p.col > 0 ? ` 第 ${p.col} 列` : ''}
                      {' · '}
                      {p.file.split(/[\\/]/).pop()}
                      <span className="prob-tag">{srcTag}</span>
                    </i>
                  </span>
                </div>
              );
            })}
          </>
        ) : tab === 'report' ? (
          <>
            <div className="prob-head">
              {(reports?.length ?? 0) > 0 && (
                <span className="prob-count info">{reports?.length} 条报告</span>
              )}
              {(reports?.length ?? 0) > 0 && (
                <button className="status-link" onClick={() => onClearReports?.()}>清空</button>
              )}
            </div>
            {(reports?.length ?? 0) === 0 && (
              <div className="muted" style={{ lineHeight: 1.7 }}>
                脚本里写 <code className="inline">报告 值1, 值2</code> 就会出现在这里（写法跟 print 一样）。
                <br />
                只写一个 <code className="inline">报告</code> 不带内容，会报告当前所有变量。
              </div>
            )}
            {reports?.map((r, i) => (
              <div className="report-item" key={i} onClick={() => r.line > 0 && onReveal(r.line)} title={r.line > 0 ? `跳到第 ${r.line} 行` : ''}>
                {r.line > 0 && <span className="report-line">第 {r.line} 行</span>}
                <pre className="report-text">{r.text}</pre>
              </div>
            ))}
          </>
        ) : (
          <>
            {!running && vars.length === 0 && (
              <div className="muted">运行或调试时，变量会实时出现在这里。</div>
            )}
            {vars.length === 0 && running && <div className="muted">还没有变量。</div>}
            {vars.length > 0 && (
              <>
                {/* 上面：脚本自己的变量（名字 + 类型 + 当前值） */}
                <div className="vsec-tag">变量</div>
                {normalVars.length === 0 && <div className="muted" style={{ paddingLeft: 2 }}>还没有普通变量</div>}
                {normalVars.map((v) => (
                  <div className="kv" key={v.name}>
                    <span><code className="inline">{v.name}</code></span>
                    <span style={{ textAlign: 'right' }}>
                      <b className="mono" style={{ fontSize: 11.5 }}>{v.type}</b>
                      <span className="mono" style={{ marginLeft: 8, userSelect: 'text' }}>{v.value}</span>
                    </span>
                  </div>
                ))}
                {/* 分割线下面：语言自己的特殊变量（ans / __last_*） */}
                {specialVars.length > 0 && (
                  <>
                    <div className="vsec-split" />
                    <div className="vsec-tag special">特殊变量</div>
                    {specialVars.map((v) => (
                      <div className="kv special" key={v.name}>
                        <span><code className="inline">{v.name}</code></span>
                        <span style={{ textAlign: 'right' }}>
                          <b className="mono" style={{ fontSize: 11.5 }}>{v.type}</b>
                          <span className="mono" style={{ marginLeft: 8, userSelect: 'text' }}>{v.value}</span>
                        </span>
                      </div>
                    ))}
                  </>
                )}
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}
