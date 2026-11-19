import React, { useEffect, useRef, useState } from 'react';
import { applySettings, DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings, type ThemeMode } from './settings';
import { applyAppearance, loadAppearance, saveAppearance, type Appearance } from './appearance';
import { api, isTauriEnv, type CopperEnvInfo } from '../core/tauri';

/* 偏好设置（PyCharm 的 Settings 简化版）。
 * 布局：左侧分区导航 + 右侧分区卡片，整块尺寸固定；
 * 内容变多（比如选了背景图多出三根滑杆）只在右侧内部滚动，不撑大窗口。 */

type SectionId = 'appearance' | 'editor' | 'interface' | 'files' | 'copper' | 'about';

const SECTIONS: { id: SectionId; title: string; note: string }[] = [
  { id: 'appearance', title: '外观', note: '主题与背景' },
  { id: 'editor', title: '编辑器', note: '字体与缩进' },
  { id: 'interface', title: '界面', note: '密度与动效' },
  { id: 'files', title: '文件与运行', note: '保存与引擎' },
  { id: 'copper', title: 'Copper 环境', note: '解释器与文档' },
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

  /* Copper 环境：解释器绑定 + 环境自带文档（关于 Copper 由环境提供） */
  const [env, setEnv] = useState<CopperEnvInfo | null>(null);
  const [aboutDoc, setAboutDoc] = useState<string>('');
  const [envBusy, setEnvBusy] = useState(false);

  useEffect(() => {
    if (!isTauriEnv()) return;
    let alive = true;
    void (async () => {
      try {
        const info = await api.copperEnvInfo();
        if (alive) setEnv(info);
      } catch { /* 环境没装就留空，页面会说明 */ }
      try {
        const doc = await api.copperEnvReadAsset('docs/about-copper.md');
        if (alive) setAboutDoc(doc);
      } catch { /* 环境没带文档就退回内置文案 */ }
    })();
    return () => { alive = false; };
  }, []);

  /** 查找 Copper：手选一个 colang 解释器绑上（exe 名字叫 colang，不叫 copper） */
  const findCopper = async (): Promise<void> => {
    if (!isTauriEnv()) return;
    setEnvBusy(true);
    try {
      const picked = await api.pickOpenFile('选择 Copper 解释器（文件名是 colang 开头）', ['exe']);
      if (!picked) return;
      const info = await api.copperEnvBindCompiler(picked);
      setEnv(info);
      try { setAboutDoc(await api.copperEnvReadAsset('docs/about-copper.md')); } catch { /* 文档缺也无所谓 */ }
    } catch (e) {
      window.alert(`绑定失败：${String(e)}`);
    } finally {
      setEnvBusy(false);
    }
  };

  /** 安装环境：选一个 Copper 环境包（zip）。装到 %APPDATA%\Float\copper 并立刻生效 */
  const installEnv = async (): Promise<void> => {
    if (!isTauriEnv()) return;
    setEnvBusy(true);
    try {
      const picked = await api.pickOpenFile('选择 Copper 环境包（Copper-Env-*.zip）', ['zip']);
      if (!picked) return;
      const info = await api.copperEnvInstall(picked);
      setEnv(info);
      try { setAboutDoc(await api.copperEnvReadAsset('docs/about-copper.md')); } catch { /* 文档缺也无所谓 */ }
      if (info.found) window.alert(`环境已装好：${info.compiler_name}`);
    } catch (e) {
      window.alert(`安装失败：${String(e)}`);
    } finally {
      setEnvBusy(false);
    }
  };

  const unbindCopper = async (): Promise<void> => {
    if (!isTauriEnv()) return;
    try { setEnv(await api.copperEnvUnbindCompiler()); } catch { /* 忽略 */ }
  };

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
              <Row label="智能补全">
                <button className={`switch ${s.smartComplete ? 'on' : ''}`} onClick={() => set('smartComplete', !s.smartComplete)}><span /></button>
              </Row>
              <Hint>
                本地统计模型：只用工程里的 Copper 代码学词对搭配，结合当前作用域给候选排序，
                不联网、不上传、不存源码。默认开启；改哪个文件只重算哪个（空闲时才算），
                不会让 CPU 一直转。关掉则只按固定优先级排序。
              </Hint>
              <Row label="行内续写">
                <button className={`switch ${s.inlineSuggest ? 'on' : ''}`} onClick={() => set('inlineSuggest', !s.inlineSuggest)}><span /></button>
              </Row>
              <Hint>
                行尾打完空格后，给出一个词的灰色建议（来自同一份本地统计），Tab 接受、Esc 忽略。
                只在置信度够高时才出现，不打断正常打字。
              </Hint>
            </Section>

            <Section id="files" title="文件与运行" note="保存与引擎">
              <Row label="自动保存">
                <button className={`switch ${s.autoSave ? 'on' : ''}`} onClick={() => set('autoSave', !s.autoSave)}><span /></button>
              </Row>
              <Row label="F5 默认">
                <div className="seg">
                  <button className={s.runMode === 'compile' ? 'on' : ''} onClick={() => set('runMode', 'compile')}>VM（快启动）</button>
                  <button className={s.runMode === 'aot' ? 'on' : ''} onClick={() => set('runMode', 'aot')}>AOT（发布级）</button>
                  <button className={s.runMode === 'debug' ? 'on' : ''} onClick={() => set('runMode', 'debug')}>解释器（断点）</button>
                </div>
              </Row>
              <Hint>
                三套引擎，按需要选：<b>VM</b> 是本机字节码解释（F5 默认，ask 真暂停、启动快）；
                <b>AOT</b> 用环境里的 <code>colang</code> 把代码编成原生 exe 再跑（首次编译 10~20 s，之后按内容缓存秒开，
                ask 也支持，适合跑性能敏感/接近发布版的程序；编的是磁盘上的文件，改完先保存）；
                <b>解释器</b> 是 F6 那套 JS 实现（可断点单步）。纯计算三者结果一致。
              </Hint>
            </Section>

            <Section id="copper" title="Copper 环境" note="解释器与文档">
              <Row label="当前环境">
                <span className="set-val">
                  {env
                    ? (env.found ? `已找到 · ${env.compiler_name}` : '未找到可用解释器')
                    : (isTauriEnv() ? '查询中…' : '浏览器预览：不读环境')}
                </span>
              </Row>
              {env?.found && (
                <>
                  <Hint>环境目录：<code>{env.home}</code>（来源 {env.source}）</Hint>
                  <Hint>解释器：<code>{env.compiler}</code></Hint>
                  <Hint>平台 {env.platform}｜版本 {env.compiler_version || env.version || '未标'}｜文档 {env.docs ? '有（教程与说明都由环境提供）' : '无'}</Hint>
                </>
              )}
              <Row label="安装环境">
                <button className="btn" disabled={envBusy} onClick={() => void installEnv()}>
                  {envBusy ? '安装中…' : '安装 Copper 环境（选 zip）…'}
                </button>
              </Row>
              <Hint>
                Float IDE 和 Copper 是两个独立的东西：<b>先装环境</b>（Copper-Env-x.x.x.zip，
                里面是 <code>colang x.x.xxx.exe</code> 和文档），<b>再装 IDE</b>。环境装到
                <code>%APPDATA%\Float\copper</code>，IDE 启动时自己找；环境更新时教程、关于页、编译器一起更新。
              </Hint>
              <Row label="查找 Copper">
                <button className="btn" disabled={envBusy} onClick={() => void findCopper()}>
                  {envBusy ? '选择中…' : '查找 Copper…'}
                </button>
              </Row>
              <Hint>
                <b>注意：Copper 的解释器不叫 copper，叫 <code>colang</code>。</b>
                文件名格式是 <code>colang 1.9.2.exe</code>（<code>colang 大.中.小.exe</code>）；
                环境里有多份时优先用版本号最新的那一份。选中这个 exe 就会绑上，下次启动照旧生效。
              </Hint>
              {env?.found && (
                <Row label="解除绑定">
                  <button className="btn" onClick={() => void unbindCopper()}>恢复自动查找</button>
                </Row>
              )}
              <Hint>
                教程文档与下面的「关于 Copper」都由环境提供（<code>docs/tutorial.json</code>、<code>docs/about-copper.md</code>），
                环境更新时它们一起更新，IDE 不再自带会过期的副本。
              </Hint>
            </Section>

            <Section id="about" title="关于" note="版本与语言">
              <Row label="Float IDE">
                <button className="btn" onClick={() => { onClose(); onAbout?.(); }}>关于 Float IDE…</button>
              </Row>
              <Hint><b>Float IDE</b> · 小方工作室 · 语言：<b>Copper</b>（源文件 .co）</Hint>
              {aboutDoc
                ? <div className="set-doc">{aboutDoc}</div>
                : (
                  <Hint>
                    <b>Copper</b>，中文是「铜」——导电又导热，是流动性最好的金属之一，也是这门语言想要的样子：
                    <b>流动</b>，一次编写，处处能跑。原名 CW Script，从 1.6.0 起正式改名 Copper，旧 .cw 文件全部兼容。
                    <br />
                    （这段是 IDE 内置兜底文案；环境里带了 <code>docs/about-copper.md</code> 时会整段换成环境提供的内容。）
                  </Hint>
                )}
              <p className="set-hint set-doc-src">关于页来源：{aboutDoc ? 'Copper 环境（docs/about-copper.md）' : 'IDE 内置兜底'}</p>
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
