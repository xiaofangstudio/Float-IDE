import { run } from './harness';

const cases: [string, string][] = [
  ['列表里放变量/表达式/调用', `
int a = 2
str b = "x"
list xs = [a, a * 3, b, join("k", a), len([1,2,3])]
print(xs)
`],
  ['嵌套列表与字典', `
list inner = [1, 2]
dict d = {"k": inner, "n": 3}
print(d)
print(d["k"][1])
`],
  ['函数调用结果直接下标', `
function mk()
    return [10, 20, 30]
print(mk()[1])
`],
  ['方法链出现在条件里', `
list xs = [3, 1, 2]
if (xs.sort().len() > 2)
    print("多")
`],
  ['for 遍历函数返回值', `
function rng(n)
    list out = []
    for i in range(n)
        out.push(i * 2)
    return out
for v in rng(3)
    print(v)
`],
  ['字典值里写调用', `
dict d = {"sum": add(1, 2), "up": "ab".upper()}
print(d)
`],
  ['类字段默认值用调用', `
class P
    str tag = join("t", 1)
P p = new P()
print(p.tag)
`],
  ['字符串插值', `
int score = 95
str name = "小明"
print("得分：{score}，名字：{name}，下一名：{score + 1}")
`],
];

void (async () => {
  for (const [name, src] of cases) {
    const r = await run(src);
    console.log(name + ' => ' + (r.err.length ? 'ERR ' + r.err[0] : JSON.stringify(r.out)));
  }
})();
