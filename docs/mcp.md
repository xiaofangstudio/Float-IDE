# Float IDE 的 MCP 服务器

MCP（Model Context Protocol）让外部 AI 客户端直接操作你的工程：读文件、搜代码、跑脚本、拿诊断。Float IDE 内置了一个 MCP 服务器，**不需要装任何东西**。

---

## 1. 启动

「工具 → MCP 服务器…」，或者点工具栏的服务器图标，或者状态栏的 `MCP 未启动`。

- 默认端口 `17871`
- **只监听 `127.0.0.1`**，外部网络访问不到
- 启动后状态栏会显示 `MCP :17871`

---

## 2. 接到客户端

面板里有一份现成的配置，点「复制配置」：

```json
{
  "mcpServers": {
    "float-ide": {
      "type": "http",
      "url": "http://127.0.0.1:17871/mcp"
    }
  }
}
```

贴到 Claude Desktop / Cursor / 任何支持 Streamable HTTP 的 MCP 客户端的配置里，重启客户端即可。

---

## 3. 提供的工具

| 工具 | 作用 | 在哪执行 |
| --- | --- | --- |
| `project_info` | 当前工程名称与根路径 | Rust |
| `list_files` | 递归列工程文件，可筛后缀 | Rust |
| `read_file` | 读工程内文件 | Rust |
| `write_file` | 原子写入（先写临时文件再改名） | Rust |
| `search` | 全文搜索，返回文件 / 行号 / 该行内容 | Rust |
| `run_script` | `code` 跑一段孤立代码；`path` 跑整个工程主文件（含 import），返回输出、诊断、报错与行号 | 界面 → Rust 内核 |
| `get_diagnostics` | 对 `.co/.cw` 做语法与语义检查，**会先摊平 import**，诊断带「（在 X 里）」的来源标注 | 界面 → Rust 内核 |
| `get_outline` | 返回文件的函数 / 类 / 变量大纲 | 界面（TS AST） |

**两条运行路径，别混**：

- `run_script` 只传 `code` = 用 TS 兼容引擎跑一段孤立代码，适合试小片段；
- `run_script` 传 `path` = 和 **F5 完全一致**：Rust 内核编译执行，import 就地摊平、
  未存盘的缓冲区一起带上、报错行号映射回原文件。改多文件工程要用这条。

`get_diagnostics` 也走 Rust 内核的静态检查：先把所有 import 摊平成一份完整程序，
再在整份程序上判断。所以「变量放在数据文件里、主文件里用」不会报「还没定义过」，
全程序都找不到的名字才会报。请求由界面转给内核执行（20 秒超时，不会卡死）。

改完工程让 AI 跑一遍（返回的是结构化 JSON，能直接定位到文件和行号）：

```bash
curl -s http://127.0.0.1:17871/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":9,"method":"tools/call",
       "params":{"name":"run_script","arguments":{"path":"main.co"}}}'
```

返回里 `ok / output / reports / error / error_line / need_input` 一应俱全，
`error_line` 是**原始文件**里的行号；配合 `get_diagnostics` 就能针对性改。

---

## 4. 一次真实调用长什么样

```bash
curl -s http://127.0.0.1:17871/mcp \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "run_script",
      "arguments": { "code": "int s = 0\nfor i in range(1, 101)\n    s += i\nprint(s)" }
    }
  }'
```

返回：

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "content": [{ "type": "text", "text": "5050" }] }
}
```

搜代码：

```bash
curl -s http://127.0.0.1:17871/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call",
       "params":{"name":"search","arguments":{"query":"function","maxResults":5}}}'
```

---

## 5. 协议细节

- 传输：**Streamable HTTP**（MCP 2025 规范），`POST /mcp` 直接返回 `application/json`
- 也支持 `GET /mcp` / `GET /health` 返回 `{"ok":true,"name":"float-ide-mcp"}`
- 支持单条请求与**批量数组**请求
- `notifications/*`（无 id）返回 `202`，无响应体
- 协议版本：`2025-06-18`
- 已实现方法：`initialize`、`tools/list`、`tools/call`、`resources/list`（空）、`prompts/list`（空）、`ping`

---

## 6. 安全

| 措施 | 说明 |
| --- | --- |
| 只绑 127.0.0.1 | 局域网里的其他机器连不上 |
| 路径收敛 | 所有 `path` 参数禁止 `..`，写文件也只在工程根内 |
| 原子写 | `write_file` 先写临时文件再改名，不会留下半截文件 |
| 请求日志 | 面板里的日志记录每一次 `→ method`，能看清谁在动你的代码 |
| 随时可停 | 面板上「停止」，端口立刻释放 |

> 注意：MCP 客户端拿到的是**你当前工程的读写权限**。给不受信任的客户端连之前，先想清楚。

---

## 7. 为什么值得接

不用 MCP 的时候，AI 只能看你在对话框里贴的那几段代码；接上之后，它可以：

- 自己找到相关文件（`list_files` + `search`）
- 看懂你的函数和类结构（`get_outline`）
- 改完立刻知道有没有写错（`get_diagnostics`）
- 真的跑一遍验证（`run_script`，走的就是 F5 那条编译通道）

这就是"AI 能在这个工程里干活"和"AI 只能猜"的区别。
