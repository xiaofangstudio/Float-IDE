#!/usr/bin/env bash
# 全量双引擎对拍（cases*.txt 一次跑完）：任何一组不一致就整体失败。
# 用法：bash tests/parity-all.sh        或  npm run test:parity
set -u
cd "$(dirname "$0")/.." || exit 1
fail=0
for f in tests/parity/cases*.txt; do
  name=$(basename "$f")
  if out=$(bash tests/parity.sh "$name" 2>&1); then
    echo "$name: $(echo "$out" | grep -E '一致' | tail -1)"
  else
    fail=1
    echo "!!! $name 不一致："
    echo "$out" | tail -30
  fi
done
if [ "$fail" -ne 0 ]; then
  echo "对拍失败"
  exit 1
fi
echo "全部对拍通过"
