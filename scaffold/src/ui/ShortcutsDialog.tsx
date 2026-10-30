import React from 'react';

/* 快捷键一览：菜单「帮助 → 快捷键」和 F1 都打开这里。 */

const GROUPS: { title: string; rows: [string, string][] }[] = [
  {
    title: '文件',
    rows: [
      ['Ctrl + N', '新建工程（欢迎页）'],
      ['Ctrl + O', '打开工程'],
      ['Ctrl + S', '保存所有修改'],
      ['Ctrl + Shift + E', '导出工程包（.cw）'],
      ['Ctrl + W', '关闭当前文件 / 关闭工程']
    ]
  },
  {
    title: '编辑',
    rows: [
      ['Ctrl + Z / Ctrl + Y', '撤销 / 重做'],
      ['Ctrl + F / Ctrl + H', '查找 / 替换'],
      ['Ctrl + G', '跳到指定行'],
      ['Ctrl + /', '注释或取消注释当前行'],
      ['Ctrl + Shift + P', '命令面板'],
      ['Ctrl + = / Ctrl + -', '放大 / 缩小字号'],
      ['Ctrl + 0', '字号恢复默认']
    ]
  },
  {
    title: '运行与调试',
    rows: [
      ['F5', '运行（编译执行，最快）'],
      ['F6', '调试（解释执行，可断点）'],
      ['F8', '继续到下一个断点'],
      ['F10 / F11', '单步跳过 / 单步进入'],
      ['Shift + F11', '单步跳出'],
      ['Shift + F5', '停止'],
      ['F9', '在光标行切换断点']
    ]
  },
  {
    title: '视图',
    rows: [
      ['Alt + 1 / Alt + 2', '资源管理器 / 源代码管理'],
      ['Alt + 3', '大纲面板'],
      ['Ctrl + `', '显示或隐藏底部控制台'],
      ['F1', '快捷键一览']
    ]
  }
];

export function ShortcutsDialog({ onClose, embedded }: { onClose: () => void; embedded?: boolean }) {
  return (
    <div className={embedded ? 'doc-embed' : 'mask'} onMouseDown={embedded ? undefined : onClose}>
      <div className={embedded ? 'doc-embed-inner' : 'dialog shortcuts'} onMouseDown={(e) => e.stopPropagation()}>
        <div className="dlg-head"><span>快捷键一览</span><button className="iconbtn" onClick={onClose}>✕</button></div>
        <div className="dlg-body sc-body">
          {GROUPS.map((g) => (
            <div className="sc-group" key={g.title}>
              <h4>{g.title}</h4>
              {g.rows.map(([k, d]) => (
                <div className="sc-row" key={k}>
                  <kbd>{k}</kbd>
                  <span>{d}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
        <div className="dlg-foot"><button className="btn primary" onClick={onClose}>知道了</button></div>
      </div>
    </div>
  );
}
