import time

def bench_fib():
    def fib(n):
        if n < 2:
            return n
        return fib(n - 1) + fib(n - 2)
    fib(30)

def bench_loop():
    s = 0
    for i in range(5000000):
        s += i

def bench_str():
    out = ""
    for i in range(50000):
        out += "x"

for name, fn in [("fib(30) 递归", bench_fib), ("循环累加 500 万", bench_loop), ("字符串拼接 5 万", bench_str)]:
    fn()  # 预热
    t0 = time.perf_counter()
    fn()
    print(f"{name}: {(time.perf_counter() - t0) * 1000:.1f} ms")
