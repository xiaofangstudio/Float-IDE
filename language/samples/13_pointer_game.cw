// ─ 指针小游戏：翻牌消消乐 ─
// 牌桌用 alloc 申请：每张牌占两格（第 0 格是牌面，第 1 格是"翻开没有"）。
// 手动内存是严格模式：程序结束前必须 free，忘了还就会报错。

list 图案 = ["苹果", "苹果", "星星", "星星", "月亮", "月亮", "云朵", "云朵"]
int 张数 = len(图案)

// 洗牌
for i in range(张数 - 1)
    int j = rand(i, 张数 - 1)
    str t = 图案[i]
    图案[i] = 图案[j]
    图案[j] = t

// 牌桌
ptr 牌桌 = alloc(张数 * 2)
for i in range(张数)
    牌桌[i * 2] = 图案[i]
    牌桌[i * 2 + 1] = false

fn 显示()
    str line = "  "
    for i in range(张数)
        if (牌桌[i * 2 + 1])
            line += join(" ", 牌桌[i * 2], " ")
        else
            line += join(" [", i + 1, "] ")
    print(line)

fn 读牌(prompt)
    ask prompt
    str raw = trim(ans)
    if (raw == "")
        return 0
    return int(raw)

int 还剩 = 张数
int 轮数 = 0
print("========================================")
print("  翻牌消消乐：翻两张一样的就消掉")
print("  直接回车 = 不玩了")
print("========================================")
报告 "游戏开始，共 4 对牌"

while (还剩 > 0)
    轮数++
    print("")
    print(join("第 ", 轮数, " 轮，还剩 ", 还剩, " 张"))
    显示()

    int a = 读牌("翻第几张？")
    if (a == 0)
        print("下次再玩。")
        break
    int b = 读牌("再翻第几张？")
    if (b == 0)
        print("下次再玩。")
        break

    if (a < 1 || a > 张数 || b < 1 || b > 张数 || a == b)
        print("这两张不行，重来一轮。")
        continue
    if (牌桌[(a - 1) * 2 + 1] || 牌桌[(b - 1) * 2 + 1])
        print("已经翻开过了，重来一轮。")
        continue

    牌桌[(a - 1) * 2 + 1] = true
    牌桌[(b - 1) * 2 + 1] = true
    显示()

    str 甲 = 牌桌[(a - 1) * 2]
    str 乙 = 牌桌[(b - 1) * 2]
    if (甲 == 乙)
        还剩 -= 2
        print(join("  两张都是「", 甲, "」，消掉！"))
        报告 join("第 ", 轮数, " 轮消掉一对：", 甲, "（还剩 ", 还剩, " 张）")
    else
        牌桌[(a - 1) * 2 + 1] = false
        牌桌[(b - 1) * 2 + 1] = false
        print(join("  是「", 甲, "」和「", 乙, "」，不一样，翻回去。"))

// 用完了必须还
free(牌桌)
if (还剩 == 0)
    print(join("全部消完，一共 ", 轮数, " 轮。"))
    报告 join("通关：", 轮数, " 轮")
else
    报告 "退出游戏"
