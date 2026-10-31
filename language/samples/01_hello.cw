// 01 你好 —— CW Script
// 行注释用 //，整行跳过用 --
// 一行一句，缩进成块，行尾的 { } 可写可不写

ask "你叫什么名字？"
str name = ans

print(join("你好，", name))
print("你的名字有 " + strlen(name) + " 个字")

// 变量声明右边可以是任意表达式
int count = 0
repeat(3)
    count++
    print(join("第 ", count, " 次问好"))
