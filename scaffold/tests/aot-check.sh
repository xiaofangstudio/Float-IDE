#!/usr/bin/env bash
# AOT 正确性扫描：每个示例分别用 VM 和 AOT 跑一遍，输出必须逐字节一致。
# AOT 不支持的特性（ask / 指针）会自动回落 VM —— 那种算"回落"，不算失败。
set -u
cd "$(dirname "$0")/../src-tauri/float-lang" || exit 1
FL=./target/release/examples
ok=0; fallback=0; bad=0
for f in ../../../language/samples/*.co; do
  name=$(basename "$f")
  vm=$(timeout 20 "$FL/run.exe" "$f" </dev/null 2>/dev/null | grep -v '^—— 用时')
  aot=$(timeout 90 "$FL/aot.exe" "$f" </dev/null 2>&1)
  if echo "$aot" | grep -q "回落 VM"; then
    # 回落：aot.exe 已经把 VM 结果打出来了，天然一致
    fallback=$((fallback+1))
    echo "回退  $name"
    continue
  fi
  # 去掉 aot 自己的进度行
  aot_out=$(echo "$aot" | grep -v '^AST → Rust' | grep -v '^rustc 编译' | grep -v '^已输出' | grep -v '^本机' )
  if [ "$vm" = "$aot_out" ]; then
    ok=$((ok+1))
  else
    bad=$((bad+1))
    echo "❌ $name"
    diff <(echo "$vm") <(echo "$aot_out") | head -6
  fi
done
echo "AOT 扫描：一致 $ok / 回落 $fallback / 不一致 $bad"
[ "$bad" = 0 ]
