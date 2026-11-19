import React, { useEffect, useState } from 'react';
import pkg from '../../package.json';
import copperSrc from '../assets/copper.png';
import { api, isTauriEnv } from '../core/tauri';
import { Logo } from './icons';

/* 「关于 Float IDE」页：品牌、版本、最新日志，以及语言 Copper 的自我介绍。
 * 和偏好设置共用 doc-embed 版式，作为文档标签页打开，不抢编辑器焦点。 */

const VERSION = (pkg as { version?: string }).version ?? '1.6.0';

/* Copper 的版本不再写死：它属于「环境」，由 copper_env/copper.json 和
 * `colang x.x.xxx` 的二进制名决定。这里运行时问一次后端，
 * 环境换了、编译器换了，关于页跟着变——不会再出现"页面写 1.0.0、实际跑 1.9.2"。
 */
function useCopperVersion(): string {
  const [v, setV] = useState('—');
  useEffect(() => {
    if (!isTauriEnv()) return;
    let alive = true;
    void api
      .copperEnvInfo()
      .then((info) => {
        if (!alive) return;
        const text = (info.compiler_version || info.version || '').trim();
        if (text) setV(text.replace(/^v/, ''));
      })
      .catch(() => { /* 没有环境就保持 — */ });
    return () => { alive = false; };
  }, []);
  return v;
}

const CHANGELOG: { v: string; items: string[] }[] = [
  {
    v: '1.9.8',
    items: [
      'Shift+F5 / 红色停止按钮真的能停 Rust 内核：VM 在循环检查点上合作式退出',
      '关于页的 Copper 版本改为运行时从环境读取，不再写死 1.0.0',
      '工程索引的符号表补完后再返回，跳转定义不再偶尔"没找到"',
      'AOT 的 ask 加了真跑对拍（编出 exe、喂 stdin、和 VM 逐字比）'
    ]
  },
  {
    v: '1.6.0',
    items: [
      '语言正式改名 Copper（原 CW Script），源码扩展名改为 .co（纯小写），旧 .cw 全部兼容',
      '偏好设置新增「关于」，关于页入驻 IDE：版本、日志、语言介绍',
      '第 27 行报错会变成「喵～第 27 行瘸了…」——吉祥物的小脾气'
    ]
  },
  {
    v: '1.5.3',
    items: [
      '真指针：ptr 从"句柄"升级成可解引用的地址，& 取址、* 读写、指针偏移与悬空检测',
      '补上 and / or / not 关键字与中缀 mod，if / elif 分支之间允许空行',
      '缩进报错精确到差几个空格；class 可以省略 new'
    ]
  },
  {
    v: '1.5.2',
    items: [
      '移除 FFmpeg 依赖，安装包瘦身',
      'MCP 增加建/删文件、打开工程、点按钮、改设置、控制台输入；外部 AI 改完代码立刻回到编辑器',
      '控制台可自由拖高、可全屏；双击 .co 文件自动进侧边栏'
    ]
  },
  {
    v: '1.5.1',
    items: [
      'F6 调试模式下文件读写可用，交互输入不再中断',
      '右侧变量面板的字典 / 列表可展开收起，超长内容截断',
      '修复"一按运行就显示程序终止"与 Monaco 模型并发冲突'
    ]
  }
];

