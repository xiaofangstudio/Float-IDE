# Linux 重编：上云主机前要准备的一切

本文件说明：**要 push 什么**、**云主机上跑什么**、**编完拿回什么**。

## 一、这次为什么 Linux 要重编

`copper_env/bin/linux-x64/` 里的四个二进制还是**上一代** AOT 产物（没含本轮作用域 / 指针 / finally 修复）。
AOT 的代码生成改在 `aot.rs` 里，Windows 与 Linux 必须各自用本机 Rust 重编一次，"同源码跨平台现编"是唯一可靠路径（不能拿 Windows 的 exe 改名充数）。

## 二、要 push 的东西

本轮改动里有几个**未跟踪**文件（`??`），`git add` 时必须带上，否则云主机那边会缺件：

```bash
# 精确添加，别用 git add -A（会带上 280MB 自举中间目录——已在 .gitignore 里堵住，但精确更稳）
git add copper_env/scripts/
git add copper_env/bin/co.sh copper_env/bin/co.cmd
git add copper_env/bin/windows-x64/copper-run.exe copper_env/bin/windows-x64/copper-embed.exe
git add copper_env/bin/linux-x64/
git add scaffold/src-tauri/float-lang/examples/embed.rs
git add scaffold/src-tauri/float-lang/examples/run.rs
git add scaffold/src-tauri/float-lang/tests/ide_f5_game.rs
git add scaffold/src/ide/MdPreview.tsx
git add scaffold/tests/fin.ts scaffold/tests/runmain.ts scaffold/tests/parity/scope.txt
git add tests/
git add .gitignore
git add -u          # 其余已跟踪文件的本轮修改（aot.rs / parser.rs / bytecode.rs / colang.co / copper.json …）

# Linux 二进制的执行位（Windows 上 git 默认记 644，不加这行，云主机 pull 下来后 copper.sh 会判"不可执行"）
git update-index --chmod=+x copper_env/bin/linux-x64/copper copper_env/bin/linux-x64/copper-self copper_env/bin/linux-x64/copper-run copper_env/bin/linux-x64/copper-embed
```

**必须入库**（少了任何一条，Linux 侧不是编不出就是编出来是旧的）：

| 文件 | 少了会怎样 |
|---|---|
| `scaffold/src-tauri/float-lang/src/*.rs` | 用旧 AOT，本轮作用域/指针/finally 修复全丢 |
| `scaffold/src-tauri/float-lang/examples/{aot,run,embed}.rs` | `embed` 未跟踪过，**最容易漏**；缺了 `copper-run`/`copper-embed` 不出来（脚本会提示跳过 embed） |
| `scaffold/src-tauri/float-lang/bootstrap/colang.co` | 编出来的编译器是旧语义 |
| `copper_env/lib/colang_rt.rs` | 自举构建时找不到运行时前奏，直接退 2 |
| `copper_env/copper.json`、`bin/copper.sh` | 启动器找不到 `bin/linux-x64/copper` |
| `copper_env/scripts/rebuild-linux.sh` | 云主机没脚本，只能手敲 |
| `.gitignore` | 不带也行，但带上能挡住 280MB 误提交 |

**不要 push**：`tools/`（整目录被忽略，约 1.9G 运行时）、`scaffold/dist/`、`*/target/`、`bootstrap/*.exe`、`scaffold/src-tauri/float-lang/_selfhost|_bench|_cls|_fuzz|_scope|_silent|_stress|_backup_*|_verified_*`（合计约 280MB，本轮已加进 `.gitignore`）。

> `.gitignore` 里有整行 `tools/`，所以脚本**故意放在 `copper_env/scripts/`**，放在 `tools/` 下会被忽略、push 不上去。

## 三、云主机上怎么跑

```bash
# 1) 拉最新代码（仓库根就是含 copper_env/ 的那一层）
git pull

# 2) 给脚本可执行权限（Windows 上新建的文件默认没有 +x）
chmod +x copper_env/scripts/rebuild-linux.sh

# 3) 跑全流程（约 8–12 分钟，其中两次 colang.co 的 LTO 构建各 2–4 分钟）
bash copper_env/scripts/rebuild-linux.sh

# 着急只要产物、不复验不动点：
bash copper_env/scripts/rebuild-linux.sh --fast
```

