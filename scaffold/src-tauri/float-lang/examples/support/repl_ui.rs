//! CLI 交互层：语法高亮行编辑 + 分章节帮助。
//!
//! 只用标准库：Windows 走 Win32 控制台 API，Unix 走 `stty` + termios 的原始字节。
//! 非终端（管道、重定向）自动退回逐行读取，不输出颜色和提示符。

use std::io::{self, IsTerminal, Write};

const PROMPT_WIDTH_FALLBACK: usize = 80;

// ───────────────────────────── 颜色 ─────────────────────────────

const RESET: &str = "\x1b[0m";
const C_PROMPT: &str = "\x1b[1;36m";
const C_KEYWORD: &str = "\x1b[1;35m";
const C_TYPE: &str = "\x1b[1;34m";
const C_BUILTIN: &str = "\x1b[1;33m";
const C_STRING: &str = "\x1b[31m";
const C_NUMBER: &str = "\x1b[32m";
const C_COMMENT: &str = "\x1b[90m";
const C_OPERATOR: &str = "\x1b[36m";
const C_METHOD: &str = "\x1b[96m";
const C_TITLE: &str = "\x1b[1;36m";
const C_DIM: &str = "\x1b[90m";

fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    io::stdout().is_terminal()
}

// ───────────────────────────── 语法高亮 ─────────────────────────────

const KEYWORDS: &[&str] = &[
    "if", "elif", "else", "while", "for", "in", "to", "step", "repeat", "forever",
    "break", "continue", "balo", "return", "stop", "wait", "until", "try", "catch",
    "finally", "throw", "import", "as", "class", "extends", "new", "self", "this",
    "super", "true", "false", "null", "and", "or", "not", "mod", "fn", "function",
    "func", "lazy", "out",
];

const TYPES: &[&str] = &["int", "float", "str", "bool", "list", "dict", "ptr", "addr", "var"];

const BUILTINS: &[&str] = &[
    "print", "ask", "len", "range", "add", "sub", "mul", "div", "pow", "sqrt", "abs",
    "floor", "ceil", "round", "min", "max", "rand", "random", "sin", "cos", "tan",
    "asin", "acos", "atan", "atan2", "log", "exp", "str", "int", "float", "bool",
    "type", "join", "char", "strlen", "upper", "lower", "trim", "split", "indexof",
    "substr", "dictallkey", "dictallval", "dictin", "listin", "contain", "readfile",
    "writefile", "appendfile", "hasfile", "now", "fetch", "sorted", "alloc", "free",
    "addr", "read", "write", "report", "json", "time", "os", "net", "re", "ui", "data",
];

const OPERATORS: &[&str] = &[
    "**=", "==", "!=", "<=", ">=", "&&", "||", "**", "++", "--", "+=", "-=", "*=", "/=",
    "%=", "//", "=>",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tok {
    Plain,
    Keyword,
    Type,
    Builtin,
    Str,
    Number,
    Comment,
    Operator,
    Method,
}

fn color_of(t: Tok) -> &'static str {
    match t {
        Tok::Plain => "",
        Tok::Keyword => C_KEYWORD,
        Tok::Type => C_TYPE,
        Tok::Builtin => C_BUILTIN,
        Tok::Str => C_STRING,
        Tok::Number => C_NUMBER,
        Tok::Comment => C_COMMENT,
        Tok::Operator => C_OPERATOR,
        Tok::Method => C_METHOD,
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic() || (c as u32) >= 0x80
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric() || (c as u32) >= 0x80
}

/// 给一行源码上色。返回的字符串只多了 ANSI 转义，字符内容一个不差。
pub fn highlight_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 32);
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];

        // 注释：/* 到行尾
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            push_token(&mut out, &chars[i..], Tok::Comment);
            break;
        }

        // 字符串（单双引号都行，尊重反斜杠转义）
        if c == '"' || c == '\'' {
            let quote = c;
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            push_token(&mut out, &chars[start..i], Tok::Str);
            continue;
        }

        // 数字：十进制、小数、0x/0b/0o、指数。整数在运行时是任意精度。
        if c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len() {
                let n = chars[i];
                if n.is_ascii_alphanumeric() || n == '.' || n == '_' {
                    i += 1;
                } else if (n == '+' || n == '-')
                    && i > start
                    && matches!(chars[i - 1], 'e' | 'E')
                {
                    i += 1;
                } else {
                    break;
                }
            }
            push_token(&mut out, &chars[start..i], Tok::Number);
            continue;
        }

        // 标识符 / 关键字 / 内建名
        if is_ident_start(c) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_continue(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if KEYWORDS.contains(&word.as_str()) {
                Tok::Keyword
            } else if TYPES.contains(&word.as_str()) {
                Tok::Type
            } else if BUILTINS.contains(&word.as_str()) {
                Tok::Builtin
            } else if chars[start] == 'f'
                && i < chars.len()
                && chars[i] == '{'
            {
                // f{"..."} 多片段拼接
                Tok::Builtin
            } else {
                // 点号后面的“名字(”当成方法/字段调用
                let mut k = start;
                while k > 0 && chars[k - 1].is_whitespace() {
                    k -= 1;
                }
                let after_dot = k > 0 && chars[k - 1] == '.';
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if after_dot && j < chars.len() && chars[j] == '(' {
                    Tok::Method
                } else {
                    Tok::Plain
                }
            };
            push_token(&mut out, &chars[start..i], kind);
            continue;
        }

        // 运算符
        let mut matched = false;
        for op in OPERATORS {
            let n = op.chars().count();
            if i + n <= chars.len() {
                let seg: String = chars[i..i + n].iter().collect();
                if seg == *op {
                    push_token(&mut out, &chars[i..i + n], Tok::Operator);
                    i += n;
                    matched = true;
                    break;
                }
            }
        }
        if matched {
            continue;
        }
        if "=+-*/%!<>&|^~?:.,;(){}[]".contains(c) {
            push_token(&mut out, &chars[i..i + 1], Tok::Operator);
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }
    out
}

