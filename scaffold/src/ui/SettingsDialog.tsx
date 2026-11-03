import React, { useEffect, useState } from 'react';
import { applySettings, DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings, type ThemeMode } from './settings';
import { applyAppearance, loadAppearance, saveAppearance, type Appearance } from './appearance';

/* 偏好设置（PyCharm 的 Settings 简化版）：主题、编辑器、运行。 */

export function SettingsDialog({ onClose, onAppearance, onAbout, embedded }: { onClose: () => void; onAppearance?: () => void; onAbout?: () => void; embedded?: boolean }) {
  const [s, setS] = useState<Settings>(() => loadSettings());
  const [bg, setBg] = useState<Appearance>(() => loadAppearance());

  useEffect(() => { applySettings(s); saveSettings(s); }, [s]);
  useEffect(() => { applyAppearance(bg); saveAppearance(bg); }, [bg]);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]): void => setS((prev) => ({ ...prev, [k]: v }));
  const setA = (patch: Partial<Appearance>): void => setBg((prev) => ({ ...prev, ...patch }));

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
                {bg.bg && <span className="set-hint" style={{ margin: 0 }}>已设置 · 面板已换磨砂</span>}
              </div>
            )}
            {bg.bg && (
              <>
                <div className="set-row">
                  <label>磨砂程度</label>
                  <input type="range" min={0} max={24} value={bg.blur}
                    onChange={(e) => setA({ blur: Number(e.target.value) })} />
                  <b>{bg.blur}px</b>
                </div>
                <div className="set-hint">磨砂越大背景越柔、面板越透。0 就是高清原图贴在后面。</div>
                <div className="set-row">
                  <label>透明度</label>
                  <input type="range" min={10} max={100} value={Math.round(bg.opacity * 100)}
                    onChange={(e) => setA({ opacity: Number(e.target.value) / 100 })} />
                  <b>{Math.round(bg.opacity * 100)}%</b>
                </div>
                <div className="set-row">
                  <label>压暗</label>
                  <input type="range" min={0} max={90} value={Math.round(bg.dim * 100)}
                    onChange={(e) => setA({ dim: Number(e.target.value) / 100 })} />
                  <b>{Math.round(bg.dim * 100)}%</b>
                </div>
              </>
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
            <div className="set-hint">
              两个按钮是两套引擎：<b>F5 走 Rust 内核</b>（快；ask 是“重放”，rand / time 会重新执行）；
              <b>F6 走内置解释器</b>（支持断点，ask 真暂停）。纯计算两边结果一致；交互式小游戏请用 F6。
            </div>
          </section>

          <section>
            <h4>关于</h4>
            <div className="set-row">
              <label>版本与日志</label>
              <button className="btn" onClick={() => { onClose(); onAbout?.(); }}>关于 Float IDE…</button>
            </div>
            <div className="set-hint"><b>Float IDE</b> · 小方工作室 · 语言：<b>Copper</b>（源文件 .co）</div>
            <div className="set-hint">
              <b>Copper</b>，中文是「铜」——导电又导热，是流动性最好的金属之一，也是这门语言想要的样子：
              <b>流动</b>，一次编写，处处能跑（工程、插件、发布出去的应用，同一套源码）；
              也流动在纸面上——简单到随便给谁看，都能一眼读懂、随时上手改。
              它的原名是 CW Script，从 1.6.0 起正式改名 Copper，旧 .cw 文件全部兼容。
              吉祥物是廿七，一只三脚橘猫——所以第 27 行报错时会说「喵～第 27 行瘸了…」。
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