脚本会做六步：

1. `cargo build --release --examples`（float-lang **零外部依赖**，离线可编）
2. 用 `examples/aot` 编 `bootstrap/colang.co` → `copper_env/bin/linux-x64/copper`
3. 用这个 `copper` 以 `COLANG_MODE=build` 编自己 → `copper-self`
4. `examples/run` → `copper-run`，`examples/embed` → `copper-embed`，全部 `chmod +x`
5. 冒烟：`copper` 不带参数必须打印"没有给目标文件…"并退 1
6. 不动点：`copper-self` 再编自己，两代 `src/main.rs` 的 SHA256 必须相同（`--fast` 跳过）

脚本最后会打印四个文件的**字节数 + SHA256**，这就是要回传的数据。

### 这套流程已经在云主机上预检过了（2026-11-19）

还没等 push，我先在云主机（`/home/user/Doubao/chats/38454210268978946/float-ide`）用**现有的旧源码**把整条链跑穿，确认 Linux 侧没有平台坑：

| 预检项 | 结果 |
|---|---|
| `cargo build --release --examples` | 通过（Linux，Rust 1.100.0） |
| AOT 编 `colang.co` | **74 s** → `copper` 4,119,888 B |
| `copper` 自举编自己 | **77 s** → `app` 3,892,184 B |
| `copper-self` 再编自己 | **257 s**（含 LTO） |
| 两代 `src/main.rs` | **完全相同** `2b1b979c828dd74638b2ff75ef879ffbbb6557095865115feb42757800f48457` |

所以脚本不是"理论上能用"，是**在目标机器上真跑通过**；换成新源码后，两代 `main.rs` 的哈希应当从 `2b1b979c…` 变成 Windows 这代的 `E33AFF98…`（跨平台编译器输出一致）。


## 四、可能需要现场处理的两处（脚本会明确报错，不会悄悄过）

- **缺 Rust**：脚本直接退出并给安装命令（`curl https://sh.rustup.rs -sSf | sh -s -- -y`），装完重跑即可。
- **AOT 在 Linux 上遇到的平台相关代码**：若第 2 步失败，先 `COPPER_AOT_VERBOSE=1` 重跑看 rustc 诊断，生成物在 `%TMP%/copper_aot_*/src/main.rs`（Linux 下是 `/tmp/copper_aot_*/src/main.rs`），按报错补 `#[cfg(unix)]` 分支——这属于源码修复，改完要同步回 Windows。

## 五、编完怎么拿回 Windows

1. 云主机上把四个文件取回：`copper_env/bin/linux-x64/{copper,copper-self,copper-run,copper-embed}`
2. 覆盖到本机 `D:\20533\deskbox\fangwork\Float\copper_env\bin\linux-x64\`
3. 本机校验：`wsl`/容器里跑 `copper_env/bin/copper.sh env` 应显示 `platform = linux-x64`；或至少确认四份文件非空、`chmod` 已带（git 不记录权限位时，用 `copper.sh` 里的 `-x` 判断会失败，可在 `copper.sh` 前加 `chmod +x` 一行兜底）
4. `git status` 会看到这四个文件被改（或被新增），提交时一并带上

## 六、参考：Windows 这一代实测数字（对照用）

| 项目 | 数值 |
|---|---|
| AOT 编 `colang.co` | 63.6 s → 3,823,104 B |
| 自举 g15（copper 编自己） | 70 s，`main.rs` 714,971 B |
| 自举 g16（copper-self 再编） | 207 s，`main.rs` 714,971 B |
| 两代 `main.rs` SHA256 | `E33AFF98A4AB7D756C4325E58EB9B2AB12B3E635EEF65F8F294DB9917950BCA1`（相同 = 不动点） |
| Windows `copper.exe` SHA256 | `33A9AC0F354B0BCD4FB568C4759C374C4FDD5497FFEC1C289072B4AB65CF8F3F` |

Linux 的哈希必然与 Windows 不同（不同平台产物），**但 `main.rs` 的 SHA256 应当也等于上面那串**——因为 `main.rs` 是编译器输出，与平台无关。若 Linux 的 g1/g2 `main.rs` 哈希也等于 `E33AFF98…`，说明两边编译器行为完全一致，这是最强的跨平台一致性证据。
