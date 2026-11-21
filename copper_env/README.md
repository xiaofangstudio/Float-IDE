# Copper 环境安装说明

这是 **Copper 环境**，不是 Float IDE。两者分开装：

1. **先装环境**（本包）——里面是 `colang x.x.xxx` 编译器、自举编译器、运行器、`co` 命令、文档。
2. **再装 Float IDE**——IDE 本身不含编译器，启动时按顺序找环境。

## Windows

推荐直接运行 `Copper-Env-1.9.2-Setup.exe`：装到 `%APPDATA%\Float\copper`，自动写好 `PATH` 与 `COPPER_HOME`，装完新开终端敲 `co` 即可。

也可以解压 zip 后运行脚本：

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

可选参数：

```powershell
.\install.ps1 -Dest "D:\Copper"    # 换目录（旧写法 -Home 也兼容）
.\install.ps1 -NoPath              # 不改进 PATH，只铺文件
```

默认安装会同时把 `bin\`（`co.cmd`）和 `bin\windows-x64\`（`colang*.exe`）加进用户 PATH，并设置 `COPPER_HOME`。

## Linux / macOS

```bash
./install.sh                 # 装到 ~/.copper
./install.sh /opt/copper     # 换目录
```

脚本会装 `co` 到 `~/.local/bin`，并在 `~/.profile` 里补 `PATH`；自定义目录会额外写入 `COPPER_HOME`。

## 装完怎么确认

```powershell
co --version
co            # 不带参数进交互模式
co help       # 完整手册，按 1-9 选章节
```

REPL 带语法高亮、历史命令和多行重绘；不想要颜色时设 `NO_COLOR=1`。

Float IDE：设置 → Copper 环境，应显示「已找到 · colang 1.9.2.exe」，并列出解释器路径。
找不到时：设置里「安装 Copper 环境（选 zip）」重装，或「查找 Copper…」手选 `colang x.x.xxx.exe`。

> 换电脑后 `co` 不生效，通常就是只把 zip 解压了、没跑安装脚本：`co.cmd` 在 `bin\` 根目录，必须把这个目录加进 PATH。

## 目录结构

```
copper_env/
  copper.json          # 环境清单（版本、平台、命名规则、文档位置）
  bin/
    co.cmd / co.sh     # 命令行启动器（co）
    copper.cmd/.sh     # 完整子命令（build/check/ast/...）
    windows-x64/       # colang 1.9.2.exe / colang-self 1.9.2.exe / copper-run.exe
    linux-x64/         # 同上的 Linux 版
    macos-arm64/       # 预留；当前包没有 mac 二进制
  lib/colang_rt.rs     # 运行时前奏（AOT 生成工程要用）
  lib/float-lang/      # 内核 crate 源码（IDE 生成 AOT 工程要用）
  docs/                # tutorial.json（教程）、about-copper.md（关于 Copper）
```

## 命名规矩

- 解释器叫 **colang**，不叫 copper。
- 文件名格式 `colang <大>.<中>.<小>[.exe]`，例如 `colang 1.9.2.exe`。
- 同目录有多份时**优先版本号最大**的那份，其次比修改时间。
- `copper-run` / `copper-embed` 是运行器与嵌入库，不参与版本比较。
