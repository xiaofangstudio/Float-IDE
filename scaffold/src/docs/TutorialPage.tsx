import React, { useCallback, useMemo, useState } from 'react';
import { CHAPTERS, TUTORIAL_STATS, type CodeSample } from './tutorial';

/* 学习中心页面（占满一个标签页，不是浮窗）。
 *
 * PyCharm / VS Code 的文档页就是这个形态：左边一列目录，右边正文，
 * 代码块能直接跑、能插到编辑器里。这才是 IDE 里「学一门语言」该有的样子。
 */

type Props = {
  /** 把一段代码在新文件里打开（欢迎页没有编辑器时不传） */
  onOpenInEditor?: (code: string, name: string) => void;
  /** 运行一段代码；返回文本就直接显示在代码块下面 */
  onRun?: (code: string) => Promise<string> | string | void;
  /** 跳到插件开发文档那一页 */
  onOpenPluginsDoc?: () => void;
};

const KIND_STYLE: Record<'tip' | 'warn' | 'note', { label: string; cls: string }> = {
  tip: { label: '技巧', cls: 'tip' },
  warn: { label: '注意', cls: 'warn' },
  note: { label: '说明', cls: 'note' }
};

function CodeBlock({ sample, onRun, onOpenInEditor }: {
  sample: CodeSample;
  onRun?: (code: string) => Promise<string> | string | void;
  onOpenInEditor?: (code: string, name: string) => void;
}) {
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [inline, setInline] = useState<string | null>(null);
  const lines = sample.code.split('\n');

  const run = async (): Promise<void> => {
    if (!onRun || busy) return;
    setBusy(true);
    try {
      const r = await onRun(sample.code);
      if (typeof r === 'string') setInline(r);
    } finally {
      setBusy(false);
    }
  };

  const copy = (): void => {
    void navigator.clipboard.writeText(sample.code).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    }).catch(() => { /* 忽略 */ });
  };

  return (
    <div className="tut-code-wrap">
      <div className="tut-code">
        <div className="tut-code-head">
          <span className="tut-code-lang">Float Script</span>
          <span className="tool-grow" />
          <button className="tut-cbtn" onClick={copy}>{copied ? '已复制' : '复制'}</button>
          {onOpenInEditor && (
            <button className="tut-cbtn" onClick={() => onOpenInEditor(sample.code, '示例.cw')}>在编辑器中打开</button>
          )}
          {onRun && !sample.ask && (
            <button className="tut-cbtn primary" onClick={() => void run()} disabled={busy}>
              {busy ? '运行中…' : '运行'}
            </button>
          )}
        </div>
        <pre className="tut-pre">
          <code>
            {lines.map((ln, i) => (
              <span key={i} className="tut-line">
                <span className="tut-ln">{i + 1}</span>
                <span className="tut-src">{ln || ' '}</span>
              </span>
            ))}
          </code>
        </pre>
      </div>
      {/* 现场运行的结果优先显示，否则显示示例里标注的固定输出 */}
      {inline !== null ? (
        <div className="tut-out">
          <div className="tut-out-head">刚才的运行结果</div>
          <pre className="tut-out-body">{inline || '（没有输出）'}</pre>
        </div>
      ) : sample.out !== undefined && (
        <div className="tut-out">
          <div className="tut-out-head">预期输出</div>
          <pre className="tut-out-body">{sample.out}</pre>
        </div>
      )}
    </div>
  );
}

