// 10 小方找书包 —— 小 RPG
// 书包藏在一个随机的地方，每找一个地方消耗一点体力，
// 走错地方会有小事件。体力用完就输，找到就赢。

list places = ["书桌下面", "衣柜顶层", "床底", "客厅沙发", "厨房台面", "阳台纸箱", "门口的鞋柜"]

dict flavor = {}
flavor["书桌下面"] = "只有一堆草稿纸和半截铅笔。"
flavor["衣柜顶层"] = "被子叠得整整齐齐，没有书包。"
flavor["床底"] = "灰尘扑面而来，摸到一只袜子。"
flavor["客厅沙发"] = "坐垫缝里摸出三块硬币，聊胜于无。"
flavor["厨房台面"] = "妈妈刚切好的苹果，你先吃了一块。"
flavor["阳台纸箱"] = "全是快递盒，写着「易碎」。"
flavor["门口的鞋柜"] = "鞋摆得歪歪扭扭，还是没看见书包。"

fn show_places(xs)
    int i = 1
    for p in xs
        print(join("   ", i, ". ", p))
        i++

fn idx_of(xs, target)
    int i = 0
    for x in xs
        if (x == target)
            return i
        i++
    return -1

int secret = rand(0, len(places) - 1)
str answer = places[secret]
int stamina = 7
int round = 0

print("========================================")
print("  小方找书包")
print("========================================")
print("小方：完了完了，书包不见了！")
print("小方：里面有今天要交的作业，还有午饭钱……")
print("旁白：你只有 7 点体力，正好够把每个地方都查一遍。")

while (true)
    round++
    print("")
    print(join("【第 ", round, " 回合 · 体力 ", stamina, "】"))
    print("要找哪里？")
    show_places(places)
    ask "输入序号（0 放弃）："

    str raw = trim(ans)
    if (raw == "")
        print("旁白：你愣了一会儿，什么也没做。")
    else
        int pick = int(raw)
        if (pick == 0)
            print("小方：算了……今天就不交作业了吧。")
            print("—— 结局：被老师点名。")
            return
        if (pick < 1 || pick > len(places))
            print("旁白：没有这个地方。")
        else
            str here = places[pick - 1]
            print(join("你走向", here, "……"))
            if (here == answer)
                print("小方：找到了！！就在这儿！")
                print("旁白：书包好好地躺着，拉链还是开着的。")
                print(join("—— 结局：赢了。用了 ", round, " 个回合。"))
                return
            print(join("旁白：", flavor[here]))

    stamina--
    if (stamina <= 0)
        print("")
        print("旁白：你瘫在沙发上，实在找不动了。")
        print(join("小方：……书包其实在", answer, "。"))
        print("—— 结局：天黑了才想起来。")
        return
