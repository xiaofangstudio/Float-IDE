// 11 计算器 —— 选择运算 + 输入两个数
// 练的是：字符串切分、类型转换、分支分发。

fn calc(a, b, op)
    if (op == "1")
        return a + b
    if (op == "2")
        return a - b
    if (op == "3")
        return a * b
    if (op == "4")
        if (b == 0)
            return "除数不能是 0"
        return a / b
    return "不认识的运算"

fn fmt(v)
    // 整数就按整数显示，小数保留两位
    if (type(v) == "int")
        return str(v)
    if (type(v) == "str")
        return v
    return str(round(v * 100) / 100)

print("========================================")
print("  计算器")
print("  1 加法   2 减法   3 乘法   4 除法")
print("========================================")

while (true)
    ask "选择运算（1-4，直接回车退出）："
    str op = trim(ans)
    if (op == "" || op == "q")
        print("再见。")
        return
    if (op != "1" && op != "2" && op != "3" && op != "4")
        print("只能输入 1 / 2 / 3 / 4。")
        continue

    ask "输入两个数（用空格分开）："
    str raw = trim(ans)
    list parts = split(raw, " ")
    if (len(parts) < 2)
        print("要输入两个数，中间用空格分开。")
        continue

    int ai = int(parts[0])
    int bi = int(parts[1])

    // 两个都是整数就走整数运算，结果更干净（10 / 4 会给 2.5）
    str sym = "+"
    if (op == "2")
        sym = "-"
    if (op == "3")
        sym = "*"
    if (op == "4")
        sym = "/"

    str shown = fmt(calc(ai, bi, op))
    print(join("  ", ai, " ", sym, " ", bi, " = ", shown))
    print("")
