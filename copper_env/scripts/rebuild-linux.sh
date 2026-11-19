#!/usr/bin/env bash
# Copper · Linux 重编脚本（在云主机 / 任何 Linux 机器上跑）
# ------------------------------------------------------------------
# 作用：从仓库源码重编出这一代 Linux 四个二进制，放进 copper_env/bin/linux-x64/：
#   copper        ← Rust AOT 编出来的环境编译器（就是 colang.co 的编译产物）
#   copper-self   ← 用上面这个 copper 自举编出来的自举版编译器
#   copper-run    ← examples/run（解释执行 + F5 那条链）
#   copper-embed  ← examples/embed（嵌入 API）
#
# 顺带证一遍自举不动点：copper 编自己 → main.rs(A)；copper-self 再编自己 → main.rs(B)；
# A 与 B 的 SHA256 必须完全相同。
#
# 用法：
#   bash copper_env/scripts/rebuild-linux.sh          # 全流程（约 8–12 分钟）
#   bash copper_env/scripts/rebuild-linux.sh --fast   # 跳过不动点复验（约 5 分钟）
#
# 依赖：Rust（cargo 在 PATH 或 ~/.cargo/bin）。float-lang 零外部 crate，离线也能编。
# ------------------------------------------------------------------
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
FL="$ROOT/scaffold/src-tauri/float-lang"
OUT="$ROOT/copper_env/bin/linux-x64"
RT="$ROOT/copper_env/lib/colang_rt.rs"
CO="$FL/bootstrap/colang.co"
TMP="${TMPDIR:-/tmp}/copper_linux_rebuild"
FAST=0
if [ "${1:-}" = "--fast" ]; then FAST=1; fi

step() { printf '\n== %s\n' "$1"; }
die() { printf '\n[失败] %s\n' "$1" >&2; exit 1; }

export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || die "没找到 cargo。先装 Rust：curl https://sh.rustup.rs -sSf | sh -s -- -y"
[ -f "$CO" ] || die "找不到 colang.co：$CO"
[ -f "$RT" ] || die "找不到运行时前奏：$RT"
mkdir -p "$OUT" "$TMP"

echo "仓库根：$ROOT"
echo "Rust   ：$(cargo --version)"
echo "目标   ：$OUT"

step "1/6 编 Rust 例程（aot / run / embed）"
cd "$FL"
cargo build --release --examples || die "cargo build 失败"
[ -x "$FL/target/release/examples/aot" ] || die "没编出 examples/aot"
[ -x "$FL/target/release/examples/run" ] || die "没编出 examples/run"

step "2/6 用 AOT 编译 colang.co → copper"
COPPER_PARSE_TARGET="$CO" \
COPPER_AOT_MAX_LINES=400000 \
COPPER_AOT_MAX_BYTES=40000000 \
  "$FL/target/release/examples/aot" "$CO" -o "$OUT/copper" || die "AOT 编 colang.co 失败"
[ -s "$OUT/copper" ] || die "copper 产物是空的"
# aot 写出来的文件不一定带执行位，下面第 3 步要直接跑它
chmod +x "$OUT/copper"

step "3/6 用 copper 自举 → copper-self"
rm -rf "$TMP/self"
COLANG_TARGET="$CO" \
COLANG_MODE=build \
COLANG_OUT="$TMP/self" \
COLANG_CRATE="$FL" \
COPPER_RT="$RT" \
COLANG_CARGO="$(command -v cargo)" \
  "$OUT/copper" || die "copper 自举构建失败"
[ -x "$TMP/self/target/release/app" ] || die "自举产物 app 不存在"
cp "$TMP/self/target/release/app" "$OUT/copper-self"

step "4/6 放 run / embed"
cp "$FL/target/release/examples/run" "$OUT/copper-run"
# embed 在部分检出里可能还不存在（它是后来加的例程）——没有就明说，别硬拷贝
if [ -x "$FL/target/release/examples/embed" ]; then
  cp "$FL/target/release/examples/embed" "$OUT/copper-embed"
  chmod +x "$OUT/copper" "$OUT/copper-self" "$OUT/copper-run" "$OUT/copper-embed"
else
  echo "注意：examples/embed.rs 不在这次检出里，跳过 copper-embed"
  chmod +x "$OUT/copper" "$OUT/copper-self" "$OUT/copper-run"
fi

step "5/6 冒烟（无目标文件应报提示并退 1）"
set +e
"$OUT/copper" > "$TMP/smoke.txt" 2>&1
rc=$?
set -e
head -n 3 "$TMP/smoke.txt"
[ "$rc" -eq 1 ] || die "copper 无参数时退出码应为 1，实际 $rc"

if [ "$FAST" -eq 0 ]; then
  step "6/6 自举不动点：copper-self 再编自己，比 main.rs 哈希"
  rm -rf "$TMP/self2"
  COLANG_TARGET="$CO" \
  COLANG_MODE=build \
  COLANG_OUT="$TMP/self2" \
  COLANG_CRATE="$FL" \
  COPPER_RT="$RT" \
  COLANG_CARGO="$(command -v cargo)" \
    "$OUT/copper-self" || die "copper-self 再自举失败"
  A=$(sha256sum "$TMP/self/src/main.rs" | cut -d' ' -f1)
  B=$(sha256sum "$TMP/self2/src/main.rs" | cut -d' ' -f1)
  echo "g1 main.rs = $A"
  echo "g2 main.rs = $B"
  [ "$A" = "$B" ] || die "不动点不成立：两代 main.rs 不一致"
  echo "不动点成立 ✓"
fi

step "结果"
for f in copper copper-self copper-run copper-embed; do
  if [ -f "$OUT/$f" ]; then
    printf '%-14s %10s 字节  %s\n' "$f" "$(stat -c %s "$OUT/$f")" "$(sha256sum "$OUT/$f" | cut -d' ' -f1)"
  else
    printf '%-14s   这次没生成\n' "$f"
  fi
done
echo
echo "对照：Windows 这一代 colang.co 的 g1/g2 main.rs SHA256 是"
echo "  FEA5DA9414B60A626BE9BCA6D4BF62CA6B409CC5CA8C72B9D9A66EBC3A4B9A36"
echo "Linux 的 main.rs 哈希应当完全等于它（编译产物与平台无关）；不等就说明语义走岔了。"

cat <<'TIP'

下一步（回到 Windows 电脑后要做的事）：
  1. 把这四个文件拷回 Windows 的 copper_env/bin/linux-x64/（同名覆盖）
  2. copper_env/copper.json 里 linux-x64 的 status 已经是 ready，无需改
  3. 提交前给二进制补执行位（Windows 上 git 只记 644）：
       git update-index --chmod=+x copper_env/bin/linux-x64/copper \
                                 copper_env/bin/linux-x64/copper-self \
                                 copper_env/bin/linux-x64/copper-run \
                                 copper_env/bin/linux-x64/copper-embed
  4. 用 `git status` 看一眼，提交这四个二进制
TIP
