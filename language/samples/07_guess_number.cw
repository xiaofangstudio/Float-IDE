// 07 猜数字 —— 小游戏
// 电脑想一个 1~100 的数，你用二分法猜，8 次机会。

int secret = rand(1, 100)
int tries = 0
int limit = 8
int lo = 1
int hi = 100

print("========================================")
print("  猜数字：我想好了一个 1~100 的整数")
print("  你有 " + limit + " 次机会，输入数字回车")
print("========================================")

while (true)
    ask "你的猜测："
    str raw = trim(ans)
    if (raw == "")
        print("没输入，先不玩了。")
        return

    int guess = int(raw)
    tries++

    if (guess == secret)
        print(join("★ 对了！就是 ", secret, "，你用了 ", tries, " 次。"))
        return

    if (guess < secret)
        lo = guess + 1
        print("小了 ↑")
    else
        hi = guess - 1
        print("大了 ↓")

    if (tries >= limit)
        print(join("× 机会用完了，答案是 ", secret, "。"))
        return

    print(join("  还剩 ", limit - tries, " 次，范围大概在 ", lo, " ~ ", hi))
