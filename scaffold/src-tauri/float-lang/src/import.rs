//! import 的展开：把被引入的文件**就地摊平**成一份源码。
//!
//! 为什么不做一个真正的模块系统？
//! 因为模块系统要动的东西太多（命名空间、导出表、循环依赖、私有性），
//! 而用户现在要的是"把公共函数放一个文件、别的文件里引进来用"。
//! 摊平能做到这件事，而且**零运行时开销**：编译出来的字节码跟手写在同一份
//! 文件里完全一样，函数调用不走任何间接层。
//!
//! 代价是行号会位移，所以这里顺手记下"哪一行属于哪个文件"，
//! 报错时能指回真正的文件和真正的行号（不记的话，库里的错误会报成
//! 主文件里一个莫名其妙的行号，那还不如不报）。

use crate::error::{FloatError, Span};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// 展开结果
#[derive(Debug)]
pub struct Expanded {
    /// 摊平后的源码（行号 = 这里面的行号）
    pub source: String,
    /// 参与的每个文件名（第 0 个是主文件）
    pub files: Vec<Rc<str>>,
    /// 合并后每一行属于哪个文件（下标 = 行号 - 1）
    pub line_file: Vec<u16>,
    /// 每一行在**它自己那个文件**里的行号（1 基）
    pub line_local: Vec<u32>,
}

/// import 展开的最大嵌套层数（超过就报错，别让递归吃栈）
const MAX_IMPORT_DEPTH: usize = 100;

struct Expander {
    out: Vec<String>,
    files: Vec<Rc<str>>,
    line_file: Vec<u16>,
    line_local: Vec<u32>,
    /// 已经在展开链上的文件：用来发现循环 import
    stack: Vec<PathBuf>,
    /// 已经摊平过的文件：同一个库 import 两次不会重复定义函数
    done: Vec<PathBuf>,
    /// IDE 里"还没存盘"的缓冲区：优先用内存里的内容，免得用户改了库没保存就跑出错
    overlay: HashMap<String, String>,
}

/// 把 `import "xxx.cw"` 全部摊平。
///
/// `base`：解析相对路径用的目录（一般是主文件所在目录）。
pub fn expand(main_file: &str, src: &str, base: Option<&Path>) -> Result<Expanded, FloatError> {
    expand_with(main_file, src, base, HashMap::new())
}

/// 同上，但带一份"内存里的文件内容"（路径 → 源码）。
/// IDE 传当前打开的所有缓冲区进来，没存盘的库文件也能 import 到。
pub fn expand_with(
    main_file: &str,
    src: &str,
    base: Option<&Path>,
    overlay: HashMap<String, String>,
) -> Result<Expanded, FloatError> {
    let mut ex = Expander {
        out: Vec::new(),
        files: Vec::new(),
        line_file: Vec::new(),
        line_local: Vec::new(),
        stack: Vec::new(),
        done: Vec::new(),
        overlay,
    };
    if let Some(b) = base {
        if let Ok(p) = b.canonicalize() {
            ex.stack.push(p.join("__main__"));
            ex.done.push(p.join("__main__"));
        }
    }
    ex.push_file(main_file, src, base)?;
    Ok(Expanded {
        source: ex.out.join("\n"),
        files: ex.files,
        line_file: ex.line_file,
        line_local: ex.line_local,
    })
}

impl Expander {
    /// 在"内存缓冲区"里找这个路径的源码（键可能是相对路径，也可能是绝对路径）
    fn mem_of(&self, p: &Path) -> Option<String> {
        if self.overlay.is_empty() {
            return None;
        }
        let canon = p.canonicalize().ok();
        let want = p.to_string_lossy().to_string();
        for (k, v) in self.overlay.iter() {
            if *k == want {
                return Some(v.clone());
            }
            if let Some(c) = &canon {
                if Path::new(k).canonicalize().ok().as_deref() == Some(c.as_path()) {
                    return Some(v.clone());
                }
            }
        }
        None
    }

    fn push_file(&mut self, name: &str, src: &str, dir: Option<&Path>) -> Result<(), FloatError> {
        self.files.push(Rc::from(name));
        let fid = (self.files.len() - 1) as u16;
        for (i, raw) in src.split('\n').enumerate() {
            let local = i as u32 + 1;
            // 只摊平顶格的 import：缩进进函数里的不展开（那是写法错了，交给解析器报错）
            let at_top = !raw.starts_with(|c: char| c.is_whitespace());
            match if at_top { import_path_of(raw) } else { None } {
                Some(want) => {
                    // import 这一行留个空行占位：它自己不产生代码，
                    // 但主文件的行号不能因为"这行被替换了"而整体前移
                    self.out.push(String::new());
                    self.line_file.push(fid);
                    self.line_local.push(local);
                    self.inline(&want, dir, local, name)?;
                }
                None => {
                    self.out.push(raw.to_string());
                    self.line_file.push(fid);
                    self.line_local.push(local);
                }
            }
        }
        Ok(())
    }

