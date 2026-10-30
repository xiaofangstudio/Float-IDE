// 04 函数
// function / fn / Function / f 都能定义函数
// 支持多参数、默认值、返回值、递归，也可以在表达式里直接调用

function greet(name, greeting = "你好")
    return join(greeting, "，", name)

print(greet("小明"))
print(greet("小红", "晚上好"))

// 递归
function fact(n)
    if (n <= 1)
        return 1
    return n * fact(n - 1)

print("5! = " + fact(5))

// 默认值可以引用前面的参数
function area(w, h = w)
    return w * h

print("3×4 = " + area(3, 4))
print("5×5 = " + area(5))

// 老的写法照样能用：函数体内改全局变量
int counter = 0
function bump()
    counter += 1
bump()
bump()
print("counter = " + counter)
