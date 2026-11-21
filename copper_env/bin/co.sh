#!/usr/bin/env bash
# Copper 启动器（Linux/macOS）：和 python 一样，敲 co 就出来
#   co                    版本 + 用法
#   co --version          只打版本
#   co <脚本.co> [输入]    用 COVM 解释器直接跑
#   co build <脚本.co> [-o 产物]        用 CO AOT 编成原生 exe
#   co check|ast|json|stats <脚本.co>   静态检查 / AST / JSON / 统计
#   co env                环境详情
#   co self <子命令>       强制用自举版编译器
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
COPPER_HOME="$(cd "$HERE/.." && pwd)"

case "$(uname -s)" in
  Linux*)  PLAT="linux-x64" ;;
  Darwin*) PLAT="macos-arm64" ;;
  *)       PLAT="unknown" ;;
esac

pick() {  # pick <平台路径> <平铺兼容路径>
  if [ -x "$HERE/$PLAT/$1" ]; then echo "$HERE/$PLAT/$1"; else echo "$HERE/$2"; fi
}

# 编译器统一命名 `colang <版本>.exe`（Linux 无 .exe 后缀）：同目录多份取版本最大的
sort_versions() {  # GNU sort -V on Linux; portable numeric fallback on macOS (BSD sort has no -V)
  if sort -V </dev/null >/dev/null 2>&1; then
    sort -V
  else
    awk '{v=$0; sub(/^.* /,"",v); split(v,a,"."); printf "%010d %010d %010d %s\n", a[1]+0,a[2]+0,a[3]+0,$0}' | sort | cut -d" " -f4-
  fi
}

newest() {  # newest <前缀 glob> <排除词> <兜底>
  local hit
  hit="$(ls -1 "$HERE/$PLAT/${1}"* 2>/dev/null | grep -v -- "$2" | sort_versions | tail -1 || true)"
  [ -n "$hit" ] || hit="$(ls -1 "$HERE/${1}"* 2>/dev/null | grep -v -- "$2" | sort_versions | tail -1 || true)"
  [ -n "$hit" ] || hit="$(pick "$3" "$3")"
  echo "$hit"
}

EXE="$(newest 'colang ' '-self' 'copper')"
SELF="$(newest 'colang-self ' '-run' 'copper-self')"
RUN="$(pick copper-run copper-run)"
RT="$COPPER_HOME/lib/colang_rt.rs"

VER="1.0.0"
if [ -f "$COPPER_HOME/copper.json" ]; then
  V="$(grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$COPPER_HOME/copper.json" | head -1 | sed 's/.*"\([^"]*\)"$/\1/')"
  [ -n "${V:-}" ] && VER="$V"
fi

cmd="${1:-}"
case "$cmd" in
  "")
    echo "Copper $VER  ($PLAT / COVM + CO AOT)"
    echo
    echo "用法:"
    echo "  co 脚本.co [输入文件]              用 COVM 解释器直接跑"
    echo "  co run 脚本.co                     同上"
    echo "  co build 脚本.co [-o 产物]         用 CO AOT 编成原生 exe"
    echo "  co check 脚本.co                   静态检查（不执行）"
    echo "  co ast / json / stats 脚本.co      AST / JSON / 统计"
    echo "  co env                             环境详情"
    echo "  co self 子命令                      强制用自举版编译器"
    echo "  co --version                       只打版本"
    echo "  co help                            完整手册（1-9 章）"
    # 跟 Windows 的 co.cmd 一致：不给参数就进交互模式（像 python 那样）
    [ -x "$RUN" ] && exec "$RUN" --repl
    exit 0
    ;;
  --version|-v|-V|version)
    echo "Copper $VER  ($PLAT / COVM + CO AOT)"
    echo "  interpreter : $RUN"
    echo "  compiler    : $EXE"
    echo "  self-host   : $SELF"
    echo "  runtime     : $RT"
    echo "  home        : $COPPER_HOME"
    exit 0
    ;;
  help)
    exec "$RUN" --help
    ;;
  -h|--help)
    exec "$HERE/copper.sh" --help 2>/dev/null || { echo "co <脚本.co> | co build | co check | co env"; exit 0; }
    ;;
  env)
    exec "$HERE/copper.sh" env
    ;;
  build|check|ast|json|stats|self)
    shift
    exec "$HERE/copper.sh" "$cmd" "$@"
    ;;
  run)
    shift
    [ $# -ge 1 ] || { echo "[co] 用法：co run 脚本.co" >&2; exit 2; }
    SCRIPT="$1"; shift
    [ -f "$SCRIPT" ] || { echo "[co] 找不到脚本：$SCRIPT" >&2; exit 2; }
    COPPER_RT="$RT" exec "$RUN" "$SCRIPT" "${1:-}"
    ;;
  *)
    [ -f "$cmd" ] || { echo "[co] 找不到脚本：$cmd" >&2; exit 2; }
    SCRIPT="$cmd"; shift || true
    COPPER_RT="$RT" exec "$RUN" "$SCRIPT" "${1:-}"
    ;;
esac
