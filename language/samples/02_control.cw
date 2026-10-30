// 02 控制结构
// if / elif / else、while、repeat、for，缩进决定块的范围

int total = 0

repeat(3)
    total += 1
print("repeat 之后：" + total)

// 次数由变量决定，循环变量随便起名，循环体里能直接用
int n = 3
for i in range(n)
    total += i
    print(join("i = ", i, "，total = ", total))

// 嵌套循环各用各的变量
for a in range(2)
    for b in range(2)
        print(join("坐标 ", a, ",", b))

// for i = 1 to 3 含两端
for k = 1 to 3
    print(join("第 ", k, " 轮"))

// 条件分支
if (total > 12)
    print("大于 12")
elif (total == 12)
    print("正好 12")
else
    print("小于 12")

// while + 跳出：balo 跳过本轮，out 跳出整个循环
int m = 0
while (m < 6)
    m++
    if (m == 2)
        balo
    print("m = " + m)
    if (m == 4)
        out
