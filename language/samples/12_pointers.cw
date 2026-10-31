// 12 指针与手动内存 —— CW Script
// 平时写脚本不用管内存：列表、字典、字符串都是自动回收的。
// 想要自己管也能管：alloc 申请格子、free 归还。
// 两种写法可以在同一个程序里混着用，不需要任何开关。

// ① 自动管理：什么都不用写
list xs = [1, 2, 3]
print("列表：", xs, " 长度 ", len(xs))

// ② 手动管理：申请 5 个格子，初值都是 null
ptr buf = alloc(5)
print("申请到：", buf)

for i in range(5)
    buf[i] = i * i
print("填完：", buf[0], buf[1], buf[2], buf[3], buf[4])

// ③ 指针也能当集合遍历，也能取长度
int sum = 0
for v in buf
    sum += v
print("求和：", sum, " 长度 ", len(buf))

// ④ 指针和普通变量一样能传来传去
fn fill_with(p, n, v)
    for i in range(n)
        p[i] = v
    return p

fill_with(buf, 2, 9)
print("前两格被改了：", buf[0], buf[1])

// ⑤ 用完归还。忘了归还也不会漏——没人引用时会自动回收
free(buf)
print("归还之后：", buf)

// ⑥ 自动和手动混用：指针里装字符串、列表都行
ptr box = alloc(3)
box[0] = "书包"
box[1] = [1, 2, 3]
box[2] = 42
print("混着装：", box[0], box[1], box[2])
free(box)
