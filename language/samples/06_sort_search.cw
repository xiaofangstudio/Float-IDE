// 06 排序与查找 —— CW Script
// 冒泡 / 插入两种排序，加一个二分查找。
// 顺便验证：同一个列表反复排序、原地修改、按下标读写都正常。

fn bubble_sort(xs)
    int n = len(xs)
    for i in range(n)
        for j in range(n - 1 - i)
            if (xs[j] > xs[j + 1])
                int t = xs[j]
                xs[j] = xs[j + 1]
                xs[j + 1] = t
    return xs

fn insertion_sort(xs)
    int n = len(xs)
    for i in range(1, n)
        int cur = xs[i]
        int j = i - 1
        while (j >= 0 && xs[j] > cur)
            xs[j + 1] = xs[j]
            j--
        xs[j + 1] = cur
    return xs

fn binary_search(xs, target)
    int lo = 0
    int hi = len(xs) - 1
    while (lo <= hi)
        int mid = floor((lo + hi) / 2)
        if (xs[mid] == target)
            return mid
        if (xs[mid] < target)
            lo = mid + 1
        else
            hi = mid - 1
    return -1

fn check_sorted(xs)
    for i in range(1, len(xs))
        if (xs[i - 1] > xs[i])
            return false
    return true

list data = [5, 3, 9, 1, 7, 2, 8, 4, 6, 0]
print("原始数据：", data)

list a = [5, 3, 9, 1, 7, 2, 8, 4, 6, 0]
bubble_sort(a)
print("冒泡排序：", a)
print("  有序？", check_sorted(a))

list b = [5, 3, 9, 1, 7, 2, 8, 4, 6, 0]
insertion_sort(b)
print("插入排序：", b)
print("  有序？", check_sorted(b))

print("")
print("二分查找：")
for target in [7, 0, 9, 42]
    int pos = binary_search(a, target)
    if (pos >= 0)
        print(join("  找 ", target, " → 下标 ", pos))
    else
        print(join("  找 ", target, " → 没找到"))

print("")
print("顺便算一下斐波那契前 12 项：")
fn fib(n)
    if (n < 2)
        return n
    return fib(n - 1) + fib(n - 2)
str line = ""
for i in range(12)
    line += str(fib(i))
    line += " "
print("  " + line)
