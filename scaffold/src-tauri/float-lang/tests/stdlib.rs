//! 标准库（os / data / json / time / random / ui / re / net）的内核侧守门测试。
//!
//! 跨引擎一致性由 `tests/parity/cases11.txt` 对拍；这里盯的是 Rust 内核自己的行为：
//! 算对、报错说人话、`import 库名` 不改变行号、命名空间写错要当场拦住。

use float_lang::api::run_text;

fn out(src: &str) -> String {
    match run_text(src) {
        Ok(s) => s,
        Err(e) => panic!("运行失败：{}\n源码：\n{}", e.message, src),
    }
}

fn assert_out(src: &str, want: &[&str]) {
    let got = out(src);
    let got_lines: Vec<&str> = got.lines().collect();
    assert_eq!(got_lines, want, "\n实际输出：\n{}\n源码：\n{}", got, src);
}

fn err(src: &str) -> String {
    match run_text(src) {
        Ok(s) => panic!("本该报错，却跑成功了，输出：\n{}\n源码：\n{}", s, src),
        Err(e) => e.message,
    }
}

/* ─────────────── import 与命名空间 ─────────────── */

#[test]
fn import_库名不报错也不占输出() {
    assert_out("import os\nimport data\nimport json\nprint(\"ok\")\n", &["ok"]);
}

#[test]
fn import_之后行号还是对的() {
    // 第 4 行出错，报错行号必须是 4（import 被换成空语句，不是删掉）
    let e = run_text("import os\nimport data\n\nprint(1 / 0)\n").err().expect("应该报错");
    assert_eq!(e.span.line, 4, "报错行号错了：{}", e.message);
}

#[test]
fn 库里没有的函数当场拦住() {
    /* 以前解析期直接报「data 里没有 nothing」，但那会误伤
     * `str data = "a,b"` + `data.split(",")` 这种同名变量的正常调用。
     * 现在改成运行期指出「是命名空间、不是变量，检查函数名拼写」。 */
    let m = err("import data\nprint(data.nothing(1))\n");
    assert!(m.contains("命名空间") && m.contains("拼写"), "报错文案不对：{}", m);
    let m2 = err("import os\nprint(os.nothing(1))\n");
    assert!(m2.contains("命名空间") && m2.contains("拼写"), "报错文案不对：{}", m2);
}

/* ─────────────── data ─────────────── */

#[test]
fn data_排序与统计() {
    assert_out(
        "import data\n\
         list xs = [3, 1, 2]\n\
         print(str(data.sort(xs)))\n\
         print(str(xs))\n\
         print(str(data.sort([3, 1, 2], \"desc\")))\n\
         print(str(data.sum([1, 2, 3])))\n\
         print(str(data.avg([1, 2, 3])))\n\
         print(str(data.median([1, 2, 3, 4])))\n",
        &["[1, 2, 3]", "[3, 1, 2]", "[3, 2, 1]", "6", "2", "2.5"],
    );
}

#[test]
fn data_集合操作() {
    assert_out(
        "import data\n\
         print(str(data.unique([1, 2, 2, 3, 1])))\n\
         print(str(data.freq([\"a\", \"a\", \"b\"])))\n\
         print(str(data.chunk([1, 2, 3], 2)))\n\
         print(str(data.zip([1, 2], [\"a\"])))\n\
         print(str(data.slice([1, 2, 3, 4, 5], -2)))\n\
         print(str(data.flatten([1, [2, [3]]])))\n",
        &[
            "[1, 2, 3]",
            "{a: 2, b: 1}",
            "[[1, 2], [3]]",
            "[[1, \"a\"]]",
            "[4, 5]",
            "[1, 2, [3]]",
        ],
    );
}

#[test]
fn data_按字段排序() {
    assert_out(
        "import data\nlist 学生 = [{\"name\": \"a\", \"score\": 88}, {\"name\": \"b\", \"score\": 93}]\nfor s in data.sort_by(学生, \"score\", \"desc\")\n    print(s[\"name\"])\n",
        &["b", "a"],
    );
}

#[test]
fn data_参数错了要报错() {
    assert!(err("import data\nprint(str(data.avg([\"a\"])))\n").contains("需要数字"));
    assert!(err("import data\nprint(str(data.sum([1, \"x\"])))\n").contains("不是数字"));
    assert!(err("import data\nprint(str(data.chunk([1], 0)))\n").contains("大于 0"));
    assert!(err("import data\nprint(str(data.median([])))\n").contains("空列表"));
    assert!(err("import data\nprint(str(data.is_empty(1)))\n").contains("需要列表"));
}

/* ─────────────── json ─────────────── */

