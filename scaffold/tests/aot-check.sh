#!/usr/bin/env bash
# AOT 正确性扫描：每个示例分别用 VM 和 AOT 跑一遍，输出必须逐字节一致。
# AOT 不支持的特性（ask / 指针）会自动回落 VM —— 那种算"回落"，不算失败。
set -u
cd "$(dirname "$0")/../src-tauri/float-lang" || exit 1
# 运行时默认静默（不打印「回落 VM」）；这个脚本要靠那条提示判断是否回落，所以显式打开。
export COPPER_AOT_VERBOSE=1
FL=./target/release/examples
ok=0; fallback=0; bad=0
for f in ../../../language/samples/*.co; do
  name=$(basename "$f")
  vm=$(timeout 20 "$FL/run.exe" "$f" </dev/null 2>/dev/null | grep -v '^—— 用时')
  # 带 --run：编译成功后直接跑原生 exe（不带的话 aot.exe 只编译不运行，拿不到输出）
  aot=$(timeout 120 "$FL/aot.exe" "$f" --run </dev/null 2>&1)
  if echo "$aot" | grep -q "回落 VM"; then
    # 回落：aot.exe 已经把 VM 结果打出来了，天然一致
    fallback=$((fallback+1))
    echo "回退  $name"
    continue
  fi
  # 去掉 aot 自己的进度行
  # 去掉 aot 自己的进度行（verbose 开着才会有）
  aot_out=$(echo "$aot" | grep -v '^AST → Rust' | grep -v '^rustc 编译' | grep -v '^已输出' | grep -v '本机执行用时' | grep -v '^分块')
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
