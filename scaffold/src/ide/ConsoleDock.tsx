import React, { useEffect, useRef, useState } from 'react';
import type { ConsoleLine } from '../core/types';
import { IconClose, IconPlus } from '../ui/icons';

/* 底部 dock：控制台（print 输出 / ask 提问输入 / 运行信息） + 真终端。 */

type Props = {
  lines: ConsoleLine[];
  askQuestion: string | null;
  onAnswer: (answer: string) => void;
  running: boolean;
  stateText: string;
  tab: 'console' | 'terminal' | 'search';
  onTab: (t: 'console' | 'terminal' | 'search') => void;
  /** 「在文件中查找」面板（由外部注入，避免把索引逻辑塞进这里） */
  searchNode?: React.ReactNode;
  projectDir: string;
  onClear: () => void;
  height?: number;
};

function TerminalLazy({ projectDir, visible }: { projectDir: string; visible: boolean }) {
  const [Comp, setComp] = useState<React.ComponentType<{ projectDir: string }> | null>(null);
  useEffect(() => {
    if (!visible || Comp) return;
    void import('./TerminalPane').then((m) => setComp(() => m.TerminalPane));
  }, [visible, Comp]);
  return Comp ? <Comp projectDir={projectDir} /> : <div className="term-host" />;
}

export function ConsoleDock({ lines, askQuestion, onAnswer, running, stateText, tab, onTab, projectDir, onClear, height, searchNode }: Props) {
  const [input, setInput] = useState('');
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines.length, askQuestion]);

  return (
    <div className="term-dock" style={height ? { height } : undefined}>
      <div className="term-tabs">
        <span className="term-label">调试输出</span>
        <div className={`term-tab ${tab === 'console' ? 'active' : ''}`} onClick={() => onTab('console')}>控制台</div>
        <div className={`term-tab ${tab === 'terminal' ? 'active' : ''}`} onClick={() => onTab('terminal')}>终端</div>
        <div className={`term-tab ${tab === 'search' ? 'active' : ''}`} onClick={() => onTab('search')}>查找</div>
        {tab === 'console' && (
          <>
            <button
              className="icon-btn"
              style={{ width: 'auto', padding: '0 8px', height: 22, fontSize: 11 }}
              title="复制全部输出"
              onClick={() => { void navigator.clipboard.writeText(lines.map((l) => l.text).join('\n')); }}
            >
              复制
            </button>
            <button className="icon-btn" style={{ width: 22, height: 22 }} title="清空控制台" onClick={onClear}>
              <IconClose size={11} />
            </button>
          </>
        )}
        {tab === 'terminal' && <span style={{ marginLeft: 4, display: 'flex' }}><IconPlus size={0} /></span>}
        <span style={{ flex: 1 }} />
        <span className="muted" style={{ fontSize: 11, marginRight: 8 }}>{stateText}</span>
      </div>
      {tab === 'search' ? (
        searchNode ?? <div className="muted" style={{ padding: 12 }}>索引构建中…</div>
      ) : tab === 'console' ? (
        <div className="console-wrap">
          <div className="console-list" ref={listRef}>
            {lines.length === 0 && <div className="muted" style={{ padding: '6px 2px' }}>输出会显示在这里。F5 运行，F6 调试（支持断点与单步）。</div>}
            {lines.map((l, i) => (
              // 脚本自己的输出不打标签，看着就像真控制台；
              // 只有"错误 / 信息"这种系统消息才加前缀，免得满屏都是「输出」两个字。
              l.kind === 'out' ? (
                <div key={i} className="console-line out">
                  <span style={{ whiteSpace: 'pre-wrap' }}>{l.text}</span>
                </div>
              ) : (
                <div key={i} className={`console-line ${l.kind}`}>
                  <span className="console-tag">{l.kind === 'err' ? '错误' : '信息'}</span>
                  <span style={{ whiteSpace: 'pre-wrap' }}>{l.text}</span>
                </div>
              )
            ))}
            {askQuestion !== null && (
              <div className="console-line info ask-row">
                <span className="console-tag">提问</span>
                <span>{askQuestion}</span>
              </div>
            )}
          </div>
          {askQuestion !== null && (
            <form
              className="ask-input-row"
              onSubmit={(e) => { e.preventDefault(); onAnswer(input); setInput(''); }}
            >
              <input
                autoFocus
                type="text"
                placeholder="输入回答，回车提交……"
                value={input}
                onChange={(e) => setInput(e.target.value)}
              />
              <button className="btn primary" style={{ fontSize: 12, padding: '5px 14px' }} type="submit">回答</button>
            </form>
          )}
          {running && askQuestion === null && <div className="console-running">脚本运行中……</div>}
        </div>
      ) : (
        <TerminalLazy projectDir={projectDir} visible={tab === 'terminal'} />
      )}
    </div>
  );
}
