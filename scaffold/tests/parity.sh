#!/usr/bin/env bash
# 双引擎对拍：同一批脚本分别用 TS 兼容引擎和 Rust 内核跑，比较输出。
# 用法：bash tests/parity.sh            （跑 cases.txt）
#       bash tests/parity.sh cases2.txt （跑别的用例文件）
set -u
cd "$(dirname "$0")/.." || exit 1
CASES=${1:-cases.txt}

npx esbuild tests/parity/run-ts.ts --bundle --platform=node --format=esm \
  --external:@tauri-apps/* --outfile=tests/parity/.run-ts.mjs --log-level=warning || exit 1
CASES="$CASES" node tests/parity/.run-ts.mjs || exit 1
( cd src-tauri/float-lang && CASES="$CASES" cargo run --release --example parity >/dev/null 2>&1 ) || {
  echo "Rust 侧跑失败"; exit 1; }

node -e '
const ts = JSON.parse(require("fs").readFileSync("tests/parity/ts.json", "utf8"));
const rs = JSON.parse(require("fs").readFileSync("tests/parity/rust.json", "utf8"));
let same = 0, diff = 0;
for (const k of Object.keys(ts)) {
  const t = ts[k], r = rs[k] || {};
  // 两边都报错就算一致（错误文案不需要逐字相同）
  if ((t.err || []).length && (r.err || []).length) { same++; continue; }
  const a = JSON.stringify(t), b = JSON.stringify(r);
  if (a === b) { same++; continue; }
  diff++;
  console.log("=== " + k);
  console.log("  TS  : " + a);
  console.log("  Rust: " + b);
}
console.log(`一致 ${same}，不一致 ${diff}`);
process.exit(diff ? 1 : 0);
'
