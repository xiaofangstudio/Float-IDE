# Float IDE 1.9.8

Copper 语言（原 CW Script）的写作与调试环境。从 1.9.0 起 Copper 独立成仓，
IDE 通过 Copper 环境调用编译器；1.9.8 起 IDE 的 AOT 与「导出 exe」改为进程内 emitter，
不再依赖环境里那份自举版 `colang` 的能力面。

## 下载

| 系统 | 文件 |
|---|---|
| Windows x64 | `Float IDE_1.9.8_x64-setup.exe` |
| macOS | 未打包（需 macOS runner） |
| Linux x64 | `Float IDE_1.9.8_amd64.deb` / `.AppImage` |

Copper 环境单独下载：`Copper-Env-1.9.2-Setup.exe` / `.zip`（Windows）、`Copper-Env-1.9.2-linux.tar.gz`（Linux，含 `install.sh`）。

| 文件 | 字节 | SHA256 |
|---|---:|---|
| `Float IDE_1.9.8_x64-setup.exe` | 12,545,999 | `46A6DDC199FC670796A0181A5ED0F0E5FCD56BB164954D6BEC7D5F0C45B1EC4D` |
| `Copper-Env-1.9.2-Setup.exe` | 7,225,179 | `A48FF51746E9C96749FD347205594DEA36C35B92399FD65E609AADB6C48BF998` |
| `Copper-Env-1.9.2.zip` | 9,819,628 | `59254CFF4549A65EA68906B472AF2A8D4CF4946EB8C30778F5C80BD60CFF1573` |
| `Float IDE_1.9.8_amd64.deb` | 10,883,964 | `92DD3C45DF9BC2083DFD4D93E17CCA4D6FF763AF21979745BE414AEB97F02F65` |
| `Float IDE_1.9.8_amd64.AppImage` | 88,840,696 | `B4E6DA82ADADF9DCE21139AE847B6921BB87EDAC48DBBFEE460FD3C0992230E7` |
| `Copper-Env-1.9.2-linux.tar.gz` | 3,718,834 | `5CEE32DB836DF29D15C04EF37247C46C735F04B278E34F3CFB9A21D1F0416ED2` |

## 本版要点

- **运行完全不设上限**：递归深度、调用深度、import 层数、表达式/块嵌套、列表/字典元素数、字符串长度、输出字节数、执行步数全部取消；AOT 生成代码的体积闸门也去掉。计算跑多久就多久，内存不够由系统处理，语言层不再主动中止。
- **任意精度整数**：整数 `+ - * **` 溢出后不再退化成 f64，也不会出现 `Infinity`；`2 ** 100000000` 会硬算出完整的 30,103,000 位十进制结果（本机实测 169 秒），并与 Python 的结果逐位一致。实现不设位数上限，小规模走 schoolbook，大规模走 Karatsuba。
- **CLI 语法高亮与完整帮助**：`co` 交互模式边输入边着色（关键字/类型/内建函数/字符串/数字/注释/运算符），支持方向键、Home/End、历史、Ctrl+A/E/U/K/W/L；`co help` 打开 9 章完整手册，按 1-9 阅读，`a` 一次看全部。
- **AOT 运行不再弹黑框**：IDE 的 F5 AOT 和 MCP `run mode=aot` 启动产物时加 `CREATE_NO_WINDOW`，AOT 的 cargo/rustc 编译子进程也加同样标志；Windows 不再新建控制台黑窗，`ask` 仍走 stdin/stderr 管道由 IDE 输入框接收答案。
- **REPL 比较表达式回显修复**：`a == 1`、`a != 1`、`a <= 1`、`a >= 1` 现在按表达式执行并回显 `true` / `false`；`=`、`+=`、`-=` 等仍按赋值语句执行。
- **REPL 粘行修复**：超长数字输出时提示符强制回到行首；输入行会清掉 ANSI 控制序列、BOM/零宽字符、裸 CR 和误入的 `>>>`；`print(...)` 后只剩残留数字或多余右括号时按有效前缀执行，不再整行报「print 后面多了内容」。
- **环境物理拆分**：IDE 安装包里只剩 `floatide.exe` + `uninstall.exe`；环境单独装到 `%APPDATA%\Float\copper`。
- **AOT 双路统一**：F5 的 AOT 与「导出 exe」都走 `float_lang::aot::build_exe`（进程内），VM 支持的能力（`wait until`、`time read`、嵌套函数、指针、`ask`）导出后同样能跑。
- **F5 三种口径**：VM / AOT / 解释器，设置里可切默认；AOT 首次编译约 2~3 秒，之后按源码哈希缓存。
- **MCP 扩到 27 个工具**：新增 `run`（VM/AOT + `timeoutMs`）、`stop`、`test`、`check`、`docs_list` / `docs_get` / `docs_search`、`env_info`；教程挂成 `resources/*`；支持无界面 `floatide --mcp <port> --project <dir>`。
- **AOT 整型特化收紧**：嵌套函数里调用整型辅助函数时，推断阶段会把不安全的特化摘掉回落 V 世界，修掉「整型函数调用非整型函数」的编译失败。

## 验证

- `cargo test --release` 347 passed / 0 failed；`tsc --noEmit` 与 `npm run test:lang` 通过。
- 无限制实测：2.5 亿次循环 7.79 秒、120 万元素列表、20 MB 字符串、40 MB 输出全部通过；AOT 编 2.5 亿次循环同样输出 `250000000`。
- F5 AOT 本机 GUI 验收：嵌套函数 + `time reset` / `time read` → 输出 `18` / `true`，编译 2.3 秒、总 2.6 秒。
- MCP 无界面 E2E：VM 超时 1.24 秒返回、AOT 死循环 1.51 秒被杀且无残留、并发 `stop` 返回 `aot_killed:true`。

## 历史版本

### Float IDE 1.9.0

- **Copper 环境接入**：IDE 不再内置编译器，改为读 `COPPER_HOME` → `%APPDATA%\Float\copper` → 同级 `copper_env/`；支持手动指定与 zip 一键安装。
- **AOT 编译**：一键把脚本编成原生程序（整型/浮点特化、百万层递归、指针、嵌套函数、`ask`）。
- **交互模式**：`--repl` 里变量与函数跨行保留，表达式自动打印结果。
- **跨平台内置函数**：`exists` / `isfile` / `isdir` / `filesize` / `getenv` / `pathjoin` / `runcmd` / `osname` / `strfind` / `writefile`。
- **整数精度**：超出 `i64` 的字面量在解析期报错，不再悄悄变成浮点。
- 验证：`cargo test` 371 项、前端 `test:lang` 全过。
