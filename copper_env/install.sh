#!/usr/bin/env sh
# 把 Copper 环境铺到用户目录（默认 ~/.copper），装完 Float IDE 就能找到。
#   ./install.sh
#   ./install.sh /opt/copper
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
src="$here/copper_env"
if [ ! -f "$src/copper.json" ]; then
  echo "这个包里没找到 copper_env/copper.json，不是有效的 Copper 环境包。" >&2
  exit 1
fi

dest=${1:-"$HOME/.copper"}
echo "安装到：$dest"
mkdir -p "$dest"
cp -R "$src/." "$dest/"

plat="linux-x64"
[ "$(uname -s)" = "Darwin" ] && plat="macos-arm64"
bin="$dest/bin/$plat"
[ -d "$bin" ] || bin="$dest/bin"

compiler=$(ls "$bin" 2>/dev/null | grep -E '^colang [0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n 1 || true)
if [ -z "$compiler" ]; then
  echo "装完了但 $bin 下没找到 colang x.x.xxx，可能这个包没带当前平台的二进制。" >&2
  exit 1
fi
chmod +x "$bin/$compiler" || true
echo "编译器：$bin/$compiler"
echo
echo "装好了。验证："
echo "  \"$bin/$compiler\" --version"
echo "然后打开 Float IDE：设置 → Copper 环境，应显示「已找到」。"
