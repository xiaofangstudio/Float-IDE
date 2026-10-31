#!/usr/bin/env bash
# 公平对比：Python / Rust VM / Node(V8) 交替跑多轮，每项取最好成绩。
# 笔记本会降频，所以交替跑比"先跑完一个再跑下一个"可信。
set -u
cd "$(dirname "$0")/.." || exit 1

ROUNDS=${1:-5}
RUST=./src-tauri/float-lang/target/release/examples/bench.exe

best_fib_py=99999; best_loop_py=99999; best_str_py=99999
best_fib_rs=99999; best_loop_rs=99999; best_str_rs=99999

pick() { # $1=file 前缀 $2=当前最好
  echo "$2"
}

for i in $(seq 1 "$ROUNDS"); do
  py=$(python tests/bench.py 2>/dev/null)
  f=$(echo "$py" | sed -n '1p' | grep -o '[0-9.]*' | tail -1)
  l=$(echo "$py" | sed -n '2p' | grep -o '[0-9.]*' | tail -1)
  s=$(echo "$py" | sed -n '3p' | grep -o '[0-9.]*' | tail -1)
  best_fib_py=$(python -c "print(min($best_fib_py,$f))")
  best_loop_py=$(python -c "print(min($best_loop_py,$l))")
  best_str_py=$(python -c "print(min($best_str_py,$s))")

  rs=$("$RUST")
  f=$(echo "$rs" | sed -n '3p' | awk '{print $(NF-1)}')
  l=$(echo "$rs" | sed -n '4p' | awk '{print $(NF-1)}')
  s=$(echo "$rs" | sed -n '5p' | awk '{print $(NF-1)}')
  f=$(echo "$rs" | sed -n '3p' | grep -o '[0-9.]* ms' | sed -n '2p' | grep -o '[0-9.]*')
  l=$(echo "$rs" | sed -n '4p' | grep -o '[0-9.]* ms' | sed -n '2p' | grep -o '[0-9.]*')
  s=$(echo "$rs" | sed -n '5p' | grep -o '[0-9.]* ms' | sed -n '2p' | grep -o '[0-9.]*')
  best_fib_rs=$(python -c "print(min($best_fib_rs,$f))")
  best_loop_rs=$(python -c "print(min($best_loop_rs,$l))")
  best_str_rs=$(python -c "print(min($best_str_rs,$s))")
done

python - "$best_fib_py" "$best_loop_py" "$best_str_py" "$best_fib_rs" "$best_loop_rs" "$best_str_rs" <<'PY'
import sys
py = [float(x) for x in sys.argv[1:4]]
rs = [float(x) for x in sys.argv[4:7]]
names = ["fib(30) 递归", "循环累加 500 万", "字符串拼接 5 万"]
print(f"{'用例':<16}{'Python':>10}{'float-lang':>14}{'倍数':>10}")
for n, p, r in zip(names, py, rs):
    print(f"{n:<16}{p:>8.1f}ms{r:>11.1f}ms{p / r:>9.2f}x")
PY