fn push_token(out: &mut String, chars: &[char], kind: Tok) {
    let style = color_of(kind);
    if style.is_empty() {
        out.extend(chars.iter().copied());
    } else {
        out.push_str(style);
        out.extend(chars.iter().copied());
        out.push_str(RESET);
    }
}

// ───────────────────────────── 可视宽度 ─────────────────────────────

fn char_width(c: char) -> usize {
    let u = c as u32;
    if c == '\t' {
        return 4;
    }
    if c.is_control() || u == 0x200b || u == 0x200c || u == 0x200d || u == 0xfeff {
        return 0;
    }
    // 组合符号 / 变体选择符不占格
    if (0x0300..=0x036f).contains(&u)
        || (0x1ab0..=0x1aff).contains(&u)
        || (0x1dc0..=0x1dff).contains(&u)
        || (0x20d0..=0x20ff).contains(&u)
        || (0xfe00..=0xfe0f).contains(&u)
        || (0xfe20..=0xfe2f).contains(&u)
    {
        return 0;
    }
    // 东亚宽字符 + emoji
    if (0x1100..=0x115f).contains(&u)
        || (0x2e80..=0xa4cf).contains(&u) && u != 0x303f
        || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u)
        || (0xfe10..=0xfe19).contains(&u)
        || (0xfe30..=0xfe6f).contains(&u)
        || (0xff00..=0xff60).contains(&u)
        || (0xffe0..=0xffe6).contains(&u)
        || (0x1f300..=0x1faff).contains(&u)
        || (0x20000..=0x3fffd).contains(&u)
    {
        return 2;
    }
    1
}

fn text_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

// ───────────────────────────── 终端按键 ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    Tab,
    CtrlA,
    CtrlC,
    CtrlD,
    CtrlE,
    CtrlK,
    CtrlL,
    CtrlU,
    CtrlW,
    Esc,
    Unknown,
}

#[cfg(windows)]
mod raw_term {
    use super::{Key, PROMPT_WIDTH_FALLBACK};
    use std::collections::VecDeque;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Coord {
        x: i16,
        y: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SmallRect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ConsoleScreenBufferInfo {
        size: Coord,
        cursor: Coord,
        attributes: u16,
        window: SmallRect,
        max_window: Coord,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: *mut c_void, mode: u32) -> i32;
        fn ReadConsoleW(
            handle: *mut c_void,
            buf: *mut u16,
            count: u32,
            read: *mut u32,
            control: *mut c_void,
        ) -> i32;
        fn GetConsoleScreenBufferInfo(
            handle: *mut c_void,
            info: *mut ConsoleScreenBufferInfo,
        ) -> i32;
    }

    const STD_INPUT: u32 = (-10i32) as u32;
    const STD_OUTPUT: u32 = (-11i32) as u32;

    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    const ENABLE_WINDOW_INPUT: u32 = 0x0008;
    const ENABLE_MOUSE_INPUT: u32 = 0x0010;
    const ENABLE_QUICK_EDIT: u32 = 0x0040;
    const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

    pub struct RawTerm {
        h_in: *mut c_void,
        old_in: u32,
        queue: VecDeque<char>,
        high: Option<u16>,
    }

