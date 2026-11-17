#!/bin/sh
# Copper 环境启动器（macOS / Linux）
# 用法：
#   copper build <file.co> [-o 产物]
#   copper run   <file.co>
#   copper check|ast|json|stats <file.co>
#   copper env
#   copper self ...            强制用自举版编译器
#
# 平台二进制约定：bin/copper-<tag>[.exe]，tag = windows-x64 / macos-arm64 / linux-x64 ...
# 本脚本只挑本机的那个；找不到就明确报错，不悄悄换别的（跨平台铺垫靠这条规矩）。
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
COPPER_HOME=$(cd "$HERE/.." && pwd)
: "${COPPER_HOME:=}"
export COPPER_HOME

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Darwin) os_tag=macos ;;
  Linux)  os_tag=linux ;;
  *)      os_tag=$(echo "$os" | tr 'A-Z' 'a-z') ;;
esac
case "$arch" in
  x86_64|amd64) arch_tag=x64 ;;
  arm64|aarch64) arch_tag=arm64 ;;
  *)            arch_tag=$arch ;;
esac
TAG="$os_tag-$arch_tag"

pick() {
  # $1 = env|self
  if [ "$1" = "self" ]; then cand="$HERE/copper-self"; else cand="$HERE/copper"; fi
  if [ -x "$cand" ]; then echo "$cand"; return 0; fi
  if [ -x "$cand-$TAG" ]; then echo "$cand-$TAG"; return 0; fi
  return 1
}

KIND=env
if [ "${1:-}" = "self" ]; then KIND=self; shift; fi
if ! COPPER=$(pick "$KIND"); then
  if COPPER=$(pick env); then KIND=env; else
    echo "[copper] 找不到 $TAG 的编译器（期望 $HERE/copper-$TAG）" >&2
    echo "         在源码目录跑：cargo run --release --example aot -- bootstrap/colang.co -o copper_env/bin/copper-$TAG" >&2
    exit 2
  fi
fi

: "${COPPER_RT:=$COPPER_HOME/lib/colang_rt.rs}"
: "${COLANG_CRATE:=$COPPER_HOME/../scaffold/src-tauri/float-lang}"
: "${COLANG_CARGO:=$(command -v cargo || true)}"
export COPPER_RT COLANG_CRATE COLANG_CARGO

cmd=${1:-}
[ -n "$cmd" ] || { echo "用法：copper [self] build|run|check|ast|json|stats|env <file.co>" >&2; exit 2; }

need_target() {
  [ -n "${2:-}" ] || { echo "[copper] 缺少源文件" >&2; exit 2; }
  COLANG_TARGET=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
  [ -f "$COLANG_TARGET" ] || { echo "[copper] 找不到源文件：$COLANG_TARGET" >&2; exit 2; }
  export COLANG_TARGET
}

case "$cmd" in
  env)
    echo "COPPER_HOME = $COPPER_HOME"
    echo "compiler    = $COPPER ($KIND)"
    echo "platform    = $TAG"
    echo "runtime     = $COPPER_RT"
    echo "crate       = $COLANG_CRATE"
    echo "cargo       = ${COLANG_CARGO:-<未找到>}"
    [ -f "$COPPER_HOME/copper.json" ] && cat "$COPPER_HOME/copper.json"
    ;;
  check|ast|json|stats)
    need_target "$cmd" "${2:-}"
    COLANG_MODE=$cmd; export COLANG_MODE
    exec "$COPPER"
    ;;
  build)
    need_target build "${2:-}"
    : "${COLANG_OUT:=${TMPDIR:-/tmp}/copper_build}"
    export COLANG_OUT COLANG_MODE=build
    "$COPPER"
    [ -x "$COLANG_OUT/target/release/app" ] || exit 1
    if [ "${3:-}" = "-o" ] && [ -n "${4:-}" ]; then
      cp "$COLANG_OUT/target/release/app" "$4"
      echo "[copper] 产物：$4"
    else
      echo "[copper] 产物：$COLANG_OUT/target/release/app"
    fi
    ;;
  run)
    need_target run "${2:-}"
    : "${COLANG_OUT:=${TMPDIR:-/tmp}/copper_build}"
    export COLANG_OUT COLANG_MODE=build
    "$COPPER"
    [ -x "$COLANG_OUT/target/release/app" ] || exit 1
    exec "$COLANG_OUT/target/release/app"
    ;;
  *)
    echo "用法：copper [self] build|run|check|ast|json|stats|env <file.co>" >&2
    exit 2
    ;;
esac
