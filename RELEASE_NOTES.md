# Float IDE 1.9.0

Copper 语言（原 CW Script）的写作与调试环境。从 1.9.0 起 Copper 独立成仓，
IDE 通过 Copper 环境调用编译器，两个产品分开发版。

## 下载

| 系统 | 文件 |
|---|---|
| Windows x64 | `Float IDE_1.9.0_x64-setup.exe` |
| macOS | `Float IDE_1.9.0_aarch64.dmg` |
| Linux x64 | `Float IDE_1.9.0_amd64.AppImage` / `Float IDE_1.9.0_amd64.deb` |

Copper 编译器单独下载：<https://github.com/xiaofangstudio/Copper/releases>

## 本版要点

- **Copper 环境接入**：IDE 不再内置编译器，改为读 `COPPER_HOME` → `%APPDATA%\Float\copper` → 同级 `copper_env/`；支持手动指定与 zip 一键安装。
- **AOT 编译**：一键把脚本编成原生程序（整型/浮点特化、百万层递归、指针、嵌套函数、`ask`）。
- **交互模式**：`--repl` 里变量与函数跨行保留，表达式自动打印结果。
- **跨平台内置函数**：`exists` / `isfile` / `isdir` / `filesize` / `getenv` / `pathjoin` / `runcmd` / `osname` / `strfind` / `writefile`。
- **整数精度**：超出 `i64` 的字面量在解析期报错，不再悄悄变成浮点。

## 验证

`cargo test` 371 项、前端 `test:lang` 全过，`tsc --noEmit` 无报错。
