//! The formula language used throughout the data files.
//!
//! Formulas are ordinary arithmetic with comparisons, boolean logic and a
//! few functions, e.g. `0.45 * ap + r.bonus` or
//! `target.hp_pct < 0.2 && rage >= 15`. Booleans are numbers (true = 1).
//!
//! Formulas are parsed and every name is resolved when the data is loaded,
//! so a typo in a data file is reported with its location before any
//! simulation runs.

use crate::vars::VarRef;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(&'static str),
    LP,
    RP,
    Comma,
    Q,
    Colon,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && (b[i + 1] as char).is_ascii_digit()) {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                i += 1;
                if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                    i += 1;
                }
                while i < b.len() && (b[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            let s = &src[st..i];
            out.push(Tok::Num(s.parse().map_err(|_| format!("bad number '{s}'"))?));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            let s = &src[st..i];
            match s {
                "and" => out.push(Tok::Op("&&")),
                "or" => out.push(Tok::Op("||")),
                "not" => out.push(Tok::Op("!")),
                "true" => out.push(Tok::Num(1.0)),
                "false" => out.push(Tok::Num(0.0)),
                _ => out.push(Tok::Id(s.trim_end_matches('.').to_string())),
            }
            continue;
        }
        let two = if i + 1 < b.len() { &src[i..i + 2] } else { "" };
        let op2 = ["<=", ">=", "==", "!=", "&&", "||"].iter().find(|&&o| o == two);
        if let Some(o) = op2 {
            out.push(Tok::Op(o));
            i += 2;
            continue;
        }
        let t = match c {
            '+' => Tok::Op("+"),
            '-' => Tok::Op("-"),
            '*' => Tok::Op("*"),
            '/' => Tok::Op("/"),
            '%' => Tok::Op("%"),
            '<' => Tok::Op("<"),
            '>' => Tok::Op(">"),
            '!' => Tok::Op("!"),
            '(' => Tok::LP,
            ')' => Tok::RP,
            ',' => Tok::Comma,
            '?' => Tok::Q,
            ':' => Tok::Colon,
            _ => return Err(format!("unexpected character '{c}'")),
        };
        out.push(t);
        i += 1;
    }
    Ok(out)
}

