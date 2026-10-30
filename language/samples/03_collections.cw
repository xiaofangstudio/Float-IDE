// 03 列表与字典

list nums
add 3 to nums
add 1 to nums
add 2 to nums
print("长度：" + len(nums))

// 下标访问与修改，负数从末尾算
print("第一个：" + nums[0])
print("最后一个：" + nums[-1])
nums[0] = 9

// 遍历列表
int sum = 0
for v in nums
    sum += v
print("求和：" + sum)

// 字典：字面量写法最省事
dict scores = {"小明": 90, "小红": 95}
scores["小刚"] = 88

dict scores read "小红"
print("小红分数：" + __last_read)

// 遍历字典：for k, v in 字典
for who, score in scores
    print(join(who, " → ", score))

print("键：" + dictallkey(scores))
print("有没有小刚：" + dictin(scores, "小刚"))