export function AboutPage({ onClose }: { onClose: () => void }) {
  const copperVersion = useCopperVersion();
  const [assoc, setAssoc] = useState<boolean | null>(null);
  const [assocBusy, setAssocBusy] = useState(false);
  const [assocMsg, setAssocMsg] = useState('');

  useEffect(() => {
    if (!isTauriEnv()) return;
    void api.assocStatus().then(setAssoc).catch(() => setAssoc(false));
  }, []);

  const doAssoc = async (): Promise<void> => {
    setAssocBusy(true);
    setAssocMsg('');
    try {
      await api.assocRegister();
      const now = await api.assocStatus();
      setAssoc(now);
      setAssocMsg(now ? '已关联。资源管理器里图标没变的话，按 F5 刷新那个文件夹。' : '写完了但状态没读到，重开一次资源管理器试试。');
    } catch (e) {
      setAssocMsg('关联失败：' + String(e).replace(/^Error:\s*/, ''));
    } finally {
      setAssocBusy(false);
    }
  };

  return (
    <div className="doc-embed">
      <div className="doc-embed-inner about-page">
        <div className="dlg-head">
          <span>关于 Float IDE</span>
          <button className="iconbtn" onClick={onClose}>✕</button>
        </div>

        <div className="dlg-body about-body">
          <section className="about-hero">
            <Logo size={76} />
            <div>
              <h3>Float IDE</h3>
              <div className="about-sub">版本 {VERSION} · 小方工作室 · MIT</div>
              <div className="about-sub">Copper 语言的官方 IDE —— 写、跑、调，一个窗口里全干完</div>
            </div>
          </section>

          <section>
            <h4>Windows 文件关联</h4>
            <div className="set-row">
              <label>.co / .cw 图标</label>
              <span className={`about-assoc ${assoc ? 'on' : ''}`}>
                {!isTauriEnv() ? '仅桌面版可用' : assoc === null ? '检测中…' : assoc ? '已关联' : '未关联'}
              </span>
              {isTauriEnv() && (
                <button className="btn" disabled={assocBusy} onClick={() => void doAssoc()}>
                  {assocBusy ? '正在写…' : '让资源管理器显示图标'}
                </button>
              )}
            </div>
            <div className="set-hint">
              绿色版没有安装程序，系统不会自动认识 .co。点一下就把当前程序挂成 .co / .cw 的打开方式（只写当前用户，不需要管理员），
              图标和双击打开都会恢复。程序挪了位置要再点一次。
            </div>
            {assocMsg && <div className="set-hint">{assocMsg}</div>}
          </section>

          <section>
            <h4>最新日志</h4>
            <ul className="about-log">
              {CHANGELOG.map((c) => (
                <li key={c.v}>
                  <span className="about-ver">{c.v}</span>
                  <div>
                    {c.items.map((it, i) => <div key={i}>{it}</div>)}
                  </div>
                </li>
              ))}
            </ul>
          </section>

          <section>
            <h4>关于 Copper</h4>
            <div className="about-brand">
              {/* Copper 的 logo：吉祥物廿七（橘猫），和 Float IDE 的图标同一套超椭圆圆角 */}
              <div className="about-logo copper">
                <img src={copperSrc} alt="Copper" draggable={false} />
              </div>
              <div>
                <h3>Copper</h3>
                <div className="about-sub">原 CW Script · 源文件扩展名 .co</div>
                <div className="about-sub">Copper {copperVersion === '—' ? '' : copperVersion} · 一门行式脚本语言，铜的流动性是它的名字来源</div>
              </div>
            </div>
            <p>
              Copper（铜）是一门<strong>行式脚本语言</strong>：一行一句、缩进成块、中英文都认。
              名字的上一站是 CW Script，1.6.0 起正式改名，语法基本没动，老代码不用重写。
            </p>
            <p><strong>底层是什么。</strong>三层结构，一条源码跑通三处：</p>
            <ul className="about-list">
              <li><b>词法 → AST → 字节码 → VM</b>：语言内核用 <b>Rust</b> 手写，不搭在别人的运行时上，为的是性能与将来的手动内存管理。</li>
              <li><b>F5 运行</b>：源码直接交给 Rust 内核的字节码 VM 执行，<b>不经过 JS / V8</b>；只有浏览器预览和非 Tauri 环境才回落到 TS 兼容引擎。</li>
              <li><b>AOT 发布</b>：同一份 AST 翻译成 <b>Rust 源码</b>，由 cargo 编成原生 exe，解压即用、不需要安装器。</li>
              <li><b>F6 调试</b>：内置解释器逐句跑，能断点、单步、看变量；两条通道结果一致，有双引擎对拍用例守着。</li>
            </ul>
            <p>
              实测（同一台机器交替跑）：循环约快 1.6 倍、字符串拼接约快 3～3.8 倍；
              纯整数递归约快 1.1～1.6 倍。倍数随机器状态浮动，别拿不同状态下的数字下结论。
              网页版演示走 TS 兼容引擎跑同一套语义——这就是「一次编写，处处能跑」。
            </p>
          </section>

          <section>
            <h4>有什么特点</h4>
            <ul className="about-list">
              <li><b>开箱即用</b>：没有构建等待，按下 F5 就跑；不需要配环境、装依赖。</li>
              <li><b>读起来像话</b>：<code>如果 血量 &lt; 10 就 提示 "快跑"</code> 这种写法直接在代码里用，中文变量名合法。</li>
              <li><b>真指针</b>：<code>ptr</code> 是地址不是句柄，<code>&amp;</code> 取址、<code>*</code> 解引用、可偏移、越界与悬空都报错；自动内存仍然是默认。</li>
              <li><b>两种执行模式</b>：编译执行求快，解释执行求看得清，随时切换。</li>
              <li><b>能长大</b>：类与继承、异常、模块 import、插件系统、发布成独立应用。</li>
            </ul>
          </section>

          <section>
            <h4>为什么叫 Copper</h4>
            <p>
              铜是流动性最好的金属之一：导电流、也导热流。这门语言想要的就是这两层「流动」——
              <b>对机器流动</b>：一份源码流到桌面、插件、浏览器里都能跑；
              <b>对人流动</b>：简单到随便递给谁都能读懂，改起来不费劲。
            </p>
          </section>

          <section>
            <h4>吉祥物为什么是猫</h4>
            <p>
              吉祥物叫 <b>廿七</b>，一只三脚大橘猫。它是在腊月二十七的雪后清晨被捡回来的，
              名字就这么定了。少一条腿，走路一晃一晃，但从来不耽误它往暖和地方钻。
            </p>
            <p>
              所以我们留了个小规矩：代码跑到<b>第 27 行</b>出错时，报错前缀会变成
              <code>喵～第 27 行瘸了…</code>，后面照常写清楚原因。
              在满是红字的控制台里，偶尔瘸一下也挺好。
            </p>
          </section>
        </div>
      </div>
    </div>
  );
}
