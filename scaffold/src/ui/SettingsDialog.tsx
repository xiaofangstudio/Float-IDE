import React, { useEffect, useRef, useState } from 'react';
import { applySettings, DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings, type ThemeMode } from './settings';
import { applyAppearance, loadAppearance, saveAppearance, type Appearance } from './appearance';

/* 偏好设置（PyCharm 的 Settings 简化版）。
 * 布局：左侧分区导航 + 右侧分区卡片，整块尺寸固定；
 * 内容变多（比如选了背景图多出三根滑杆）只在右侧内部滚动，不撑大窗口。 */

type SectionId = 'appearance' | 'editor' | 'interface' | 'files' | 'about';

const SECTIONS: { id: SectionId; title: string; note: string }[] = [
  { id: 'appearance', title: '外观', note: '主题与背景' },
  { id: 'editor', title: '编辑器', note: '字体与缩进' },
  { id: 'interface', title: '界面', note: '密度与动效' },
  { id: 'files', title: '文件与运行', note: '保存与引擎' },
  { id: 'about', title: '关于', note: '版本与语言' }
];

function Row({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return <div className="set-row"><label>{label}</label>{children}</div>;
}

function Hint({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="set-hint">{children}</p>;
}

function Section({ id, title, note, children }: { id: SectionId; title: string; note: string; children: React.ReactNode }): React.ReactElement {
  return (
    <section className="set-sect" id={`set-${id}`}>
      <div className="set-sect-head"><h4>{title}</h4><span>{note}</span></div>
      <div className="set-sect-body">{children}</div>
    </section>
  );
}

export function SettingsDialog({ onClose, onAppearance, onAbout, embedded }: { onClose: () => void; onAppearance?: () => void; onAbout?: () => void; embedded?: boolean }) {
  const [s, setS] = useState<Settings>(() => loadSettings());
  const [bg, setBg] = useState<Appearance>(() => loadAppearance());
  const [active, setActive] = useState<SectionId>('appearance');
  const paneRef = useRef<HTMLDivElement>(null);

  useEffect(() => { applySettings(s); saveSettings(s); }, [s]);
  useEffect(() => { applyAppearance(bg); saveAppearance(bg); }, [bg]);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]): void => setS((prev) => ({ ...prev, [k]: v }));
  const setA = (patch: Partial<Appearance>): void => setBg((prev) => ({ ...prev, ...patch }));

  /* 点了分区就跳过去；滚动时高亮跟着走。用 scrollTop 而不是平滑滚动，
     免得动画把高亮一路带得乱跳。 */
  const go = (id: SectionId): void => {
    const pane = paneRef.current;
    const el = pane?.querySelector<HTMLElement>(`#set-${id}`);
    if (!pane || !el) return;
    pane.scrollTop = Math.max(0, el.offsetTop - 8);
    setActive(id);
  };

  const onPaneScroll = (): void => {
    const pane = paneRef.current;
    if (!pane) return;
    const line = pane.scrollTop + 16;
    let cur: SectionId = SECTIONS[0].id;
    for (const sec of SECTIONS) {
      const el = pane.querySelector<HTMLElement>(`#set-${sec.id}`);
      if (el && el.offsetTop <= line) cur = sec.id;
    }
    setActive(cur);
  };

  return (
    <div className={embedded ? 'doc-embed settings-host' : 'mask'} onMouseDown={embedded ? undefined : onClose}>
      <div className={embedded ? 'doc-embed-inner settings-shell embedded' : 'dialog settings settings-shell'} onMouseDown={(e) => e.stopPropagation()}>
        <div className="dlg-head"><span>偏好设置</span><button className="iconbtn" onClick={onClose}>✕</button></div>

        <div className="set-body">
          <nav className="set-nav" aria-label="设置分区">
            <div className="set-nav-cap">分区</div>
            {SECTIONS.map((sec) => (
              <button key={sec.id} className={`set-nav-item ${active === sec.id ? 'on' : ''}`} onClick={() => go(sec.id)}>{sec.title}</button>
            ))}
          </nav>

          <div className="set-pane" ref={paneRef} onScroll={onPaneScroll}>
            <Section id="appearance" title="外观" note="主题与背景">
              <Row label="主题">
                <div className="seg">
                  {([['light', '浅色'], ['dark', '深色'], ['system', '跟随系统']] as [ThemeMode, string][]).map(([v, t]) => (
                    <button key={v} className={s.theme === v ? 'on' : ''} onClick={() => set('theme', v)}>{t}</button>
                  ))}
                </div>
              </Row>
              {onAppearance && (
                <Row label="背景图">
                  <button className="btn" onClick={() => { onClose(); onAppearance(); }}>自定义背景…</button>
                </Row>
              )}
              {bg.bg && <Hint>已设置背景图 · 面板已换磨砂</Hint>}
              {bg.bg && (
                <>
                  <Row label="磨砂程度">
                    <input type="range" min={0} max={24} value={bg.blur}
                      onChange={(e) => setA({ blur: Number(e.target.value) })} />
                    <b>{bg.blur}px</b>
                  </Row>
                  <Hint>磨砂越大背景越柔、面板越透。0 就是高清原图贴在后面。</Hint>
                  <Row label="图片浓度">
                    <input type="range" min={10} max={100} value={Math.round(bg.opacity * 100)}
                      onChange={(e) => setA({ opacity: Number(e.target.value) / 100 })} />
                    <b>{Math.round(bg.opacity * 100)}%</b>
                  </Row>
                  <Row label="压暗">
                    <input type="range" min={0} max={90} value={Math.round(bg.dim * 100)}
                      onChange={(e) => setA({ dim: Number(e.target.value) / 100 })} />
                    <b>{Math.round(bg.dim * 100)}%</b>
                  </Row>
                </>
              )}
            </Section>

            <Section id="editor" title="编辑器" note="字体与缩进">
              <Row label="字号">
                <input type="range" min={10} max={28} value={s.fontSize} onChange={(e) => set('fontSize', Number(e.target.value))} />
                <b>{s.fontSize}px</b>
              </Row>
              <Row label="缩进">
                <div className="seg">
                  {[2, 4, 8].map((n) => (
                    <button key={n} className={s.tabSize === n ? 'on' : ''} onClick={() => set('tabSize', n)}>{n} 空格</button>
                  ))}
                </div>
              </Row>
              <Row label="自动换行">
                <button className={`switch ${s.wordWrap ? 'on' : ''}`} onClick={() => set('wordWrap', !s.wordWrap)}><span /></button>
              </Row>
            </Section>

            <Section id="interface" title="界面" note="密度与动效">
              <Row label="紧凑模式">
                <button className={`switch ${s.compact ? 'on' : ''}`} onClick={() => set('compact', !s.compact)}><span /></button>
              </Row>
              <Hint>行高与内边距收紧，一屏能多看几行代码。喜欢宽松的可以关掉。</Hint>
              <Row label="界面动画">
                <button className={`switch ${s.animations ? 'on' : ''}`} onClick={() => set('animations', !s.animations)}><span /></button>
              </Row>
              <Hint>面板淡入、按钮悬停位移等。关掉后界面瞬间响应，不拖泥带水。</Hint>
              <Row label="补全详情面板">
                <button className={`switch ${s.suggestDocs ? 'on' : ''}`} onClick={() => set('suggestDocs', !s.suggestDocs)}><span /></button>
              </Row>
              <Hint>补全时在右侧显示这个函数的说明与用法。关掉只留候选列表。</Hint>
            </Section>

            <Section id="files" title="文件与运行" note="保存与引擎">
              <Row label="自动保存">
                <button className={`switch ${s.autoSave ? 'on' : ''}`} onClick={() => set('autoSave', !s.autoSave)}><span /></button>
              </Row>
              <Row label="F5 默认">
                <div className="seg">
                  <button className={s.runMode === 'compile' ? 'on' : ''} onClick={() => set('runMode', 'compile')}>编译执行（快）</button>
                  <button className={s.runMode === 'debug' ? 'on' : ''} onClick={() => set('runMode', 'debug')}>解释执行（可断点）</button>
                </div>
              </Row>
              <Hint>
                两个按钮是两套引擎：<b>F5 走 Rust 内核</b>（快；ask 是“重放”，rand / time 会重新执行）；
                <b>F6 走内置解释器</b>（支持断点，ask 真暂停）。纯计算两边结果一致；交互式小游戏请用 F6。
              </Hint>
            </Section>

            <Section id="about" title="关于" note="版本与语言">
              <Row label="版本与日志">
                <button className="btn" onClick={() => { onClose(); onAbout?.(); }}>关于 Float IDE…</button>
              </Row>
              <Hint><b>Float IDE</b> · 小方工作室 · 语言：<b>Copper</b>（源文件 .co）</Hint>
              <Hint>
                <b>Copper</b>，中文是「铜」——导电又导热，是流动性最好的金属之一，也是这门语言想要的样子：
                <b>流动</b>，一次编写，处处能跑（工程、插件、发布出去的应用，同一套源码）；
                也流动在纸面上——简单到随便给谁看，都能一眼读懂、随时上手改。
                它的原名是 CW Script，从 1.6.0 起正式改名 Copper，旧 .cw 文件全部兼容。
                吉祥物是廿七，一只三脚橘猫——所以第 27 行报错时会说「喵～第 27 行瘸了…」。
              </Hint>
            </Section>
          </div>
        </div>

        <div className="dlg-foot">
          <button className="btn" onClick={() => setS({ ...DEFAULT_SETTINGS })}>恢复默认</button>
          <button className="btn primary" onClick={onClose}>完成</button>
        </div>
      </div>
    </div>
  );
}
