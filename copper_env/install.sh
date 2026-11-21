#!/usr/bin/env sh
# 把 Copper 环境铺到用户目录（默认 ~/.copper），并安装 `co` 命令。
#
#   ./install.sh
#   ./install.sh /opt/copper
#
# 安装后：
#   co --version
#   co            # 进交互模式
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -f "$here/copper_env/copper.json" ]; then
  src="$here/copper_env"
elif [ -f "$here/copper.json" ]; then
  src="$here"
else
  echo "这个包里没找到 copper_env/copper.json，不是有效的 Copper 环境包。" >&2
  exit 1
fi

dest=${1:-"$HOME/.copper"}
echo "安装到：$dest"
mkdir -p "$dest"
cp -R "$src/." "$dest/"

case "$(uname -s)" in
  Darwin) plat="macos-arm64" ;;
  Linux)  plat="linux-x64" ;;
  *)      plat="unknown" ;;
esac

bin="$dest/bin/$plat"
[ -d "$bin" ] || bin="$dest/bin"

# 启动器与运行器需要可执行位（zip 解压后经常丢）
chmod +x "$dest/bin/co.sh" "$dest/bin/copper.sh" 2>/dev/null || true
for f in "$bin"/*; do
  [ -f "$f" ] && chmod +x "$f" 2>/dev/null || true
done

compiler=$(ls "$bin" 2>/dev/null | grep -E '^colang [0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n 1 || true)
if [ -z "$compiler" ]; then
  echo "警告：$bin 下没找到当前平台（$plat）的 colang 编译器。" >&2
  echo "      IDE 仍可用内核 AOT；但命令行 co build 不可用。" >&2
else
  echo "编译器：$bin/$compiler"
fi

# `co` 命令：放到 ~/.local/bin，写成绝对路径的 wrapper（比软链更稳，不依赖 readlink -f）
launcher_dir="$HOME/.local/bin"
mkdir -p "$launcher_dir"
cat > "$launcher_dir/co" <<EOF
#!/usr/bin/env sh
exec "$dest/bin/co.sh" "\$@"
EOF
chmod +x "$launcher_dir/co"

# 让新开的 shell 能找到 co，并让 IDE 找到自定义目录
profile="$HOME/.profile"
path_line='export PATH="$HOME/.local/bin:$PATH"'
home_line="export COPPER_HOME=\"$dest\""
if [ ! -f "$profile" ] || ! grep -Fq "$path_line" "$profile" 2>/dev/null; then
  printf '\n%s\n' "$path_line" >> "$profile"
fi
if [ "$dest" != "$HOME/.copper" ]; then
  if [ ! -f "$profile" ] || ! grep -Fq "$home_line" "$profile" 2>/dev/null; then
    printf '%s\n' "$home_line" >> "$profile"
  fi
fi

echo
echo "装好了。新开一个终端，验证："
echo "  co --version"
echo "  co"
echo "如果当前终端就要用： export PATH=\"\$HOME/.local/bin:\$PATH\""
echo "然后打开 Float IDE：设置 → Copper 环境，应显示「已找到」。"