    impl RawTerm {
        pub fn new() -> Option<Self> {
            unsafe {
                let h_in = GetStdHandle(STD_INPUT);
                let h_out = GetStdHandle(STD_OUTPUT);
                if h_in.is_null() {
                    return None;
                }
                let mut old_in = 0u32;
                if GetConsoleMode(h_in, &mut old_in) == 0 {
                    return None;
                }

                let mut out_mode = 0u32;
                if GetConsoleMode(h_out, &mut out_mode) != 0 {
                    let _ = SetConsoleMode(h_out, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }

                let mut new_in = old_in | ENABLE_EXTENDED_FLAGS | ENABLE_VIRTUAL_TERMINAL_INPUT;
                new_in &= !(ENABLE_LINE_INPUT
                    | ENABLE_ECHO_INPUT
                    | ENABLE_PROCESSED_INPUT
                    | ENABLE_WINDOW_INPUT
                    | ENABLE_MOUSE_INPUT
                    | ENABLE_QUICK_EDIT);
                if SetConsoleMode(h_in, new_in) == 0 {
                    // 老系统不支持 VT 输入：退一步，至少关掉行缓冲和回显。
                    let fallback = (old_in | ENABLE_EXTENDED_FLAGS)
                        & !(ENABLE_LINE_INPUT
                            | ENABLE_ECHO_INPUT
                            | ENABLE_PROCESSED_INPUT
                            | ENABLE_WINDOW_INPUT
                            | ENABLE_MOUSE_INPUT
                            | ENABLE_QUICK_EDIT);
                    if SetConsoleMode(h_in, fallback) == 0 {
                        return None;
                    }
                }
                Some(Self {
                    h_in,
                    old_in,
                    queue: VecDeque::new(),
                    high: None,
                })
            }
        }

        pub fn cols(&self) -> usize {
            unsafe {
                let h_out = GetStdHandle(STD_OUTPUT);
                let mut info: ConsoleScreenBufferInfo = std::mem::zeroed();
                if GetConsoleScreenBufferInfo(h_out, &mut info) != 0 {
                    let w = info.window.right - info.window.left + 1;
                    if w > 0 {
                        return w as usize;
                    }
                    if info.size.x > 0 {
                        return info.size.x as usize;
                    }
                }
            }
            PROMPT_WIDTH_FALLBACK
        }

        fn read_char(&mut self) -> Option<char> {
            if let Some(c) = self.queue.pop_front() {
                return Some(c);
            }
            unsafe {
                loop {
                    let mut buf = [0u16; 16];
                    let mut n = 0u32;
                    let ok = ReadConsoleW(
                        self.h_in,
                        buf.as_mut_ptr(),
                        buf.len() as u32,
                        &mut n,
                        std::ptr::null_mut(),
                    );
                    if ok == 0 || n == 0 {
                        return None;
                    }
                    for &u in &buf[..n as usize] {
                        if let Some(h) = self.high.take() {
                            if (0xdc00..=0xdfff).contains(&u) {
                                let code =
                                    0x10000 + (((h as u32) - 0xd800) << 10) + ((u as u32) - 0xdc00);
                                if let Some(c) = char::from_u32(code) {
                                    self.queue.push_back(c);
                                }
                            } else {
                                self.queue.push_back('\u{fffd}');
                                self.push_unit(u);
                            }
                        } else if (0xd800..=0xdbff).contains(&u) {
                            self.high = Some(u);
                        } else {
                            self.queue.push_back(char::from_u32(u as u32).unwrap_or('\u{fffd}'));
                        }
                    }
                    if let Some(c) = self.queue.pop_front() {
                        return Some(c);
                    }
                    // 只有半个代理对时继续读下一批
                }
            }
        }

        fn push_unit(&mut self, u: u16) {
            if (0xd800..=0xdbff).contains(&u) {
                self.high = Some(u);
            } else {
                self.queue.push_back(char::from_u32(u as u32).unwrap_or('\u{fffd}'));
            }
        }

        fn esc_key(&mut self) -> Key {
            let next = match self.read_char() {
                Some(c) => c,
                None => return Key::Esc,
            };
            if next == '[' || next == 'O' {
                let mut seq = String::new();
                loop {
                    let Some(c) = self.read_char() else {
                        return Key::Unknown;
                    };
                    seq.push(c);
                    if c.is_ascii_alphabetic() || c == '~' || seq.len() > 24 {
                        break;
                    }
                }
                map_csi(&seq)
            } else {
                // Esc 后不是序列：把那个字符还回去，Esc 单独算
                self.queue.push_front(next);
                Key::Esc
            }
        }

        pub fn read_key(&mut self) -> Option<Key> {
            let c = self.read_char()?;
            Some(match c {
                '\r' | '\n' => Key::Enter,
                '\x7f' | '\x08' => Key::Backspace,
                '\t' => Key::Tab,
                '\x01' => Key::CtrlA,
                '\x03' => Key::CtrlC,
                '\x04' => Key::CtrlD,
                '\x05' => Key::CtrlE,
                '\x0b' => Key::CtrlK,
                '\x0c' => Key::CtrlL,
                '\x15' => Key::CtrlU,
                '\x17' => Key::CtrlW,
                '\x1b' => self.esc_key(),
                _ if c.is_control() => Key::Unknown,
                _ => Key::Char(c),
            })
        }
    }

    impl Drop for RawTerm {
        fn drop(&mut self) {
            unsafe {
                let _ = SetConsoleMode(self.h_in, self.old_in);
            }
        }
    }

    fn map_csi(seq: &str) -> Key {
        match seq {
            "A" => Key::Up,
            "B" => Key::Down,
            "C" => Key::Right,
            "D" => Key::Left,
            "H" | "1~" | "7~" => Key::Home,
            "F" | "4~" | "8~" => Key::End,
            "3~" => Key::Delete,
            "Z" => Key::Tab,
            _ if seq.ends_with('A') => Key::Up,
            _ if seq.ends_with('B') => Key::Down,
            _ if seq.ends_with('C') => Key::Right,
            _ if seq.ends_with('D') => Key::Left,
            _ if seq.ends_with('H') => Key::Home,
            _ if seq.ends_with('F') => Key::End,
            _ => Key::Unknown,
        }
    }

    impl RawTerm {
        pub fn flush_state(&mut self) {}
    }
}

#[cfg(unix)]
mod raw_term {
    use super::{Key, PROMPT_WIDTH_FALLBACK};
    use std::ffi::{c_int, c_ulong};
    use std::io::{self, Read};
    use std::process::Command;

    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }

    extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }

    #[cfg(target_os = "linux")]
    const TIOCGWINSZ: c_ulong = 0x5413;
    #[cfg(not(target_os = "linux"))]
    const TIOCGWINSZ: c_ulong = 0x40087468;

    pub struct RawTerm {
        saved: String,
    }

    impl RawTerm {
        pub fn new() -> Option<Self> {
            let saved = Command::new("stty").arg("-g").output().ok()?;
            if !saved.status.success() {
                return None;
            }
            let saved = String::from_utf8(saved.stdout).ok()?.trim().to_string();
            if saved.is_empty()
                || !saved
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == ':')
            {
                return None;
            }
            // 不开 -opost：输出换行仍要正常回卷；关掉行缓冲/回显/信号键即可。
            let ok = Command::new("stty")
                .args(["-icanon", "-echo", "-isig", "min", "1", "time", "0"])
                .status()
                .ok()?
                .success();
            if !ok {
                return None;
            }
            Some(Self { saved })
        }

        pub fn cols(&self) -> usize {
            unsafe {
                let mut ws = Winsize {
                    ws_row: 0,
                    ws_col: 0,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                if ioctl(0, TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
                    return ws.ws_col as usize;
                }
            }
            PROMPT_WIDTH_FALLBACK
        }

        fn read_char(&mut self) -> Option<char> {
            let mut buf = [0u8; 1];
            let n = io::stdin().read(&mut buf).ok()?;
            if n == 0 {
                return None;
            }
            let b0 = buf[0];
            if b0 < 0x80 {
                return Some(b0 as char);
            }
            let need = if b0 & 0xe0 == 0xc0 {
                2
            } else if b0 & 0xf0 == 0xe0 {
                3
            } else if b0 & 0xf8 == 0xf0 {
                4
            } else {
                return Some('\u{fffd}');
            };
            let mut bytes = vec![b0];
            for _ in 1..need {
                let mut one = [0u8; 1];
                let n = io::stdin().read(&mut one).ok()?;
                if n == 0 {
                    break;
                }
                bytes.push(one[0]);
            }
            let s = String::from_utf8_lossy(&bytes);
            s.chars().next()
        }

        fn esc_key(&mut self) -> Key {
            let next = match self.read_char() {
                Some(c) => c,
                None => return Key::Esc,
            };
            if next == '[' || next == 'O' {
                let mut seq = String::new();
                loop {
                    let Some(c) = self.read_char() else {
                        return Key::Unknown;
                    };
                    seq.push(c);
                    if c.is_ascii_alphabetic() || c == '~' || seq.len() > 24 {
                        break;
                    }
                }
                map_csi(&seq)
            } else {
                Key::Esc
            }
        }

        pub fn read_key(&mut self) -> Option<Key> {
            let c = self.read_char()?;
            Some(match c {
                '\r' | '\n' => Key::Enter,
                '\x7f' | '\x08' => Key::Backspace,
                '\t' => Key::Tab,
                '\x01' => Key::CtrlA,
                '\x03' => Key::CtrlC,
                '\x04' => Key::CtrlD,
                '\x05' => Key::CtrlE,
                '\x0b' => Key::CtrlK,
                '\x0c' => Key::CtrlL,
                '\x15' => Key::CtrlU,
                '\x17' => Key::CtrlW,
                '\x1b' => self.esc_key(),
                _ if c.is_control() => Key::Unknown,
                _ => Key::Char(c),
            })
        }
    }

    impl Drop for RawTerm {
        fn drop(&mut self) {
            let _ = Command::new("stty").arg(&self.saved).status();
        }
    }

    fn map_csi(seq: &str) -> Key {
        match seq {
            "A" => Key::Up,
            "B" => Key::Down,
            "C" => Key::Right,
            "D" => Key::Left,
            "H" | "1~" | "7~" => Key::Home,
            "F" | "4~" | "8~" => Key::End,
            "3~" => Key::Delete,
            "Z" => Key::Tab,
            _ if seq.ends_with('A') => Key::Up,
            _ if seq.ends_with('B') => Key::Down,
            _ if seq.ends_with('C') => Key::Right,
            _ if seq.ends_with('D') => Key::Left,
            _ if seq.ends_with('H') => Key::Home,
            _ if seq.ends_with('F') => Key::End,
            _ => Key::Unknown,
        }
    }

    impl RawTerm {
        pub fn flush_state(&mut self) {}
    }
}

// ───────────────────────────── 行编辑器 ─────────────────────────────

struct TtyEditor {
    history: Vec<String>,
    cols: usize,
    color: bool,
    last_cursor_row: usize,
}