#[derive(Clone, Debug)]
enum Raw {
    Num(f64),
    Id(String),
    Un(&'static str, Box<Raw>),
    Bin(&'static str, Box<Raw>, Box<Raw>),
    Tern(Box<Raw>, Box<Raw>, Box<Raw>),
    Call(String, Vec<Raw>),
}

struct Parser {
    t: Vec<Tok>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.p)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.p).cloned();
        self.p += 1;
        t
    }
    fn eat_op(&mut self, ops: &[&'static str]) -> Option<&'static str> {
        if let Some(Tok::Op(o)) = self.peek() {
            if let Some(&f) = ops.iter().find(|&&x| x == *o) {
                self.p += 1;
                return Some(f);
            }
        }
        None
    }
    fn expr(&mut self) -> Result<Raw, String> {
        let c = self.or()?;
        if self.peek() == Some(&Tok::Q) {
            self.p += 1;
            let a = self.expr()?;
            if self.next() != Some(Tok::Colon) {
                return Err("expected ':' in conditional".into());
            }
            let b = self.expr()?;
            return Ok(Raw::Tern(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn or(&mut self) -> Result<Raw, String> {
        let mut l = self.and()?;
        while let Some(o) = self.eat_op(&["||"]) {
            let r = self.and()?;
            l = Raw::Bin(o, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<Raw, String> {
        let mut l = self.cmp()?;
        while let Some(o) = self.eat_op(&["&&"]) {
            let r = self.cmp()?;
            l = Raw::Bin(o, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn cmp(&mut self) -> Result<Raw, String> {
        let l = self.add()?;
        if let Some(o) = self.eat_op(&["<", "<=", ">", ">=", "==", "!="]) {
            let r = self.add()?;
            return Ok(Raw::Bin(o, Box::new(l), Box::new(r)));
        }
        Ok(l)
    }
    fn add(&mut self) -> Result<Raw, String> {
        let mut l = self.mul()?;
        while let Some(o) = self.eat_op(&["+", "-"]) {
            let r = self.mul()?;
            l = Raw::Bin(o, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn mul(&mut self) -> Result<Raw, String> {
        let mut l = self.unary()?;
        while let Some(o) = self.eat_op(&["*", "/", "%"]) {
            let r = self.unary()?;
            l = Raw::Bin(o, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn unary(&mut self) -> Result<Raw, String> {
        if let Some(o) = self.eat_op(&["-", "!"]) {
            return Ok(Raw::Un(o, Box::new(self.unary()?)));
        }
        self.primary()
    }
    fn primary(&mut self) -> Result<Raw, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Raw::Num(n)),
            Some(Tok::Id(s)) => {
                if self.peek() == Some(&Tok::LP) {
                    self.p += 1;
                    let mut args = Vec::new();
                    if self.peek() != Some(&Tok::RP) {
                        loop {
                            args.push(self.expr()?);
                            match self.next() {
                                Some(Tok::Comma) => continue,
                                Some(Tok::RP) => break,
                                _ => return Err(format!("expected ',' or ')' in call to {s}")),
                            }
                        }
                    } else {
                        self.p += 1;
                    }
                    Ok(Raw::Call(s, args))
                } else {
                    Ok(Raw::Id(s))
                }
            }
            Some(Tok::LP) => {
                let e = self.expr()?;
                if self.next() != Some(Tok::RP) {
                    return Err("expected ')'".into());
                }
                Ok(e)
            }
            other => Err(format!("unexpected token {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Func {
    Min,
    Max,
    Clamp,
    Abs,
    Floor,
    Ceil,
    Round,
    Pow,
    Sqrt,
    If,
    /// interp(x, x0, y0, x1, y1, ...): piecewise-linear lookup, clamped at the ends.
    Interp,
}

#[derive(Clone, Debug)]
pub enum Node {
    C(f64),
    V(VarRef),
    Neg(Box<Node>),
    Not(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Cmp(&'static str, Box<Node>, Box<Node>),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Tern(Box<Node>, Box<Node>, Box<Node>),
    F(Func, Vec<Node>),
}

/// Something that can supply values for variables during evaluation.
pub trait Env {
    fn var(&self, v: &VarRef) -> f64;
}

/// A compiled formula.
#[derive(Clone, Debug)]
pub struct Expr {
    pub node: Node,
    pub src: String,
}

impl Expr {
    pub fn constant(v: f64) -> Expr {
        Expr { node: Node::C(v), src: format!("{v}") }
    }
    pub fn is_const(&self) -> Option<f64> {
        if let Node::C(v) = self.node { Some(v) } else { None }
    }

    /// Parse and resolve. `resolve` turns a name into a variable handle.
    pub fn compile(src: &str, resolve: &mut dyn FnMut(&str) -> Result<VarRef, String>) -> Result<Expr, String> {
        let toks = lex(src).map_err(|e| format!("in formula `{src}`: {e}"))?;
        if toks.is_empty() {
            return Err("empty formula".into());
        }
        let mut p = Parser { t: toks, p: 0 };
        let raw = p.expr().map_err(|e| format!("in formula `{src}`: {e}"))?;
        if p.p < p.t.len() {
            return Err(format!("in formula `{src}`: trailing input"));
        }
        let node = build(&raw, resolve).map_err(|e| format!("in formula `{src}`: {e}"))?;
        Ok(Expr { node: fold(node), src: src.to_string() })
    }

    #[inline]
    pub fn eval(&self, env: &dyn Env) -> f64 {
        eval(&self.node, env)
    }

    /// Walk every variable used (for "which talents are modeled" reports).
    pub fn visit_vars(&self, f: &mut dyn FnMut(&VarRef)) {
        visit(&self.node, f)
    }
}

fn visit(n: &Node, f: &mut dyn FnMut(&VarRef)) {
    match n {
        Node::C(_) => {}
        Node::V(v) => f(v),
        Node::Neg(a) | Node::Not(a) => visit(a, f),
        Node::Bin(_, a, b) | Node::Cmp(_, a, b) | Node::And(a, b) | Node::Or(a, b) => {
            visit(a, f);
            visit(b, f)
        }
        Node::Tern(a, b, c) => {
            visit(a, f);
            visit(b, f);
            visit(c, f)
        }
        Node::F(_, args) => args.iter().for_each(|a| visit(a, f)),
    }
}

fn build(r: &Raw, res: &mut dyn FnMut(&str) -> Result<VarRef, String>) -> Result<Node, String> {
    Ok(match r {
        Raw::Num(n) => Node::C(*n),
        Raw::Id(s) => Node::V(res(s)?),
        Raw::Un("-", a) => Node::Neg(Box::new(build(a, res)?)),
        Raw::Un(_, a) => Node::Not(Box::new(build(a, res)?)),
        Raw::Bin(o, a, b) => {
            let (a, b) = (Box::new(build(a, res)?), Box::new(build(b, res)?));
            match *o {
                "&&" => Node::And(a, b),
                "||" => Node::Or(a, b),
                "+" | "-" | "*" | "/" | "%" => Node::Bin(o.chars().next().unwrap(), a, b),
                cmp => Node::Cmp(cmp, a, b),
            }
        }
        Raw::Tern(c, a, b) => Node::Tern(Box::new(build(c, res)?), Box::new(build(a, res)?), Box::new(build(b, res)?)),
        Raw::Call(name, args) => {
            let (f, n) = match name.as_str() {
                "min" => (Func::Min, 0),
                "max" => (Func::Max, 0),
                "clamp" => (Func::Clamp, 3),
                "abs" => (Func::Abs, 1),
                "floor" => (Func::Floor, 1),
                "ceil" => (Func::Ceil, 1),
                "round" => (Func::Round, 1),
                "pow" => (Func::Pow, 2),
                "sqrt" => (Func::Sqrt, 1),
                "if" => (Func::If, 3),
                "interp" => {
                    if args.len() < 3 || args.len() % 2 == 0 {
                        return Err("interp(x, x0, y0, x1, y1, ...) needs x plus pairs".into());
                    }
                    (Func::Interp, 0)
                }
                _ => return Err(format!("unknown function '{name}'")),
            };
            if n > 0 && args.len() != n {
                return Err(format!("{name}() takes {n} arguments"));
            }
            if n == 0 && args.is_empty() {
                return Err(format!("{name}() needs arguments"));
            }
            let a: Result<Vec<_>, _> = args.iter().map(|a| build(a, res)).collect();
            Node::F(f, a?)
        }
    })
}

fn fold(n: Node) -> Node {
    struct NoEnv;
    impl Env for NoEnv {
        fn var(&self, _: &VarRef) -> f64 {
            unreachable!()
        }
    }
    fn is_c(n: &Node) -> bool {
        matches!(n, Node::C(_))
    }
    let n = match n {
        Node::Neg(a) => Node::Neg(Box::new(fold(*a))),
        Node::Not(a) => Node::Not(Box::new(fold(*a))),
        Node::Bin(o, a, b) => Node::Bin(o, Box::new(fold(*a)), Box::new(fold(*b))),
        Node::Cmp(o, a, b) => Node::Cmp(o, Box::new(fold(*a)), Box::new(fold(*b))),
        Node::And(a, b) => Node::And(Box::new(fold(*a)), Box::new(fold(*b))),
        Node::Or(a, b) => Node::Or(Box::new(fold(*a)), Box::new(fold(*b))),
        Node::Tern(a, b, c) => Node::Tern(Box::new(fold(*a)), Box::new(fold(*b)), Box::new(fold(*c))),
        Node::F(f, args) => Node::F(f, args.into_iter().map(fold).collect()),
        other => other,
    };
    let all_c = match &n {
        Node::Neg(a) | Node::Not(a) => is_c(a),
        Node::Bin(_, a, b) | Node::Cmp(_, a, b) | Node::And(a, b) | Node::Or(a, b) => is_c(a) && is_c(b),
        Node::Tern(a, b, c) => is_c(a) && is_c(b) && is_c(c),
        Node::F(_, args) => args.iter().all(is_c),
        _ => false,
    };
    if all_c {
        Node::C(eval(&n, &NoEnv))
    } else {
        n
    }
}

#[inline]
fn b(x: bool) -> f64 {
    if x { 1.0 } else { 0.0 }
}

fn eval(n: &Node, env: &dyn Env) -> f64 {
    match n {
        Node::C(v) => *v,
        Node::V(v) => env.var(v),
        Node::Neg(a) => -eval(a, env),
        Node::Not(a) => b(eval(a, env) == 0.0),
        Node::Bin(o, x, y) => {
            let (x, y) = (eval(x, env), eval(y, env));
            match o {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                '/' => {
                    if y == 0.0 { 0.0 } else { x / y }
                }
                _ => {
                    if y == 0.0 { 0.0 } else { x % y }
                }
            }
        }
        Node::Cmp(o, x, y) => {
            let (x, y) = (eval(x, env), eval(y, env));
            b(match *o {
                "<" => x < y,
                "<=" => x <= y,
                ">" => x > y,
                ">=" => x >= y,
                "==" => (x - y).abs() < 1e-9,
                _ => (x - y).abs() >= 1e-9,
            })
        }
        Node::And(x, y) => b(eval(x, env) != 0.0 && eval(y, env) != 0.0),
        Node::Or(x, y) => b(eval(x, env) != 0.0 || eval(y, env) != 0.0),
        Node::Tern(c, x, y) => {
            if eval(c, env) != 0.0 { eval(x, env) } else { eval(y, env) }
        }
        Node::F(f, a) => match f {
            Func::Min => a.iter().map(|x| eval(x, env)).fold(f64::INFINITY, f64::min),
            Func::Max => a.iter().map(|x| eval(x, env)).fold(f64::NEG_INFINITY, f64::max),
            Func::Clamp => {
                let v = eval(&a[0], env);
                v.max(eval(&a[1], env)).min(eval(&a[2], env))
            }
            Func::Abs => eval(&a[0], env).abs(),
            Func::Floor => eval(&a[0], env).floor(),
            Func::Ceil => eval(&a[0], env).ceil(),
            Func::Round => eval(&a[0], env).round(),
            Func::Pow => eval(&a[0], env).powf(eval(&a[1], env)),
            Func::Sqrt => eval(&a[0], env).max(0.0).sqrt(),
            Func::If => {
                if eval(&a[0], env) != 0.0 { eval(&a[1], env) } else { eval(&a[2], env) }
            }
            Func::Interp => {
                let x = eval(&a[0], env);
                let pts: Vec<(f64, f64)> = a[1..].chunks(2).map(|c| (eval(&c[0], env), eval(&c[1], env))).collect();
                if x <= pts[0].0 {
                    return pts[0].1;
                }
                for w in pts.windows(2) {
                    if x <= w[1].0 {
                        let f = if w[1].0 > w[0].0 { (x - w[0].0) / (w[1].0 - w[0].0) } else { 1.0 };
                        return w[0].1 + f * (w[1].1 - w[0].1);
                    }
                }
                pts[pts.len() - 1].1
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vars::{Res, Who};

    struct E;
    impl Env for E {
        fn var(&self, v: &VarRef) -> f64 {
            match v {
                VarRef::Res(Who::Me, Res::Rage) => 40.0,
                VarRef::Talent(0) => 5.0,
                _ => 0.0,
            }
        }
    }
    fn res(s: &str) -> Result<VarRef, String> {
        match s {
            "rage" => Ok(VarRef::Res(Who::Me, Res::Rage)),
            "t.cruelty" => Ok(VarRef::Talent(0)),
            "hp" => Ok(VarRef::Res(Who::Me, Res::Hp)),
            _ => Err(format!("unknown name '{s}'")),
        }
    }
    fn ev(s: &str) -> f64 {
        Expr::compile(s, &mut res).unwrap().eval(&E)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("1 + 2 * 3"), 7.0);
        assert_eq!(ev("(1 + 2) * 3"), 9.0);
        assert_eq!(ev("-2 * -3"), 6.0);
        assert_eq!(ev("10 / 4"), 2.5);
        assert_eq!(ev("1 / 0"), 0.0);
        assert_eq!(ev("2.5e1"), 25.0);
    }

    #[test]
    fn logic() {
        assert_eq!(ev("rage >= 40 && t.cruelty == 5"), 1.0);
        assert_eq!(ev("rage > 40 or not t.cruelty"), 0.0);
        assert_eq!(ev("rage < 30 ? 1 : 2"), 2.0);
        assert_eq!(ev("min(rage, 30, 50)"), 30.0);
        assert_eq!(ev("clamp(rage, 0, 25)"), 25.0);
        assert_eq!(ev("if(rage > 10, 3, 4)"), 3.0);
        assert_eq!(ev("interp(rage, 0, 0, 20, 100, 60, 300)"), 200.0);
        assert_eq!(ev("interp(100, 0, 0, 20, 100)"), 100.0);
        assert_eq!(ev("interp(-5, 0, 7, 20, 100)"), 7.0);
    }

    #[test]
    fn folding() {
        let e = Expr::compile("0.45 * 2 + 1", &mut res).unwrap();
        assert_eq!(e.is_const(), Some(1.9));
    }

    #[test]
    fn errors_name_the_problem() {
        let err = Expr::compile("rage + tpyo", &mut res).unwrap_err();
        assert!(err.contains("tpyo"), "{err}");
        assert!(Expr::compile("1 +", &mut res).is_err());
        assert!(Expr::compile("max()", &mut res).is_err());
    }
}
