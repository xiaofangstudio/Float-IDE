# Copper 环境安装说明

这是 **Copper 环境**，不是 Float IDE。两者分开装：

1. **先装环境**（本包）——里面是 `colang x.x.xxx.exe` 编译器、自举编译器、运行器、文档。
2. **再装 Float IDE**——IDE 本身不含编译器，启动时按顺序找环境。

## Windows

双击或用 PowerShell 跑：

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

默认装到 `%APPDATA%\Float\copper`，装完 IDE 直接就能用。

可选参数：

```powershell
.\install.ps1 -Home "D:\Copper"        # 换目录
.\install.ps1 -AddToPath               # 同时把 bin\windows-x64 加进用户 PATH（命令行里能直接敲 colang）
.\install.ps1 -SetHomeVar              # 同时设 COPPER_HOME 用户环境变量
```

## Linux / macOS

```bash
./install.sh                 # 装到 ~/.copper
./install.sh /opt/copper     # 换目录
```

## 装完怎么确认

在 Float IDE：设置 → Copper 环境，应显示「已找到 · colang 1.9.2.exe」，并列出解释器路径。
找不到时：设置里「安装 Copper 环境（选 zip）」重装，或「查找 Copper…」手选 `colang x.x.xxx.exe`。

命令行验证（这个编译器是给 IDE 调用的，配合 `COLANG_TARGET` 用；直接敲 `--version` 不认）：

```powershell
$env:COLANG_TARGET = "你的文件.co"
& "$env:APPDATA\Float\copper\bin\windows-x64\colang 1.9.2.exe"
```

## 目录结构

```
copper_env/
  copper.json          # 环境清单（版本、平台、命名规则、文档位置）
  bin/
    windows-x64/       # colang 1.9.2.exe / colang-self 1.9.2.exe / copper-run.exe / copper-embed.exe
    linux-x64/         # 同上的 Linux 版
  lib/colang_rt.rs     # 运行时前奏（AOT 生成工程要用）
  lib/float-lang/      # 内核 crate 源码（crate 依赖；IDE 生成 AOT 工程要用）
  docs/                # tutorial.json（教程）、about-copper.md（关于 Copper）
```

## 命名规矩

- 解释器叫 **colang**，不叫 copper。
- 文件名格式 `colang <大>.<中>.<小>[.exe]`，例如 `colang 1.9.2.exe`。
- 同目录有多份时**优先版本号最大**的那份，其次比修改时间。
- `copper-run` / `copper-embed` 是运行器与嵌入库，不参与版本比较。