impl TtyEditor {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            cols: PROMPT_WIDTH_FALLBACK,
            color: color_enabled(),
            last_cursor_row: 0,
        }
    }

    fn prompt_text(&self, prompt: &str) -> String {
        if self.color {
            format!("{C_PROMPT}{prompt}{RESET}")
        } else {
            prompt.to_string()
        }
    }

    fn draw(&mut self, prompt: &str, chars: &[char], cursor: usize, highlight: bool) -> io::Result<()> {
        let mut out = String::new();
        if self.last_cursor_row > 0 {
            out.push_str(&format!("\x1b[{}A", self.last_cursor_row));
        }
        out.push_str("\r\x1b[0J");
        out.push_str(&self.prompt_text(prompt));

        let line: String = chars.iter().collect();
        if highlight && self.color {
            out.push_str(&highlight_line(&line));
        } else {
            out.push_str(&line);
        }

        let total = text_width(prompt) + text_width(&line);
        let before: String = chars[..cursor.min(chars.len())].iter().collect();
        let desired = text_width(prompt) + text_width(&before);
        let cols = self.cols.max(1);

        if desired != total {
            let end_row = total.saturating_sub(1) / cols;
            let des_row = desired.saturating_sub(1) / cols;
            if end_row > des_row {
                out.push_str(&format!("\x1b[{}A", end_row - des_row));
            }
            out.push('\r');
            let des_col = desired % cols;
            if des_col > 0 {
                out.push_str(&format!("\x1b[{}C", des_col));
            }
            self.last_cursor_row = des_row;
        } else {
            self.last_cursor_row = total.saturating_sub(1) / cols;
        }

        let mut stdout = io::stdout();
        stdout.write_all(out.as_bytes())?;
        stdout.flush()
    }

    fn set_buffer(&self, chars: &mut Vec<char>, cursor: &mut usize, s: &str) {
        *chars = s.chars().collect();
        *cursor = chars.len();
    }

    fn read_line(&mut self, prompt: &str, highlight: bool) -> Option<String> {
        let Some(mut raw) = raw_term::RawTerm::new() else {
            return plain_read_line(prompt);
        };
        self.cols = raw.cols().max(20);
        self.last_cursor_row = 0;

        let mut chars: Vec<char> = Vec::new();
        let mut cursor = 0usize;
        let mut hist_pos: Option<usize> = None;
        let mut draft = String::new();

        let _ = self.draw(prompt, &chars, cursor, highlight);
        loop {
            let key = match raw.read_key() {
                Some(k) => k,
                None => {
                    if chars.is_empty() {
                        let _ = writeln!(io::stdout());
                        return None;
                    }
                    let _ = self.draw(prompt, &chars, chars.len(), highlight);
                    let _ = writeln!(io::stdout());
                    let s: String = chars.iter().collect();
                    return Some(s);
                }
            };

            match key {
                Key::Enter => {
                    let _ = self.draw(prompt, &chars, chars.len(), highlight);
                    let _ = writeln!(io::stdout());
                    let s: String = chars.iter().collect();
                    if !s.trim().is_empty() && self.history.last() != Some(&s) {
                        self.history.push(s.clone());
                        if self.history.len() > 500 {
                            self.history.remove(0);
                        }
                    }
                    self.last_cursor_row = 0;
                    return Some(s);
                }
                Key::Char(c) => {
                    chars.insert(cursor, c);
                    cursor += 1;
                    hist_pos = None;
                }
                Key::Tab => {
                    for _ in 0..4 {
                        chars.insert(cursor, ' ');
                        cursor += 1;
                    }
                    hist_pos = None;
                }
                Key::Backspace => {
                    if cursor > 0 {
                        cursor -= 1;
                        chars.remove(cursor);
                        hist_pos = None;
                    }
                }
                Key::Delete => {
                    if cursor < chars.len() {
                        chars.remove(cursor);
                        hist_pos = None;
                    }
                }
                Key::Left => cursor = cursor.saturating_sub(1),
                Key::Right => {
                    if cursor < chars.len() {
                        cursor += 1;
                    }
                }
                Key::Home | Key::CtrlA => cursor = 0,
                Key::End | Key::CtrlE => cursor = chars.len(),
                Key::CtrlU => {
                    chars.drain(..cursor);
                    cursor = 0;
                    hist_pos = None;
                }
                Key::CtrlK => {
                    chars.truncate(cursor);
                    hist_pos = None;
                }
                Key::CtrlW => {
                    while cursor > 0 && chars[cursor - 1].is_whitespace() {
                        cursor -= 1;
                        chars.remove(cursor);
                    }
                    while cursor > 0 && !chars[cursor - 1].is_whitespace() {
                        cursor -= 1;
                        chars.remove(cursor);
                    }
                    hist_pos = None;
                }
                Key::CtrlL => {
                    let _ = io::stdout().write_all(b"\x1b[2J\x1b[H");
                    self.last_cursor_row = 0;
                }
                Key::CtrlC => {
                    let _ = self.draw(prompt, &chars, chars.len(), highlight);
                    let _ = io::stdout().write_all(b"^C\n");
                    let _ = io::stdout().flush();
                    self.last_cursor_row = 0;
                    return Some(String::new());
                }
                Key::CtrlD => {
                    if chars.is_empty() {
                        let _ = writeln!(io::stdout());
                        self.last_cursor_row = 0;
                        return None;
                    }
                    if cursor < chars.len() {
                        chars.remove(cursor);
                        hist_pos = None;
                    }
                }
                Key::Up => {
                    if !self.history.is_empty() {
                        if hist_pos.is_none() {
                            draft = chars.iter().collect();
                            hist_pos = Some(self.history.len());
                        }
                        let p = hist_pos.unwrap_or(0);
                        if p > 0 {
                            let np = p - 1;
                            hist_pos = Some(np);
                            let s = self.history[np].clone();
                            self.set_buffer(&mut chars, &mut cursor, &s);
                        }
                    }
                }
                Key::Down => {
                    if let Some(p) = hist_pos {
                        if p + 1 < self.history.len() {
                            let np = p + 1;
                            hist_pos = Some(np);
                            let s = self.history[np].clone();
                            self.set_buffer(&mut chars, &mut cursor, &s);
                        } else {
                            hist_pos = None;
                            let d = draft.clone();
                            self.set_buffer(&mut chars, &mut cursor, &d);
                        }
                    }
                }
                Key::Esc | Key::Unknown => {}
            }
            let _ = self.draw(prompt, &chars, cursor, highlight);
            let _ = raw.flush_state();
        }
    }
}

fn plain_read_line(_prompt: &str) -> Option<String> {
    let mut s = String::new();
    match io::stdin().read_line(&mut s) {
        Ok(0) => None,
        Ok(_) => {
            while s.ends_with('\n') || s.ends_with('\r') {
                s.pop();
            }
            Some(s)
        }
        Err(_) => None,
    }
}

// ───────────────────────────── 输入抽象 ─────────────────────────────

pub struct InputReader {
    tty: bool,
    editor: Option<TtyEditor>,
}

impl InputReader {
    pub fn new() -> Self {
        let tty = io::stdin().is_terminal() && io::stdout().is_terminal();
        let editor = if tty { Some(TtyEditor::new()) } else { None };
        Self { tty, editor }
    }

    pub fn is_tty(&self) -> bool {
        self.tty
    }

    pub fn read_line(&mut self, prompt: &str, highlight: bool) -> Option<String> {
        if let Some(ed) = self.editor.as_mut() {
            ed.read_line(prompt, highlight)
        } else {
            plain_read_line(prompt)
        }
    }
}

// ───────────────────────────── 帮助文档 ─────────────────────────────

struct Chapter {
    title: &'static str,
    body: &'static str,
}

