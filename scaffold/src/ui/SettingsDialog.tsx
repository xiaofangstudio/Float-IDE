import React, { useEffect, useState } from 'react';
import { applySettings, DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings, type ThemeMode } from './settings';

/* 偏好设置（PyCharm 的 Settings 简化版）：主题、编辑器、运行。 */

export function SettingsDialog({ onClose, onAppearance, embedded }: { onClose: () => void; onAppearance?: () => void; embedded?: boolean }) {
  const [s, setS] = useState<Settings>(() => loadSettings());

  useEffect(() => { applySettings(s); saveSettings(s); }, [s]);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]): void => setS((prev) => ({ ...prev, [k]: v }));

  return (
    <div className={embedded ? 'doc-embed' : 'mask'} onMouseDown={embedded ? undefined : onClose}>
      <div className={embedded ? 'doc-embed-inner' : 'dialog settings'} onMouseDown={(e) => e.stopPropagation()}>
        <div className="dlg-head"><span>偏好设置</span><button className="iconbtn" onClick={onClose}>✕</button></div>

        <div className="dlg-body set-body">
          <section>
            <h4>外观</h4>
            <div className="set-row">
              <label>主题</label>
              <div className="seg">
                {([['light', '浅色'], ['dark', '深色'], ['system', '跟随系统']] as [ThemeMode, string][]).map(([v, t]) => (
                  <button key={v} className={s.theme === v ? 'on' : ''} onClick={() => set('theme', v)}>{t}</button>
                ))}
              </div>
            </div>
            {onAppearance && (
              <div className="set-row">
                <label>背景图</label>
                <button className="btn" onClick={() => { onClose(); onAppearance(); }}>自定义背景…</button>
              </div>
            )}
          </section>

          <section>
            <h4>编辑器</h4>
            <div className="set-row">
              <label>字号</label>
              <input type="range" min={10} max={28} value={s.fontSize} onChange={(e) => set('fontSize', Number(e.target.value))} />
              <b>{s.fontSize}px</b>
            </div>
            <div className="set-row">
              <label>缩进</label>
              <div className="seg">
                {[2, 4, 8].map((n) => (
                  <button key={n} className={s.tabSize === n ? 'on' : ''} onClick={() => set('tabSize', n)}>{n} 空格</button>
                ))}
              </div>
            </div>
            <div className="set-row">
              <label>自动换行</label>
              <button className={`switch ${s.wordWrap ? 'on' : ''}`} onClick={() => set('wordWrap', !s.wordWrap)}><span /></button>
            </div>
          </section>

          <section>
            <h4>界面</h4>
            <div className="set-row">
              <label>紧凑模式</label>
              <button className={`switch ${s.compact ? 'on' : ''}`} onClick={() => set('compact', !s.compact)}><span /></button>
            </div>
            <div className="set-hint">行高与内边距收紧，一屏能多看几行代码。喜欢宽松的可以关掉。</div>
            <div className="set-row">
              <label>界面动画</label>
              <button className={`switch ${s.animations ? 'on' : ''}`} onClick={() => set('animations', !s.animations)}><span /></button>
            </div>
            <div className="set-hint">面板淡入、按钮悬停位移等。关掉后界面瞬间响应，不拖泥带水。</div>
            <div className="set-row">
              <label>补全详情面板</label>
              <button className={`switch ${s.suggestDocs ? 'on' : ''}`} onClick={() => set('suggestDocs', !s.suggestDocs)}><span /></button>
            </div>
            <div className="set-hint">补全时在右侧显示这个函数的说明与用法。关掉只留候选列表。</div>
          </section>

          <section>
            <h4>文件与运行</h4>
            <div className="set-row">
              <label>自动保存</label>
              <button className={`switch ${s.autoSave ? 'on' : ''}`} onClick={() => set('autoSave', !s.autoSave)}><span /></button>
            </div>
            <div className="set-row">
              <label>F5 默认</label>
              <div className="seg">
                <button className={s.runMode === 'compile' ? 'on' : ''} onClick={() => set('runMode', 'compile')}>编译执行（快）</button>
                <button className={s.runMode === 'debug' ? 'on' : ''} onClick={() => set('runMode', 'debug')}>解释执行（可断点）</button>
              </div>
            </div>
          </section>
        </div>

        <div className="dlg-foot">
          <button className="btn" onClick={() => setS({ ...DEFAULT_SETTINGS })}>恢复默认</button>
          <button className="btn primary" onClick={onClose}>完成</button>
        </div>
      </div>
    </div>
  );
}