#[test]
fn json_往返() {
    assert_out(
        "import json\n\
         var j = json.parse(\"{\\\"a\\\": [1, 2.5, true, null], \\\"b\\\": \\\"中文\\\"}\")\n\
         print(j[\"b\"])\n\
         print(json.stringify(j))\n\
         print(json.pretty({\"x\": 1}))\n",
        // 输出按行切分（和 IDE 控制台一致），pretty 的三行就是三条
        &["中文", "{\"a\":[1,2.5,true,null],\"b\":\"中文\"}", "{", "  \"x\": 1", "}"],
    );
}

#[test]
fn json_坏文本要报错并且try_parse不炸() {
    assert!(err("import json\nprint(json.parse(\"{\\\"a\\\": }\"))\n").contains("json.parse"));
    assert_out(
        "import json\nvar r = json.try_parse(\"{\\\"a\\\": }\")\nprint(str(r[\"ok\"]))\n",
        &["false"],
    );
}

/* ─────────────── time ─────────────── */

#[test]
fn time_固定时间戳() {
    assert_out(
        "import time\n\
         print(time.format(0, \"YYYY-MM-DD\"))\n\
         print(str(time.diff(100, 160)))\n\
         print(str(time.add(1000, 60)))\n\
         print(str(time.parts(0)[\"year\"]))\n",
        &["1970-01-01", "60", "1060", "1970"],
    );
}

/* ─────────────── random：种子定死就必须可复现 ─────────────── */

#[test]
fn random_同一个种子同一串数() {
    let src = "import random\nrandom.seed(42)\nprint(str(random.rand(1, 100)))\nprint(str(random.rand(1, 100)))\n";
    let a = out(src);
    let b = out(src);
    assert_eq!(a, b, "同一个种子跑出两串不同的数");
    for line in a.lines() {
        let n: i64 = line.trim().parse().expect("rand 应该是整数");
        assert!((1..=100).contains(&n), "rand 越界：{}", n);
    }
}

#[test]
fn random_参数错了要报错() {
    assert!(err("import random\nprint(str(random.sample([1, 2], 5)))\n").contains("数量不对"));
    assert!(err("import random\nprint(str(random.weighted([1], [1, 2])))\n").contains("数量要一样"));
}

/* ─────────────── re ─────────────── */

#[test]
fn re_基本匹配与分组() {
    assert_out(
        "import re\n\
         print(str(re.findall(\"[0-9]+\", \"a1b22c333\")))\n\
         print(re.replace(\"[0-9]+\", \"N\", \"a1b22\"))\n\
         print(str(re.split(\",\", \"a,b,c\")))\n\
         print(str(re.findall(\"href=\\\"([^\\\"]+)\\\"\", \"x href=\\\"/a\\\" y\")))\n\
         print(str(re.test(\"\\\\d+\", \"abc\")))\n",
        &["[\"1\", \"22\", \"333\"]", "aNbN", "[\"a\", \"b\", \"c\"]", "[\"/a\"]", "false"],
    );
}

/* ─────────────── ui ─────────────── */

#[test]
fn ui_进度条与表格() {
    assert_out(
        "import ui\nprint(ui.bar(3, 10, 10))\n",
        &["[███░░░░░░░]"],
    );
    // 中文按 2 格宽算，两行才对得齐
    let table = out("import ui\nprint(ui.table([\"名字\"], [[\"阿方\"], [\"chen\"]]))\n");
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines.len(), 3, "表格应该是表头 + 两行：\n{}", table);
    assert_eq!(lines[0], "名字", "表头被中文宽度算歪了：{:?}", lines[0]);
}

/* ─────────────── os：不需要宿主的路径函数 ─────────────── */

#[test]
fn os_路径函数() {
    assert_out(
        "import os\n\
         print(os.basename(\"a/b/c.co\"))\n\
         print(os.dirname(\"a/b/c.co\"))\n\
         print(os.extname(\"a/b/c.co\"))\n\
         print(os.pathjoin(\"a/\", \"b\"))\n",
        &["c.co", "a/b", ".co", "a/b"],
    );
}

/* ─────────────── 函数表与实现必须一一对上 ─────────────── */

#[test]
fn 命名空间表里的函数都能调用到() {
    // 表里写了但实现里漏了，调用就会报"没有这个函数"——这条测试专门抓这种漏
    for (ns, fns) in float_lang::stdlib::NAMESPACES {
        for f in *fns {
            let full = format!("{}.{}", ns, f);
            assert!(float_lang::stdlib::is_fn(&full), "表里列了 {} 但 is_fn 不认识", full);
        }
    }
}
