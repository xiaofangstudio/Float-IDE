//! data.* 带回调的高阶函数：Rust VM 专属能力（TS 兼容引擎没有回调机制）。
//! lambda 语法本身两边都支持，但 data.filter/map 这类组合只在 F5 / Rust VM 里跑。

use float_lang::run_text;

fn out(src: &str) -> String {
    run_text(src).expect("应该能跑").trim().to_string()
}

#[test]
fn map_filter_reduce() {
    let src = r#"
list xs = [1, 2, 3, 4, 5]
print(data.map(xs, fn(x) return x * 2))
print(data.filter(xs, fn(x) return x > 2))
print(data.reduce(xs, fn(a, b) return a + b, 0))
"#;
    assert_eq!(out(src), "[2, 4, 6, 8, 10]\n[3, 4, 5]\n15");
}

#[test]
fn any_all_短路() {
    let src = r#"
int calls = 0
function enough(x)
    calls += 1
    return x > 2
list xs = [1, 2, 3, 4]
print(data.any(xs, enough))
print(calls)
"#;
    // 第三个元素命中就停，只调了 3 次
    assert_eq!(out(src), "true\n3");
}

#[test]
fn all_遇到假就停() {
    let src = r#"
int calls = 0
function bad(x)
    calls += 1
    return x < 3
list xs = [1, 2, 3, 4]
print(data.all(xs, bad))
print(calls)
"#;
    assert_eq!(out(src), "false\n3");
}

#[test]
fn find_与_find_index() {
    let src = r#"
list xs = [1, 2, 3, 4, 5]
print(data.find(xs, fn(x) return x > 3))
print(data.find_index(xs, fn(x) return x == 4))
print(data.find(xs, fn(x) return x > 99))
print(data.find_index(xs, fn(x) return x > 99))
"#;
    assert_eq!(out(src), "4\n3\nnull\n-1");
}

#[test]
fn groupby_与_index_by() {
    let src = r#"
list xs = [1, 2, 3, 4, 5]
print(data.groupby(xs, fn(x) return x // 2))
print(data.index_by(xs, fn(x) return x))
"#;
    assert_eq!(out(src), "{0: [1], 1: [2, 3], 2: [4, 5]}\n{1: 1, 2: 2, 3: 3, 4: 4, 5: 5}");
}

#[test]
fn 函数值当回调() {
    let src = r#"
function double(x)
    return x * 2
list xs = [1, 2, 3]
print(data.map(xs, double))
fn f = double
print(data.map(xs, f))
"#;
    assert_eq!(out(src), "[2, 4, 6]\n[2, 4, 6]");
}

#[test]
fn 字典列表当元素() {
    let src = r#"
list students = [{"分": 90}, {"分": 60}, {"分": 85}]
function big(s)
    return s["分"] >= 80
print(data.filter(students, big))
print(data.find(students, big))
"#;
    assert_eq!(out(src), "[{分: 90}, {分: 85}]\n{分: 90}");
}