export function TutorialPage({ onOpenInEditor, onRun, onOpenPluginsDoc }: Props) {
  const [activeCh, setActiveCh] = useState(CHAPTERS[0].id);
  const [q, setQ] = useState('');
  const [done, setDone] = useState<Set<string>>(() => {
    try {
      const raw = window.localStorage.getItem('float.tutorial.done');
      return new Set<string>(raw ? (JSON.parse(raw) as string[]) : []);
    } catch { return new Set<string>(); }
  });

  const chapter = useMemo(
    () => CHAPTERS.find((c) => c.id === activeCh) ?? CHAPTERS[0],
    [activeCh]
  );

  const markDone = useCallback((id: string): void => {
    setDone((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      try { window.localStorage.setItem('float.tutorial.done', JSON.stringify([...next])); } catch { /* 忽略 */ }
      return next;
    });
  }, []);

  const filtered = useMemo(() => {
    const k = q.trim().toLowerCase();
    if (!k) return CHAPTERS;
    return CHAPTERS
      .map((c) => ({
        ...c,
        sections: c.sections.filter((s) =>
          s.title.toLowerCase().includes(k) ||
          (s.lead ?? '').toLowerCase().includes(k) ||
          (s.paras ?? []).join(' ').toLowerCase().includes(k) ||
          (s.codes ?? []).some((x) => x.code.toLowerCase().includes(k))
        )
      }))
      .filter((c) => c.sections.length > 0 || c.title.toLowerCase().includes(k));
  }, [q]);

  const progress = Math.round((done.size / TUTORIAL_STATS.sections) * 100);

  return (
    <div className="tut-page">
      {/* 左：目录 */}
      <aside className="tut-nav">
        <div className="tut-nav-head">
          <div className="tut-nav-title">Float Script 教程</div>
          <div className="tut-nav-sub">{TUTORIAL_STATS.chapters} 章 · {TUTORIAL_STATS.sections} 节 · {TUTORIAL_STATS.samples} 个示例</div>
          <div className="tut-progress">
            <div className="tut-progress-bar"><span style={{ width: progress + '%' }} /></div>
            <span className="tut-progress-num">{progress}%</span>
          </div>
        </div>
        <div className="tut-search">
          <input placeholder="搜索教程内容…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <div className="tut-nav-list">
          {filtered.map((c) => (
            <div key={c.id} className="tut-nav-group">
              <button
                className={`tut-nav-ch ${c.id === activeCh ? 'on' : ''}`}
                onClick={() => setActiveCh(c.id)}
              >
                <span className="tut-nav-icon">{c.icon}</span>
                <span className="tut-nav-text">{c.title}</span>
                <span className={`tut-nav-tick ${c.sections.every((s) => done.has(s.id)) ? 'all' : ''}`}>
                  {c.sections.filter((s) => done.has(s.id)).length}/{c.sections.length}
                </span>
              </button>
              {c.id === activeCh && (
                <div className="tut-nav-sections">
                  {c.sections.map((s) => (
                    <a
                      key={s.id}
                      className={`tut-nav-sec ${done.has(s.id) ? 'done' : ''}`}
                      href={`#sec-${s.id}`}
                      onClick={(e) => {
                        e.preventDefault();
                        document.getElementById(`sec-${s.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
                      }}
                    >
                      {s.title}
                    </a>
                  ))}
                </div>
              )}
            </div>
          ))}
          {filtered.length === 0 && <div className="tut-nav-empty">没有匹配的内容</div>}
        </div>
        {onOpenPluginsDoc && (
          <button className="tut-nav-foot" onClick={onOpenPluginsDoc}>写插件 →</button>
        )}
      </aside>

      {/* 右：正文 */}
      <main className="tut-main">
        <header className="tut-hero">
          <div className="tut-hero-icon">{chapter.icon}</div>
          <div>
            <h1 className="tut-hero-title">{chapter.title}</h1>
            <p className="tut-hero-sub">{chapter.summary}</p>
          </div>
        </header>

        {chapter.sections.map((s, i) => (
          <section key={s.id} id={`sec-${s.id}`} className="tut-sec">
            <div className="tut-sec-head">
              <span className="tut-sec-num">{i + 1}</span>
              <h2 className="tut-sec-title">{s.title}</h2>
              <button
                className={`tut-done ${done.has(s.id) ? 'on' : ''}`}
                onClick={() => markDone(s.id)}
                title={done.has(s.id) ? '标记为未读' : '标记为已读'}
              >
                {done.has(s.id) ? '✓ 已读' : '标记已读'}
              </button>
            </div>

            {s.lead && <p className="tut-lead">{s.lead}</p>}
            {(s.paras ?? []).map((p, k) => <p key={k} className="tut-para">{p}</p>)}
            {(s.codes ?? []).map((c, k) => (
              <CodeBlock key={k} sample={c} onRun={onRun} onOpenInEditor={onOpenInEditor} />
            ))}
            {(s.tips ?? []).map((t, k) => (
              <div key={k} className={`tut-tip ${KIND_STYLE[t.kind].cls}`}>
                <span className="tut-tip-label">{KIND_STYLE[t.kind].label}</span>
                <span>{t.text}</span>
              </div>
            ))}
          </section>
        ))}

        <footer className="tut-foot">
          {(() => {
            const idx = CHAPTERS.findIndex((c) => c.id === chapter.id);
            const prev = CHAPTERS[idx - 1];
            const next = CHAPTERS[idx + 1];
            return (
              <>
                {prev
                  ? <button className="tut-navbtn" onClick={() => setActiveCh(prev.id)}>← {prev.title}</button>
                  : <span />}
                {next
                  ? <button className="tut-navbtn primary" onClick={() => setActiveCh(next.id)}>{next.title} →</button>
                  : <span />}
              </>
            );
          })()}
        </footer>
      </main>
    </div>
  );
}
