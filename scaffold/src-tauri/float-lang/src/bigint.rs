//! 任意精度整数：给 Copper 的整数运算兜底。
//!
//! 设计取向和 limits.rs 相反：**不设位数上限**，算得慢也算完，绝不在
//! i64 边界把结果退化成 f64（那会得到 Infinity / 丢精度）。
//!
//! 表示：符号 + 以 10^9 为基的小端 limb 数组。选十进制基是为了让
//! `to_string()` 直接拼 limb，不需要再做一次 O(n²) 的进制转换。
//! 乘法：小规模 schoolbook，大规模 Karatsuba；这样 2^100000000 这种
//! 三千万位的数也能在可接受的时间内硬算出来，而不是直接报「太大」。
//!
//! 只实现语言里真正需要的整数运算：加、减、乘、幂、比较、转 i64/f64。
//! 除法/取模仍由调用方决定（整数能整除就整除，否则走浮点语义）。

use std::cmp::Ordering;
use std::fmt;

const BASE: u64 = 1_000_000_000;
const BASE_U32: u32 = 1_000_000_000;
/// 低于这个长度用 schoolbook，避免 Karatsuba 的递归开销。
const KARATSUBA_CUTOFF: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct BigInt {
    neg: bool,
    /// 小端：limbs[0] 是个位段，每个 limb < 10^9。
    limbs: Vec<u32>,
}

impl BigInt {
    pub fn zero() -> Self {
        BigInt { neg: false, limbs: Vec::new() }
    }

    pub fn one() -> Self {
        BigInt { neg: false, limbs: vec![1] }
    }

