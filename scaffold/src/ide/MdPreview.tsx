import React, { useEffect, useMemo, useState } from 'react';
import type { editor as MonacoEditor } from 'monaco-editor';
import { IconClose } from '../ui/icons';

/* Markdown 实时预览：手写渲染器，不引第三方库（项目依赖克制是明确风格）。
 * 安全底线：任何原文文本先整体 HTML 转义，再生成的标签全部由本文件拼接，
 * 用户内容永远进不到标签属性之外的位置，dangerouslySetInnerHTML 是可控的。
 * 链接刻意不给 href（Tauri 里点了会整窗导航走），URL 以 tooltip 形式展示。 */

function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/* 行内语法：`code`、**粗**、*斜*、[文字](地址)。输入必须已转义。 */
function inline(s: string): string {
  return s
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, '$1<em>$2</em>')
    .replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, '<span class="md-img" title="$2">［图：$1］</span>')
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a class="md-link" title="$2">$1</a>');
}

/* 列表：支持两层嵌套（子项缩进 ≥2 空格）。返回 </ul>/<ol> 收口前的 HTML 片段数组。 */
function renderList(lines: string[], start: number, end: number): { html: string; next: number } {
  const ordered = /^\s*\d+\.\s+/.test(lines[start]);
  const tag = ordered ? 'ol' : 'ul';
  const items: string[] = [];
  let i = start;
  while (i < end) {
    const m = lines[i].match(/^(\s*)([-*+]|\d+\.)\s+(.*)$/);
    if (!m) break;
    const indent = m[1].length;
    if (indent >= 2 && items.length) {
      // 子项：收进上一个 li 里，嵌套一层
      const sub: string[] = [];
      const subOrdered = /^\d+\./.test(m[2]);
      sub.push(m[3]);
      i++;
      while (i < end && /^\s{2,}([-*+]|\d+\.)\s+/.test(lines[i])) {
        const sm = lines[i].match(/^\s*([-*+]|\d+\.)\s+(.*)$/);
        sub.push(sm ? sm[2] : lines[i].trim());
        i++;
      }
      items[items.length - 1] += `<${subOrdered ? 'ol' : 'ul'}>${sub
        .map((t) => `<li>${inline(esc(t))}</li>`)
        .join('')}</${subOrdered ? 'ol' : 'ul'}>`;
      continue;
    }
    items.push(inline(esc(m[3])));
    i++;
    // 同一项的续行（非列表、非空、有缩进）并入当前项
    while (i < end && lines[i].trim() && !/^\s*([-*+]|\d+\.)\s+/.test(lines[i]) && /^\s/.test(lines[i])) {
      items[items.length - 1] += '<br>' + inline(esc(lines[i].trim()));
      i++;
    }
  }
  return { html: `<${tag}>${items.map((t) => `<li>${t}</li>`).join('')}</${tag}>`, next: i };
}

/* 块级解析主循环 */
export function renderMarkdown(src: string): string {
  const lines = src.replace(/\r\n/g, '\n').split('\n');
  const out: string[] = [];
  let i = 0;
  const para: string[] = [];
  const flushPara = (): void => {
    if (para.length) {
      out.push(`<p>${para.map((l) => inline(esc(l))).join('<br>')}</p>`);
      para.length = 0;
    }
  };
  while (i < lines.length) {
    const line = lines[i];
    // 围栏代码块
    const fence = line.match(/^\s*```\s*(\S*)\s*$/);
    if (fence) {
      flushPara();
      const lang = fence[1];
      const buf: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) { buf.push(lines[i]); i++; }
      i++; // 没闭合就吃到文件尾（和顶格 /** 一个脾气）
      out.push(`<pre class="md-code"${lang ? ` data-lang="${esc(lang)}"` : ''}><code>${esc(buf.join('\n'))}</code></pre>`);
      continue;
    }
    // 标题
    const h = line.match(/^(#{1,6})\s+(.*)$/);
    if (h) {
      flushPara();
      const lv = h[1].length;
      out.push(`<h${lv}>${inline(esc(h[2]))}</h${lv}>`);
      i++;
      continue;
    }
    // 分隔线
    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      flushPara();
      out.push('<hr>');
      i++;
      continue;
    }
    // 引用
    if (/^\s*>\s?/.test(line)) {
      flushPara();
      const buf: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        buf.push(lines[i].replace(/^\s*>\s?/, ''));
        i++;
      }
      out.push(`<blockquote>${buf.map((l) => `<p>${inline(esc(l))}</p>`).join('')}</blockquote>`);
      continue;
    }
    // 列表
    if (/^\s*([-*+]|\d+\.)\s+/.test(line)) {
      flushPara();
      const r = renderList(lines, i, lines.length);
      out.push(r.html);
      i = r.next;
      continue;
    }
    // 表格：头行 + 分隔行
    if (
      /\|/.test(line) && i + 1 < lines.length
      && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1]) && /\|/.test(lines[i + 1])
    ) {
      flushPara();
      const cells = (l: string): string[] =>
        l.trim().replace(/^\|/, '').replace(/\|$/, '').split('|').map((c) => c.trim());
      const head = cells(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && /\|/.test(lines[i]) && lines[i].trim()) {
        rows.push(cells(lines[i]));
        i++;
      }
      out.push(
        `<table><thead><tr>${head.map((c) => `<th>${inline(esc(c))}</th>`).join('')}</tr></thead>`
        + `<tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${inline(esc(c))}</td>`).join('')}</tr>`).join('')}</tbody></table>`
      );
      continue;
    }
    // 空行 → 收段落
    if (!line.trim()) {
      flushPara();
      i++;
      continue;
    }
    para.push(line.trim());
    i++;
  }
  flushPara();
  return out.join('\n');
}

type Props = {
  /** 当前 md 文件的 Monaco 模型：订阅它就能实时跟着敲键刷新 */
  model: MonacoEditor.ITextModel | null;
  onClose: () => void;
};

export function MdPreview({ model, onClose }: Props): React.JSX.Element {
  const [text, setText] = useState<string>(() => model?.getValue() ?? '');
  useEffect(() => {
    if (!model) { setText(''); return; }
    setText(model.getValue());
    const d = model.onDidChangeContent(() => setText(model.getValue()));
    return () => d.dispose();
  }, [model]);
  const html = useMemo(() => renderMarkdown(text), [text]);
  return (
    <div className="md-split">
      <div className="md-split-head">
        <span>预览</span>
        <span style={{ flex: 1 }} />
        <button className="icon-btn" style={{ width: 20, height: 20 }} title="关闭预览" onClick={onClose}>
          <IconClose size={11} />
        </button>
      </div>
      <div className="md-preview-body" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}
