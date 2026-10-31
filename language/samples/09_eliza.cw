// 09 小伊 —— 一个像 Eliza 那样的对话机器人
// 原理很朴素：找关键词 → 换个说法反问回去。真正有意思的是"没有关键词"时也要能接话。

dict replies = {}
replies["妈妈"] = ["你家里还有谁？", "跟我说说你妈妈吧。", "你和妈妈像吗？"]
replies["爸爸"] = ["你爸爸是做什么的？", "你和爸爸关系怎么样？"]
replies["朋友"] = ["你的朋友多吗？", "朋友对你来说意味着什么？"]
replies["工作"] = ["你喜欢现在的工作吗？", "工作里最让你累的是什么？"]
replies["学习"] = ["你最近在学什么？", "学不会的时候你会怎么办？"]
replies["累"] = ["听起来你挺累的。是什么让你这么累？", "累的时候你会做什么？"]
replies["开心"] = ["什么事让你开心？", "这种开心能持续多久？"]
replies["难过"] = ["愿意多说一点吗？", "难过的时候身边有人吗？"]
replies["害怕"] = ["你在害怕什么？", "如果不怕了，你会做什么？"]
replies["喜欢"] = ["你说说看，喜欢它哪一点？", "这种喜欢是什么时候开始的？"]
replies["为什么"] = ["你自己觉得呢？", "如果知道原因，会有什么不一样？"]

list probing = ["嗯，然后呢？", "我听到了。你怎么看这件事？", "这让你想起了什么？", "说具体一点？", "如果换个角度，会是什么样？"]
int probe_i = 0

fn find_key(text)
    list keys = replies.keys()
    for k in keys
        if (text.contains(k))
            return k
    return ""
}

fn reply_to(text)
    str k = find_key(text)
    list opts = []
    if (k != "")
        opts = replies[k]
    else
        probe_i++
        if (probe_i >= len(probing))
            probe_i = 0
        print(join("小伊：", probing[probe_i]))
        return

    int i = rand(0, len(opts) - 1)
    print(join("小伊：", opts[i]))

print("========================================")
print("  小伊 —— 一个很早期的对话程序")
print("  （说「再见」结束）")
print("========================================")

while (true)
    ask "你："
    str text = trim(ans)
    if (text == "")
        print("小伊：你还在吗？")
        continue
    if (text.contains("再见") || text.contains("拜拜") || text == "exit")
        print("小伊：今天就聊到这儿。照顾好自己。")
        return

    reply_to(text)
