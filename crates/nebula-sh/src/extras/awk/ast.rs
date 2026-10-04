//! The parsed form of an awk program.

use super::Builtin;
use std::rc::Rc;

/// Where a variable lives: a global slot, or a slot of the current function call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Var {
    Global(usize),
    Local(usize),
}

#[derive(Clone, Debug)]
pub enum LValue {
    Var(Var),
    Index(Var, Vec<Expr>),
    Field(Box<Expr>),
    /// `NF`: reading it splits the record, setting it rebuilds `$0`.
    Nf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Debug)]
pub enum GetlineSource {
    /// Plain `getline`: the next record of the main input.
    Input,
    File(Box<Expr>),
    Command(Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum Expr {
    Num(f64),
    Str(Rc<str>),
    /// `/re/` on its own: whether `$0` matches. Holds an index into `Program::regexes`.
    Regex(usize),
    Read(LValue),
    Assign(Option<BinOp>, LValue, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    In(Vec<Expr>, Var),
    Match(bool, Box<Expr>, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Concat(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Plus(Box<Expr>),
    Not(Box<Expr>),
    IncDec {
        target: LValue,
        delta: f64,
        prefix: bool,
    },
    Call(usize, Vec<Expr>),
    Builtin(Builtin, Vec<Expr>),
    Getline {
        source: GetlineSource,
        target: Option<LValue>,
    },
    /// A parenthesized expression, kept so `print (a) > "f"` and `(a, b) in arr` parse.
    Group(Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Redirect {
    File,
    Append,
    Pipe,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Expr(Expr),
    Print {
        args: Vec<Expr>,
        dest: Option<(Redirect, Expr)>,
    },
    Printf {
        args: Vec<Expr>,
        dest: Option<(Redirect, Expr)>,
    },
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Box<Stmt>>,
        body: Box<Stmt>,
    },
    ForIn(Var, Var, Box<Stmt>),
    Block(Vec<Stmt>),
    Next,
    NextFile,
    Exit(Option<Expr>),
    Return(Option<Expr>),
    Break,
    Continue,
    Delete(Var, Option<Vec<Expr>>),
}

#[derive(Clone, Debug)]
pub enum Pattern {
    All,
    Begin,
    End,
    Expr(Expr),
    Range(Expr, Expr),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub pattern: Pattern,
    /// `None` means the default action, `print $0`.
    pub action: Option<Vec<Stmt>>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub params: usize,
    pub body: Vec<Stmt>,
    /// Parameters the body uses as arrays: an uninitialized variable passed there
    /// becomes an array in the caller, as awk requires.
    pub array_params: Vec<bool>,
}

#[derive(Debug)]
pub struct Program {
    pub items: Vec<Item>,
    pub functions: Vec<Function>,
    pub globals: Vec<String>,
    pub regexes: Vec<String>,
}