    fn inline(
        &mut self,
        want: &str,
        dir: Option<&Path>,
        line: u32,
        from: &str,
    ) -> Result<(), FloatError> {
        let mut cand: Vec<PathBuf> = Vec::new();
        let p = PathBuf::from(want);
        if p.is_absolute() {
            cand.push(p.clone());
        } else if let Some(d) = dir {
            cand.push(d.join(&p));
        }
        cand.push(p.clone());
        if p.extension().is_none() {
            let mut with_ext: Vec<PathBuf> = cand.iter().map(|c| c.with_extension("cw")).collect();
            cand.append(&mut with_ext);
        }
        // 内存里的缓冲区优先（IDE 里新建但没存盘的库文件也算"找得到"）
        let mut from_mem: Option<(PathBuf, String)> = None;
        for c in &cand {
            if let Some(text) = self.mem_of(c) {
                from_mem = Some((c.clone(), text));
                break;
            }
        }
        let found = match &from_mem {
            Some((p, _)) => Some(p.clone()),
            None => cand.iter().find(|c| c.is_file()).cloned(),
        };
        let path = match found {
            Some(p) => p,
            None => {
                return Err(FloatError::new(
                    format!(
                        "找不到库文件「{}」。import 的写法是 import \"工具.cw\"，\
                         路径相对当前脚本所在目录。",
                        want
                    ),
                    Span::new(&Rc::from(from), line, 1, 1),
                ))
            }
        };
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        // 展开是递归的：链子太深会真的把栈吃掉。100 层对真项目绰绰有余，
        // 超过就明确报错，而不是崩进程。
        if self.stack.len() >= MAX_IMPORT_DEPTH {
            return Err(FloatError::new(
                format!(
                    "import 嵌套超过 {} 层了，停下来。多半是库文件一层层互相 import；\
                     把公共部分抽成一个单独的库，别让引用链这么长。",
                    MAX_IMPORT_DEPTH
                ),
                Span::new(&Rc::from(from), line, 1, 1),
            ));
        }
        if self.stack.contains(&key) {
            return Err(FloatError::new(
                format!("「{}」被循环 import 了，两个文件互相引用会无限展开", want),
                Span::new(&Rc::from(from), line, 1, 1),
            ));
        }
        if self.done.contains(&key) {
            // 已经摊平过：不再展开（否则同一个库里的函数会被定义两次）
            return Ok(());
        }
        let text = match from_mem {
            Some((_, t)) => t,
            None => std::fs::read_to_string(&path).map_err(|e| {
                FloatError::new(
                    format!("读不了库文件「{}」：{}", want, e),
                    Span::new(&Rc::from(from), line, 1, 1),
                )
            })?,
        };
        let sub_dir = path.parent().map(|p| p.to_path_buf());
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| want.to_string());

        self.stack.push(key.clone());
        self.done.push(key);
        self.push_file(&label, &text, sub_dir.as_deref())?;
        self.stack.pop();
        Ok(())
    }
}

/// 一行是不是 import？是就返回要引入的路径
fn import_path_of(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.starts_with('#') || t.starts_with("//") || t.starts_with("--") {
        return None;
    }
    let rest = t.strip_prefix("import").or_else(|| t.strip_prefix("引入"))?;
    // "important" 这种不能误判
    if !rest.starts_with(|c: char| c.is_whitespace() || c == '(' || c == '"' || c == '\'') {
        return None;
    }
    let mut s = rest.trim();
    s = s.trim_end_matches(';').trim();
    s = s.trim_start_matches('(').trim_end_matches(')').trim();
    let s = s.trim_matches('"').trim_matches('\'').trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn 认得出_import_行() {
        assert_eq!(import_path_of("import \"a.cw\""), Some("a.cw".into()));
        assert_eq!(import_path_of("  引入 b"), Some("b".into()));
        assert_eq!(import_path_of("import(\"c.cw\")"), Some("c.cw".into()));
        assert_eq!(import_path_of("important = 1"), None);
        assert_eq!(import_path_of("print 1"), None);
    }

    #[test]
    fn 摊平之后行号还对得上() {
        let dir = std::env::temp_dir().join("cwtest_import_1");
        std::fs::create_dir_all(&dir).unwrap();
        let mut lib = std::fs::File::create(dir.join("util.cw")).unwrap();
        writeln!(lib, "fn 加一(x)\n    return x + 1").unwrap();
        drop(lib);

        let main = "import \"util.cw\"\nprint 加一(1)";
        let ex = expand("main.cw", main, Some(&dir)).unwrap();
        assert_eq!(ex.files.len(), 2);
        // 库被摊到了主文件第一行后面
        assert!(ex.source.contains("fn 加一(x)"));
        // 主文件第 2 行（print）在合并源码里是最后一行，反查回去还是第 2 行
        let last = ex.line_local.len() - 1;
        assert_eq!(ex.line_local[last], 2);
        assert_eq!(ex.line_file[last], 0);
        // 库自己的行认得自己
        assert_eq!(ex.line_file[1], 1);
        assert_eq!(ex.line_local[1], 1);
    }

    #[test]
    fn 找不到文件要报清楚() {
        let e = expand("main.cw", "import \"没有这个.cw\"", None).unwrap_err();
        assert!(e.message.contains("找不到库文件"), "{}", e.message);
    }
}
