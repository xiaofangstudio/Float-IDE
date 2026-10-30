import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api, type McpStatus } from '../core/tauri';
import { IconPlay, IconStop } from '../ui/icons';

/* MCP 面板：把 Float IDE 变成一个 MCP 服务器，外部 AI 客户端可以借此
 * 读工程、搜代码、跑脚本、拿诊断。
 *
 * 默认只绑 127.0.0.1，不对外监听；配置里给的一串 JSON 直接贴到客户端即可。 */

type Props = {
  projectDir: string;
  onClose: () => void;
  /** 作为整页标签打开时为 true */
  embedded?: boolean;
};

const DEFAULT_PORT = 17871;

const TOOL_DOC: { name: string; desc: string }[] = [
  { name: 'project_info', desc: '当前工程名称与根路径' },
  { name: 'list_files', desc: '递归列出工程文件（可筛后缀）' },
  { name: 'read_file', desc: '读工程内文件' },
  { name: 'write_file', desc: '原子写入工程文件' },
  { name: 'search', desc: '全文搜索，返回文件/行号/该行内容' },
  { name: 'run_script', desc: '用 Float 内核跑一段代码，返回输出与错误' },
  { name: 'get_diagnostics', desc: '对 .cw 做语法与语义检查' },
  { name: 'get_outline', desc: '返回文件的符号大纲' }
];

export function McpPanel({ projectDir, onClose, embedded }: Props) {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [port, setPort] = useState(DEFAULT_PORT);
  const [log, setLog] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState('');
  const [copied, setCopied] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async (): Promise<void> => {
    try {
      const st = await api.mcpStatus();
      setStatus(st);
      if (st.port) setPort(st.port);
      setLog(await api.mcpLog());
    } catch {
      setStatus(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = window.setInterval(() => { void refresh(); }, 2000);
    return () => window.clearInterval(t);
  }, [refresh]);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log.length]);

  const start = async (): Promise<void> => {
    setBusy(true);
    setMsg('');
    try {
      const st = await api.mcpStart(port, projectDir);
      setStatus(st);
      setLog(await api.mcpLog());
    } catch (e) {
      setMsg(String(e).replace(/^Error:\s*/, ''));
    } finally {
      setBusy(false);
    }
  };

  const stop = async (): Promise<void> => {
    setBusy(true);
    try {
      await api.mcpStop();
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const running = !!status?.running;
  const url = `http://127.0.0.1:${status?.port || port}/mcp`;

  const configJson = JSON.stringify({
    mcpServers: {
      'float-ide': {
        type: 'http',
        url
      }
    }
  }, null, 2);

  const copy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(configJson);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch { /* 忽略 */ }
  };

  return (
    <div className={`mcp-panel ${embedded ? 'embedded' : ''}`}>
      <div className="mcp-head">
        <span className={`mcp-dot ${running ? 'on' : ''}`} />
        <span className="mcp-title">MCP 服务器</span>
        <span className="mcp-state">{running ? '运行中' : '已停止'}</span>
        <span className="tool-grow" />
        {running
          ? <button className="pp-btn danger" disabled={busy} onClick={() => void stop()}><IconStop size={11} /> 停止</button>
          : <button className="pp-btn primary" disabled={busy} onClick={() => void start()}><IconPlay size={11} /> 启动</button>}
        {!embedded && <button className="pp-x" onClick={onClose}>✕</button>}
      </div>

      {!running && (
        <div className="mcp-row">
          <label>端口</label>
          <input
            type="number"
            value={port}
            min={1024}
            max={65535}
            onChange={(e) => setPort(Number(e.target.value) || DEFAULT_PORT)}
          />
          <span className="mcp-hint">只监听 127.0.0.1，外部网络访问不到</span>
        </div>
      )}

      {msg && <div className="mcp-err">{msg}</div>}

      <div className="mcp-row">
        <label>地址</label>
        <code className="mcp-url">{url}</code>
      </div>

      <div className="mcp-sect">
        <div className="mcp-sect-title">客户端配置（复制后贴到 Claude Desktop / Cursor / 任意 MCP 客户端）</div>
        <pre className="mcp-code">{configJson}</pre>
        <button className="pp-btn" onClick={() => void copy()}>{copied ? '已复制' : '复制配置'}</button>
      </div>

      <div className="mcp-sect">
        <div className="mcp-sect-title">对外提供的工具（{TOOL_DOC.length} 个）</div>
        <div className="mcp-tools">
          {TOOL_DOC.map((t) => (
            <div key={t.name} className="mcp-tool">
              <code>{t.name}</code>
              <span>{t.desc}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="mcp-sect grow">
        <div className="mcp-sect-title">请求日志</div>
        <div className="mcp-log" ref={logRef}>
          {log.length === 0 && <div className="mcp-empty">启动后这里会显示外部客户端发来的每一次调用</div>}
          {log.map((l, i) => <div key={i} className="mcp-log-line">{l}</div>)}
        </div>
      </div>
    </div>
  );
}
