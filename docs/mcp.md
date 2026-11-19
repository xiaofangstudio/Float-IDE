# Float IDE 的 MCP 服务器

MCP（Model Context Protocol）让外部 AI 客户端直接操作你的工程：读文件、搜代码、跑脚本、拿诊断。Float IDE 内置了一个 MCP 服务器，**不需要装任何东西**。

---

## 1. 启动

「工具 → MCP 服务器…」，或者点工具栏的服务器图标，或者状态栏的 `MCP 未启动`。

- 默认端口 `17871`
- **只监听 `127.0.0.1`**，外部网络访问不到
- 启动后状态栏会显示 `MCP :17871`

### 无界面模式（给命令行里的 AI）

不打开 IDE 窗口也能跑同一个服务器，直接指向一个工程：

```bash
"Float IDE\floatide.exe" --mcp 17871 --project D:\proj
```

也可以用环境变量 `COPPER_MCP_PROJECT` 指定工程。无界面模式下只提供 Rust 侧工具
（文件、搜索、`run`、`test`、`check`、`docs_*`、`env_info`）；依赖编辑器窗口的
工具（`run_script`、`get_diagnostics`、`get_outline`、`read_buffer` 等）会明确
返回「需要 IDE 界面在线」。

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
| `run` | 一键运行：`mode` 选 `vm` / `aot`，`inputs` 直接喂给 `ask`，返回结构化结果（AOT 会报编译耗时/是否命中缓存） | Rust 内核 |
| `test` | 一键测试：自动找 `tests/**/*.co`、`test_*.co`、`*_test.co`，逐个跑，返回 `passed / failed` 与每个文件的报错 | Rust 内核 |
| `check` | 一键查 bug：传 `path` 查单个文件；不传扫全工程，按文件分组返回诊断 | Rust 内核 |
| `docs_list` | Copper 教程目录（17 章，含摘要与节数） | Rust |
| `docs_get` | 读某一章教程：讲解 + 示例代码 + 示例输出 + 注意点（Markdown） | Rust |
| `docs_search` | 教程全文搜索，返回章节/小节与上下文片段 | Rust |
| `env_info` | Copper 环境是否就绪、编译器路径与文档目录 | Rust |
| `get_diagnostics` | 对 `.co/.cw` 做语法与语义检查，**会先摊平 import**，诊断带「（在 X 里）」的来源标注 | 界面 → Rust 内核 |
| `get_outline` | 返回文件的函数 / 类 / 变量大纲 | 界面（TS AST） |

**两条运行路径，别混**：

- `run_script` 只传 `code` = 用 TS 兼容引擎跑一段孤立代码，适合试小片段；
- `run_script` 传 `path` = 和 **F5 完全一致**：Rust 内核编译执行，import 就地摊平、
  未存盘的缓冲区一起带上、报错行号映射回原文件。改多文件工程要用这条。
- `run` 读磁盘上的文件（不带未存盘缓冲区），但同一套参数可选 VM / AOT，最适合
  「改完 → 跑一遍 → 看真实结果」；AOT 首次 10~20 秒，之后按源码内容哈希命中缓存。

`ask` 是语句，答案在变量 `ans` 里：`ask "名字？"` 之后用 `ans`。`run` / `test` 的
`inputs` 会按顺序喂给这些 `ask`，喂完之后的 `ask` 直接拿空串，不会把调用卡死。

### 3.1 一键测试的约定

Copper 还没有 assert 内置，测试约定很简单：

- 文件放在工程里的 `tests/` 目录，或命名成 `test_*.co` / `*_test.co`；
- **跑完不报错 = 通过**，要断言失败就主动 `throw "原因"`；
- 失败会返回 `ok:false`、`error` 和 `error_line`（已映射回原文件）。

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call",
 "params":{"name":"test","arguments":{}}}
```

返回：

```json
{"total":3,"passed":2,"failed":1,"millis":412,
 "results":[{"file":"tests/a.co","ok":true,"millis":3,"output":"..."},
            {"file":"tests/b.co","ok":false,"error":"第 5 行：期望 42 实际 41","error_line":5}]}
```

### 3.2 让 AI 学会写这门语言

MCP 除了工具，还把 Copper 教程挂成了 **resources**：

| 资源 URI | 内容 |
| --- | --- |
| `copper://docs/tutorial` | 教程目录（JSON） |
| `copper://docs/about` | 关于 Copper（Markdown） |
| `copper://docs/env` | 环境信息（JSON） |
| `copper://docs/types`、`copper://docs/control`、`copper://docs/func` 等 | 每一章（Markdown） |

AI 的典型开场：

1. `env_info` —— 确认环境已装、AOT 能编；
2. `docs_list` —— 看有哪 17 章；
3. `docs_get {"chapter":"types"}` / `docs_search {"query":"指针"}` —— 学语法；
4. `run` / `test` / `check` —— 写完立刻验。

也可以直接在客户端里读资源（`resources/list` + `resources/read`），不占工具调用。

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
- 已实现方法：`initialize`、`tools/list`、`tools/call`、`resources/list`、`resources/read`、`prompts/list`、`ping`

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