    pub fn from_i64(v: i64) -> Self {
        if v == 0 {
            return Self::zero();
        }
        let neg = v < 0;
        let mut n = v.unsigned_abs();
        let mut limbs = Vec::new();
        while n > 0 {
            limbs.push((n % BASE) as u32);
            n /= BASE;
        }
        BigInt { neg, limbs }
    }

    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.neg && !self.is_zero()
    }

    pub fn sign(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.neg {
            -1
        } else {
            1
        }
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.neg = false;
        }
    }

    pub fn cmp_abs(&self, other: &Self) -> Ordering {
        match self.limbs.len().cmp(&other.limbs.len()) {
            Ordering::Equal => {
                for i in (0..self.limbs.len()).rev() {
                    match self.limbs[i].cmp(&other.limbs[i]) {
                        Ordering::Equal => {}
                        o => return o,
                    }
                }
                Ordering::Equal
            }
            o => o,
        }
    }

    pub fn cmp(&self, other: &Self) -> Ordering {
        match (self.sign(), other.sign()) {
            (a, b) if a != b => a.cmp(&b),
            (0, 0) => Ordering::Equal,
            (1, 1) => self.cmp_abs(other),
            (-1, -1) => other.cmp_abs(self),
            _ => unreachable!(),
        }
    }

    fn add_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
        let n = a.len().max(b.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let x = *a.get(i).unwrap_or(&0) as u64;
            let y = *b.get(i).unwrap_or(&0) as u64;
            let s = x + y + carry;
            out.push((s % BASE) as u32);
            carry = s / BASE;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        out
    }

    /// a - b，调用前保证 |a| >= |b|。
    fn sub_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i64;
        for i in 0..a.len() {
            let x = a[i] as i64;
            let y = *b.get(i).unwrap_or(&0) as i64;
            let mut d = x - y - borrow;
            if d < 0 {
                d += BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(d as u32);
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    pub fn add(&self, other: &Self) -> Self {
        if self.is_zero() {
            return other.clone();
        }
        if other.is_zero() {
            return self.clone();
        }
        if self.neg == other.neg {
            let mut r = BigInt { neg: self.neg, limbs: Self::add_abs(&self.limbs, &other.limbs) };
            r.normalize();
            return r;
        }
        match self.cmp_abs(other) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => {
                let mut r = BigInt { neg: self.neg, limbs: Self::sub_abs(&self.limbs, &other.limbs) };
                r.normalize();
                r
            }
            Ordering::Less => {
                let mut r = BigInt { neg: other.neg, limbs: Self::sub_abs(&other.limbs, &self.limbs) };
                r.normalize();
                r
            }
        }
    }

    pub fn sub(&self, other: &Self) -> Self {
        let mut neg = other.clone();
        if !neg.is_zero() {
            neg.neg = !neg.neg;
        }
        self.add(&neg)
    }

    pub fn neg(&self) -> Self {
        let mut r = self.clone();
        if !r.is_zero() {
            r.neg = !r.neg;
        }
        r
    }

    fn mul_school(a: &[u32], b: &[u32]) -> Vec<u32> {
        if a.is_empty() || b.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u32; a.len() + b.len()];
        for i in 0..a.len() {
            let ai = a[i] as u64;
            if ai == 0 {
                continue;
            }
            let mut carry = 0u64;
            for j in 0..b.len() {
                let idx = i + j;
                let cur = out[idx] as u64 + ai * b[j] as u64 + carry;
                out[idx] = (cur % BASE) as u32;
                carry = cur / BASE;
            }
            let mut idx = i + b.len();
            while carry > 0 {
                let cur = out[idx] as u64 + carry;
                out[idx] = (cur % BASE) as u32;
                carry = cur / BASE;
                idx += 1;
            }
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn add_shifted(out: &mut Vec<u32>, add: &[u32], shift: usize) {
        if add.is_empty() {
            return;
        }
        if out.len() < shift + add.len() + 1 {
            out.resize(shift + add.len() + 1, 0);
        }
        let mut carry = 0u64;
        let mut i = 0usize;
        while i < add.len() || carry > 0 {
            let idx = shift + i;
            if idx >= out.len() {
                out.push(0);
            }
            let x = out[idx] as u64 + *add.get(i).unwrap_or(&0) as u64 + carry;
            out[idx] = (x % BASE) as u32;
            carry = x / BASE;
            i += 1;
        }
    }

    fn mul_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
        if a.is_empty() || b.is_empty() {
            return Vec::new();
        }
        if a.len().min(b.len()) <= KARATSUBA_CUTOFF {
            return Self::mul_school(a, b);
        }
        let m = a.len().max(b.len()) / 2;
        let a0 = &a[..m.min(a.len())];
        let a1 = if a.len() > m { &a[m..] } else { &[] };
        let b0 = &b[..m.min(b.len())];
        let b1 = if b.len() > m { &b[m..] } else { &[] };

        let z0 = Self::mul_abs(a0, b0);
        let z2 = Self::mul_abs(a1, b1);
        let sa = Self::add_abs(a0, a1);
        let sb = Self::add_abs(b0, b1);
        let mut z1 = Self::mul_abs(&sa, &sb);
        z1 = Self::sub_abs(&z1, &z0);
        z1 = Self::sub_abs(&z1, &z2);

        let mut out = Vec::with_capacity(a.len() + b.len());
        Self::add_shifted(&mut out, &z0, 0);
        Self::add_shifted(&mut out, &z1, m);
        Self::add_shifted(&mut out, &z2, 2 * m);
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut r = BigInt {
            neg: self.neg != other.neg,
            limbs: Self::mul_abs(&self.limbs, &other.limbs),
        };
        r.normalize();
        r
    }

    /// 非负整数幂。指数是 u32（Copper 的 `**` 指数是 i64，调用方先检查）。
    pub fn pow(&self, mut exp: u32) -> Self {
        let mut base = self.clone();
        let mut acc = Self::one();
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc.mul(&base);
            }
            exp >>= 1;
            if exp > 0 {
                base = base.mul(&base);
            }
        }
        acc
    }

    /// 能塞进 i64 就返回，否则 None。
    pub fn to_i64(&self) -> Option<i64> {
        let mut n: u64 = 0;
        for (i, &limb) in self.limbs.iter().enumerate() {
            if i >= 3 {
                return None;
            }
            let mul = BASE.pow(i as u32);
            n = n.checked_add((limb as u64).checked_mul(mul)?)?;
        }
        if self.neg {
            if n <= i64::MAX as u64 + 1 {
                Some((n as i128 * -1) as i64)
            } else {
                None
            }
        } else if n <= i64::MAX as u64 {
            Some(n as i64)
        } else {
            None
        }
    }

    pub fn to_f64(&self) -> f64 {
        let mut n = 0.0f64;
        for &limb in self.limbs.iter().rev() {
            n = n * BASE as f64 + limb as f64;
        }
        if self.neg {
            -n
        } else {
            n
        }
    }

    pub fn to_string(&self) -> String {
        if self.limbs.is_empty() {
            return "0".to_string();
        }
        let mut s = String::with_capacity(self.limbs.len() * 9 + 1);
        if self.neg {
            s.push('-');
        }
        s.push_str(&self.limbs[self.limbs.len() - 1].to_string());
        for limb in self.limbs[..self.limbs.len() - 1].iter().rev() {
            s.push_str(&format!("{:09}", limb));
        }
        s
    }
}

impl fmt::Debug for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