const CHAPTERS: &[Chapter] = &[
    Chapter {
        title: "1  快速开始与 REPL",
        body: r#"
Copper 是一门强调“一行一句、缩进成块”的脚本语言，源文件后缀是 .co，旧 .cw 也能跑。

最小程序
  print("你好，Copper")

启动方式
  co                       进入交互模式（等价于 copper-run --repl）
  co hello.co              运行脚本
  co run hello.co          同上
  co help                  打开这份分章节帮助
  co --version             查看版本

REPL 里可以直接敲表达式，结果自动打印：
  >>> 1 + 2
  3
  >>> "ab" + "cd"
  abcd

变量、函数、类会跨行保留。写错的那一行不会进入状态，改对再回车即可。
REPL 目前按“一行一条语句”执行；多行缩进块、跨行函数体请写进 .co 文件再运行。

REPL 命令
  help / ?                 分章节帮助
  help 3                   直接看第 3 章
  help all                 一次打印全部章节
  exit / quit / q          退出

输入有语法高亮：关键字、类型、内建函数、字符串、数字、注释、运算符分色。
不想要颜色就设环境变量 NO_COLOR=1。

行编辑键
  ← →                     移动光标
  Home / End              行首 / 行尾
  ↑ ↓                     翻历史命令
  Backspace / Delete      删除字符
  Ctrl+A / Ctrl+E         行首 / 行尾
  Ctrl+U / Ctrl+K         删到行首 / 删到行尾
  Ctrl+W                  删前一个词
  Ctrl+L                  清屏
  Ctrl+C                  放弃当前行
  Ctrl+D                  空行时退出
"#,
    },
    Chapter {
        title: "2  语法骨架与字面量",
        body: r#"
一行一句
  回车结束一条语句，行尾分号可写可不写；不支持一行多句。

缩进成块
  比上一行更深的缩进属于上一行的块，退回去就结束。
  推荐 4 个空格；Tab 会按 4 个空格展开，别混用。
  也可以写 { 和 }，但块体必须另起一行，不能 if (x) { print(1) } 挤一行。

注释
  /* 行注释，从 /* 到行尾，不用闭合
  /** 块注释，必须顶格开始，可跨行，写到 */ 结束
  块注释没闭合只给提醒，会一直注释到文件末尾。

字符串
  单双引号都行："文本" / '文本'
  支持转义：\" \\ \n \t
  插值：print("得分：{score}")，花括号里可写表达式
  不想插值就写 \{ 和 \}
  多片段拼接：print(f{"姓名：", name, "，得分：", score})

数字
  整数：10、-3、0x1f、0b101
  小数：3.14、1e9
  整数是任意精度：2 ** 1000000 会完整算出来，不会变 Infinity。
  浮点是 f64，极大指数仍可能是 Infinity，这是浮点本身的边界。

标识符
  英文、中文、下划线都能做变量名：int 血量 = 100
  关键字不能当变量名；点号后的成员名不受关键字限制。

续行
  [ { ( 没闭合时会自然续到下一行，字典和长参数表可以分多行写。
"#,
    },
    Chapter {
        title: "3  变量、类型与值",
        body: r#"
声明
  int age = 18
  float price = 9.9
  str name = "小方"
  bool ready = true
  list nums = [1, 2]
  dict hero = {"hp": 100}
  ptr p = &age

类型只是标注，不做强制转换：int x = 1.5 仍然是 1.5。
类型可以省：score = 100。

空值
  null 是唯一空值；nil / none 已取消。
  字典缺键返回 null；变量没定义则直接报错。

布尔
  true / false
  条件里 0、空字符串、空列表、null 按假处理，其余按真处理。

转换
  int(x) float(x) str(x) bool(x)
  int("42")、str(3.5) 都能用。

多返回值 / 解构
  return lo, hi            打包成列表返回
  int low, int high = f()   一次接住
  low, high = f()

特殊变量（老写法，仍兼容）
  ans            ask 的回答
  __last_calc    add/sub/mul/div 等计算结果
  __last_cmp     比较/逻辑结果
  __last_len     len 的结果
  新代码建议直接用表达式返回值，不必依赖这些变量。

作用域
  函数里声明的变量是局部的；函数外看不见。
  要改外面的变量，不要重新声明，直接赋值。
"#,
    },
    Chapter {
        title: "4  运算符与表达式",
        body: r#"
算术
  +  -  *  /       加减乘除；/ 结果可能是小数
  //                整除
  %  或  a mod b   取余
  **                乘方
  ++  --            自增自减
  += -= *= /= %= **= 复合赋值

整数不溢出
  整数加减乘和乘方在超出 i64 后自动转任意精度整数。
  2 ** 100000000 会输出完整 3010 万位十进制，慢但一定算完。

字符串
  + 拼接："第 " + 3 + " 关"
  字符串里 {表达式} 插值
  f{...} 多片段拼接，自动转文本

比较
  == != < > <= >=
  数字和数字比、字符串和字符串比；类型不匹配会报错。

逻辑
  && || !            短路
  and or not         等价关键字
  ?:                 三元：条件 ? 值1 : 值2

下标与成员
  xs[0]              列表下标，负数从末尾数
  d["键"] / d.键      字典取值
  obj.field          对象字段
  obj.method(args)   方法调用
  s.trim().upper()   链式调用

指针
  &x                 取地址
  *p                 解引用
  p[0]、p + 1        按格偏移
  **pp               多级解引用

优先级（低到高）
  ?: → || → && → | → ^ → & → == != → < > <= >= → + - → * / % // → **
  拿不准就加括号，括号最优先。
"#,
    },
    Chapter {
        title: "5  控制流",
        body: r#"
条件
  if (hp > 60)
      print("良好")
  elif (hp > 20)
      print("受伤")
  else
      print("危险")

  括号可省：if hp > 0
  if 后不写条件等价于 if (true)，可当占位。

按次数循环
  for i in range(5)          0 1 2 3 4
  for i in range(2, 5)       2 3 4，含头不含尾
  for i in range(10, 0, -2)  10 8 6 4 2

含两端循环
  for i = 1 to 5             1 2 3 4 5
  for i = 10 to 1 step -3    10 7 4 1

遍历
  for x in 列表
  for ch in "abc"
  for k in 字典              默认给键
  for k, v in 字典           键和值

条件循环
  while (n < 3)
      n++

重复 / 死循环
  repeat(3)
      print("再来一次")

  forever
      ...
      if (done)
          break

跳出
  break      跳出整个循环
  continue   跳过本轮（balo 是旧别名）
  stop       停止整个脚本

等待
  wait 0.5            等半秒
  wait until (ready)  条件为假时重复块内语句

注意：语言层不设循环次数、递归深度、输出大小或执行时长上限。
真死循环就自己按 Ctrl+C / 关进程；这是使用者的决定，不是解释器替你拦。
"#,
    },
    Chapter {
        title: "6  函数、闭包与高阶函数",
        body: r#"
定义
  function add(a, b)
      return a + b

  fn add(a, b) = a + b      也可以
  fn add(a, b)
      return a + b

参数
  默认值：function greet(name, greeting = "你好")
  默认值可以引用前面的参数：function area(w, h = w)
  lazy 参数：按需求值，用来实现自定义短路逻辑
  function AND2(bool x, lazy bool y)

返回
  没写 return 返回 null
  return a, b 打包成列表
  函数可以先调用后定义（顶层统一注册）

递归
  function fact(n)
      if (n <= 1)
          return 1
      return n * fact(n - 1)

  递归不设人为层数上限，栈够就能跑；栈耗尽由进程退出。

嵌套与闭包
  fn 计数器()
      int n = 0
      return fn()
          n += 1
          return n

  内层函数记住外层变量；每次调用外层函数都会得到独立的一份状态。

内联函数
  fn(x) return x * 2
  函数体只有一条 return 表达式时最合适。

高阶函数
  data.map / filter / reduce / groupby / index_by / any / all / find / find_index
  回调既可以是 fn(...)，也可以是已定义的函数名。
  any / all 短路。

作用域
  函数局部变量外面看不见；要改全局变量直接赋值，不要重新声明。
"#,
    },
    Chapter {
        title: "7  列表、字典与字符串",
        body: r#"
列表
  list xs = [3, 1, 2]
  xs[0]          3
  xs[-1]         2
  xs[0] = 9
  len(xs)

  常用方法
    xs.push(v)          末尾追加
    xs.pop()            取出末尾
    xs.insert(i, v)     插入
    xs.remove(i)        按下标删除
    xs.sort()           原地排序
    xs.reverse()
    xs.slice(a, b)
    xs.join("-")
    xs.contains(v)
    xs.sum()
    xs.clear()

  旧指令仍兼容：
    add 4 to xs
    insert 0 to xs at 0
    replace xs 0 with 7
    del xs 2

字典
  dict d = {"a": 1, "b": 2}
  d["a"] / d.a
  d["缺键"]       返回 null，不报错
  d["新键"] = 3
  data.require(d, "键")     缺键当场报错
  data.get(d, "键", 0)      缺键给默认值

  方法：keys values items has remove get set len

遍历
  for x in xs
  for k in d
  for k, v in d

字符串
  len(s)
  s.upper() s.lower() s.trim()
  s.split(",") s.replace("a", "b")
  s.find("x")          找不到 -1
  s.contains("x")
  s.starts_with("x") s.ends_with("x")
  s.substr(a, n)
  s.repeat(n)
  s.to_int() s.to_float()

  字符串可以用 + 拼接，也可以 {表达式} 插值、f{...} 拼片段。

容器不设元素数量、字符串长度、打印深度上限；内存不够由系统处理。
"#,
    },
    Chapter {
        title: "8  类、异常、模块与指针",
        body: r#"
类
  class Hero
      str name = ""
      int hp = 100

      function init(name, hp)
          self.name = name
          self.hp = hp

      function hurt(damage)
          hp -= damage
          if (hp < 0)
              hp = 0
          return hp

      function info()
          return self.name + " 剩余 " + hp

  Hero h = new Hero("阿方", 120)
  h.hurt(30)
  print(h.info())

  字段在类体里声明；方法里可以直接写字段名，也可以 self.field。
  方法之间直接调用，不必写 self.。
  构造方法是 init，new 类名(...) 时自动调用。

继承
  class Dog extends Animal
      function init(name, breed)
          super(name)
          self.breed = breed

      function speak()
          return name + "：汪！"

  子类没写 init 就沿用父类；super(...) 只能用在子类方法里。

异常
  try
      throw "出错了"
  catch e
      print(e.message)
      print(e.line)
      print(e.file)
  finally
      print("一定执行")

  catch 变量是字典：message / line / file。
  只写 finally 也合法；没人接的错误会打印并终止脚本。

模块
  import "lib.co"       按文件
  import "tools"        按目录，找 tools/main.co
  import data           标准库
  导入在运行到这一行时执行，循环导入只执行一次。

标准库
  data    排序、统计、去重、切片、高阶函数
  json    parse / stringify / pretty / valid / try_parse
  time    format / parse / iso / diff / mark
  random  rand / float / choice / shuffle / sample / gauss
  re      findall / replace / split / test / escape
  os      文件、目录、路径、环境变量、runcmd
  net     fetch / post / download / request / encode
  ui      bar / progress / table / color / title / clear

指针与手动内存
  int x = 10
  ptr p = &x
  *p = 42
  ptr block = alloc(3)
  block[0] = 7
  free(block)

  &列表[i] 指向那一格；指针可偏移、可迭代、可多级解引用。
  函数返回局部变量地址会得到悬空指针，解引用时明确报错。
  alloc 的内存要自己 free；列表里的地址不能 free。
"#,
    },
    Chapter {
        title: "9  CLI、排错与运行限制",
        body: r#"
CLI
  co hello.co [输入文件]       用 COVM 解释器运行
  co run hello.co              同上
  co build hello.co -o out.exe 用 CO AOT 编原生程序
  co check hello.co            只做静态检查
  co ast / json / stats hello.co
  co env                       环境详情
  co self build hello.co       强制用自举编译器
  co help                      这份分章节帮助
  co --version                 版本

copper-run 直接用法
  copper-run hello.co
  copper-run --repl
  copper-run --help

AOT 与 VM
  F5 / co 默认走 VM，启动快。
  co build 走 AOT，把脚本翻成 Rust 再编 exe，程序启动后不依赖解释器。
  两者语义一致；AOT 对极少数特性会明确报“暂不支持”并回落 VM。

常见错误
  变量「x」没有定义       拼写错，或还没赋值
  下标 3 超出范围          下标从 0 开始
  print 后面多了内容       通常是终端粘行；本版 REPL 已清洗残留
  编译错误：第 N 行…       看行号与列号，错误行不会进入 REPL 状态

运行限制
  递归深度：无
  调用深度：无
  表达式/块嵌套：无
  import 层数：无
  列表/字典元素数：无
  字符串长度：无
  输出字节数：无
  执行步数：无
  MCP run/test 超时：默认 0（不限）

  整数是任意精度，能硬算就硬算；允许慢、允许吃内存。
  电脑卡住时由使用者自己 Ctrl+C 或结束进程，解释器不再替你设人为上限。

REPL 的一个已知设计
  它把历史输入攒成源码，每次整段重跑，只显示新增输出。
  所以执行过 2 ** 100000000 这种超大结果后，下一行会重新算一遍。
  要避免这种等待，把大计算写进文件单独跑。
"#,
    },
];

fn chapter_count() -> usize {
    CHAPTERS.len()
}

pub fn print_all_help() {
    for (i, ch) in CHAPTERS.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{}", ch.title);
        println!("{}", "─".repeat(text_width(ch.title).min(60)));
        print!("{}", ch.body.trim_start_matches('\n'));
        if !ch.body.ends_with('\n') {
            println!();
        }
    }
}

fn clear_screen() {
    let _ = io::stdout().write_all(b"\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
}

fn print_chapter(i: usize, reader: &mut InputReader) {
    if i >= CHAPTERS.len() {
        return;
    }
    if reader.is_tty() {
        clear_screen();
    }
    let ch = &CHAPTERS[i];
    println!("{}", paint(C_TITLE, &ch.title));
    println!("{}", "─".repeat(text_width(ch.title).min(60)));
    print!("{}", ch.body.trim_start_matches('\n'));
    if !ch.body.ends_with('\n') {
        println!();
    }
}

fn paint(code: &str, text: &str) -> String {
    if color_enabled() {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn print_menu() {
    println!("{}", paint(C_TITLE, "Copper 完整帮助"));
    println!("{}", "─".repeat(48));
    for ch in CHAPTERS {
        println!("  {}", ch.title);
    }
    println!();
    println!("{}", paint(C_DIM, "输入 1-9 阅读章节；a 查看全部；回车或 q 返回 REPL。"));
}

/// `arg` 为 None 时进入菜单；为数字时直接打开章节；为 `all` 时全打印。
pub fn help_command(reader: &mut InputReader, arg: Option<&str>) {
    let arg = arg.map(|s| s.trim()).unwrap_or("");
    if !reader.is_tty() {
        print_all_help();
        return;
    }
    match arg {
        "" => help_menu(reader),
        "all" | "全部" => {
            clear_screen();
            print_all_help();
            println!();
            let _ = reader.read_line("按回车返回 REPL > ", false);
        }
        other => {
            if let Ok(n) = other.parse::<usize>() {
                if (1..=chapter_count()).contains(&n) {
                    print_chapter(n - 1, reader);
                    println!();
                    let _ = reader.read_line("回车返回目录，q 返回 REPL > ", false);
                    return;
                }
            }
            println!("没有这一章：{}（可用 1-9 或 all）", other);
            help_menu(reader);
        }
    }
}

fn help_menu(reader: &mut InputReader) {
    if !reader.is_tty() {
        print_all_help();
        return;
    }
    loop {
        print_menu();
        let choice = match reader.read_line("章节 1-9 > ", false) {
            Some(s) => s.trim().to_string(),
            None => return,
        };
        if choice.is_empty() || choice == "q" || choice == "quit" || choice == "退出" {
            return;
        }
        if choice == "a" || choice == "all" || choice == "全部" {
            clear_screen();
            print_all_help();
            println!();
            let _ = reader.read_line("按回车返回目录，q 返回 REPL > ", false);
            continue;
        }
        match choice.parse::<usize>() {
            Ok(n) if (1..=chapter_count()).contains(&n) => {
                clear_screen();
                print_chapter(n - 1, reader);
                println!();
                let back = reader.read_line("回车返回目录，q 返回 REPL > ", false);
                if matches!(back.as_deref().map(|s| s.trim()), Some("q") | Some("quit") | Some("退出")) {
                    return;
                }
            }
            _ => println!("请输入 1-9，或 q 返回。"),
        }
    }
}

// ───────────────────────────── 测试 ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut esc = false;
        for c in s.chars() {
            if c == '\x1b' {
                esc = true;
            } else if esc {
                if c.is_ascii_alphabetic() {
                    esc = false;
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn highlighting_preserves_text() {
        let cases = [
            "if (x > 0) { print(\"你好\") }",
            "int 血量 = 100 /* 注释",
            "2 ** 1000000",
            "fn add(a, b) return a + b",
            "'单引号' + f{\"x\", y}",
        ];
        for c in cases {
            assert_eq!(strip_ansi(&highlight_line(c)), c);
        }
    }

    #[test]
    fn chinese_is_wide() {
        assert_eq!(text_width("中文"), 4);
        assert_eq!(text_width("abc"), 3);
    }

    #[test]
    fn all_chapters_have_titles_and_bodies() {
        assert_eq!(chapter_count(), 9);
        for ch in CHAPTERS {
            assert!(!ch.title.is_empty());
            assert!(ch.body.len() > 200, "{} 太短", ch.title);
        }
    }
}
