//! A recursive-descent parser for the POSIX awk grammar, with the operator precedence
//! of the standard (from lowest): `?:`, `||`, `&&`, `in`, `~`, comparisons,
//! concatenation, `+ -`, `* / %`, unary `! + -`, `^`, `++ --`, `$`, grouping.

use super::ast::*;
use super::lexer::{Tok, Token};
use super::{Builtin, SPECIAL_GLOBALS};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    globals: Vec<String>,
    global_index: HashMap<String, usize>,
    function_index: HashMap<String, usize>,
    functions: Vec<Option<Function>>,
    locals: Option<HashMap<String, usize>>,
    regexes: Vec<String>,
}

type Result<T> = std::result::Result<T, String>;

fn assign_op(tok: &Tok) -> Option<Option<BinOp>> {
    Some(match tok {
        Tok::Assign => None,
        Tok::AddAssign => Some(BinOp::Add),
        Tok::SubAssign => Some(BinOp::Sub),
        Tok::MulAssign => Some(BinOp::Mul),
        Tok::DivAssign => Some(BinOp::Div),
        Tok::ModAssign => Some(BinOp::Mod),
        Tok::PowAssign => Some(BinOp::Pow),
        _ => return None,
    })
}

fn starts_expr(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Number(_)
            | Tok::Str(_)
            | Tok::Regex(_)
            | Tok::Name(_)
            | Tok::FuncName(_)
            | Tok::Builtin(_)
            | Tok::Dollar
            | Tok::Not
            | Tok::Minus
            | Tok::Plus
            | Tok::LParen
            | Tok::Incr
            | Tok::Decr
            | Tok::Getline
    )
}

/// Tokens that start the right side of a concatenation. Signs and `!` don't: `a -1`
/// subtracts.
fn starts_concat(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Number(_)
            | Tok::Str(_)
            | Tok::Name(_)
            | Tok::FuncName(_)
            | Tok::Builtin(_)
            | Tok::Dollar
            | Tok::LParen
    )
}

fn as_lvalue(expr: &Expr) -> Option<LValue> {
    match expr {
        Expr::Read(target) => Some(target.clone()),
        _ => None,
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        globals: Vec::new(),
        global_index: HashMap::new(),
        function_index: HashMap::new(),
        functions: Vec::new(),
        locals: None,
        regexes: Vec::new(),
    };
    for name in SPECIAL_GLOBALS {
        parser.global(name);
    }
    parser.scan_functions()?;
    let items = parser.program()?;
    let mut functions: Vec<Function> = parser.functions.into_iter().flatten().collect();
    mark_array_params(&mut functions);
    Ok(Program {
        items,
        functions,
        globals: parser.globals,
        regexes: parser.regexes,
    })
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn peek_at(&self, offset: usize) -> &Tok {
        let index = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[index].tok
    }

    fn line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn advance(&mut self) -> Tok {
        let tok = self.tokens[self.pos].tok.clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn error<T>(&self, message: &str) -> Result<T> {
        Err(format!("syntax error at line {}: {message}", self.line()))
    }

    fn unexpected<T>(&self) -> Result<T> {
        let found = match self.peek() {
            Tok::Eof => "end of program".to_owned(),
            Tok::Newline => "end of line".to_owned(),
            other => format!("{other:?}"),
        };
        self.error(&format!("unexpected {found}"))
    }

    fn expect(&mut self, tok: Tok, what: &str) -> Result<()> {
        if *self.peek() == tok {
            self.advance();
            Ok(())
        } else {
            self.error(&format!("expected {what}"))
        }
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.advance();
        }
    }

    fn skip_terminators(&mut self) {
        while matches!(self.peek(), Tok::Newline | Tok::Semi) {
            self.advance();
        }
    }

    fn global(&mut self, name: &str) -> usize {
        if let Some(&index) = self.global_index.get(name) {
            return index;
        }
        let index = self.globals.len();
        self.globals.push(name.to_owned());
        self.global_index.insert(name.to_owned(), index);
        index
    }

    fn var(&mut self, name: &str) -> Var {
        if let Some(index) = self.locals.as_ref().and_then(|locals| locals.get(name)) {
            return Var::Local(*index);
        }
        Var::Global(self.global(name))
    }

    fn regex(&mut self, pattern: String) -> usize {
        self.regexes.push(pattern);
        self.regexes.len() - 1
    }

    /// Function names are known before parsing, so calls can come before definitions.
    fn scan_functions(&mut self) -> Result<()> {
        for window in self.tokens.windows(2) {
            if window[0].tok != Tok::Function {
                continue;
            }
            match &window[1].tok {
                Tok::Name(name) | Tok::FuncName(name) => {
                    if self.function_index.contains_key(name) {
                        return Err(format!("function `{name}' is defined twice"));
                    }
                    self.function_index
                        .insert(name.clone(), self.functions.len());
                    self.functions.push(None);
                }
                _ => {
                    return Err(format!(
                        "syntax error at line {}: expected a function name",
                        window[1].line
                    ))
                }
            }
        }
        Ok(())
    }

    fn program(&mut self) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        loop {
            self.skip_terminators();
            match self.peek() {
                Tok::Eof => break,
                Tok::Function => self.function()?,
                _ => items.push(self.item()?),
            }
        }
        Ok(items)
    }

    fn function(&mut self) -> Result<()> {
        self.advance();
        let name = match self.advance() {
            Tok::Name(name) | Tok::FuncName(name) => name,
            _ => return self.error("expected a function name"),
        };
        if Builtin::from_name(&name).is_some() {
            return self.error(&format!("`{name}' is a built-in function"));
        }
        self.expect(Tok::LParen, "( after the function name")?;
        let mut params = HashMap::new();
        let mut count = 0;
        self.skip_newlines();
        while *self.peek() != Tok::RParen {
            match self.advance() {
                Tok::Name(param) => {
                    params.insert(param, count);
                    count += 1;
                }
                _ => return self.error("expected a parameter name"),
            }
            self.skip_newlines();
            if *self.peek() == Tok::Comma {
                self.advance();
                self.skip_newlines();
            } else if *self.peek() != Tok::RParen {
                return self.error("expected , or ) in the parameter list");
            }
        }
        self.advance();
        self.skip_newlines();
        self.locals = Some(params);
        let body = self.block();
        self.locals = None;
        let index = self.function_index[&name];
        self.functions[index] = Some(Function {
            name,
            params: count,
            body: body?,
            array_params: vec![false; count],
        });
        Ok(())
    }

    fn item(&mut self) -> Result<Item> {
        let pattern = match self.peek() {
            Tok::Begin => {
                self.advance();
                Pattern::Begin
            }
            Tok::End => {
                self.advance();
                Pattern::End
            }
            Tok::LBrace => Pattern::All,
            _ => {
                let first = self.expr(false)?;
                if *self.peek() == Tok::Comma {
                    self.advance();
                    self.skip_newlines();
                    Pattern::Range(first, self.expr(false)?)
                } else {
                    Pattern::Expr(first)
                }
            }
        };
        if *self.peek() == Tok::LBrace {
            return Ok(Item {
                pattern,
                action: Some(self.block()?),
            });
        }
        if matches!(pattern, Pattern::Begin | Pattern::End) {
            return self.error("BEGIN and END need an action in { }");
        }
        match self.peek() {
            Tok::Newline | Tok::Semi | Tok::Eof => Ok(Item {
                pattern,
                action: None,
            }),
            _ => self.unexpected(),
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(Tok::LBrace, "{")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_terminators();
            match self.peek() {
                Tok::RBrace => {
                    self.advance();
                    return Ok(stmts);
                }
                Tok::Eof => return self.error("missing }"),
                _ => stmts.push(self.statement()?),
            }
        }
    }

    fn statement(&mut self) -> Result<Stmt> {
        match self.peek() {
            Tok::LBrace => Ok(Stmt::Block(self.block()?)),
            Tok::Semi => {
                self.advance();
                Ok(Stmt::Block(Vec::new()))
            }
            Tok::If => {
                self.advance();
                self.expect(Tok::LParen, "( after if")?;
                let cond = self.expr(false)?;
                self.expect(Tok::RParen, ") after the if condition")?;
                self.skip_newlines();
                let then = Box::new(self.statement()?);
                let save = self.pos;
                self.skip_terminators();
                if *self.peek() == Tok::Else {
                    self.advance();
                    self.skip_newlines();
                    let otherwise = Box::new(self.statement()?);
                    Ok(Stmt::If(cond, then, Some(otherwise)))
                } else {
                    self.pos = save;
                    Ok(Stmt::If(cond, then, None))
                }
            }
            Tok::While => {
                self.advance();
                self.expect(Tok::LParen, "( after while")?;
                let cond = self.expr(false)?;
                self.expect(Tok::RParen, ") after the while condition")?;
                if *self.peek() == Tok::Semi {
                    self.advance();
                    return Ok(Stmt::While(cond, Box::new(Stmt::Block(Vec::new()))));
                }
                self.skip_newlines();
                Ok(Stmt::While(cond, Box::new(self.statement()?)))
            }
            Tok::Do => {
                self.advance();
                self.skip_newlines();
                let body = Box::new(self.statement()?);
                self.skip_terminators();
                self.expect(Tok::While, "while after the do body")?;
                self.expect(Tok::LParen, "( after while")?;
                let cond = self.expr(false)?;
                self.expect(Tok::RParen, ") after the while condition")?;
                self.end_simple()?;
                Ok(Stmt::DoWhile(body, cond))
            }
            Tok::For => self.for_statement(),
            _ => {
                let stmt = self.simple_statement()?;
                self.end_simple()?;
                Ok(stmt)
            }
        }
    }

    /// A simple statement ends at `;`, a newline, `}` or the end of the program.
    fn end_simple(&mut self) -> Result<()> {
        match self.peek() {
            Tok::Semi | Tok::Newline => {
                self.advance();
                Ok(())
            }
            Tok::RBrace | Tok::Eof | Tok::Else => Ok(()),
            _ => self.unexpected(),
        }
    }

    fn for_statement(&mut self) -> Result<Stmt> {
        self.advance();
        self.expect(Tok::LParen, "( after for")?;
        if let (Tok::Name(key), Tok::In, Tok::Name(array), Tok::RParen) = (
            self.peek().clone(),
            self.peek_at(1).clone(),
            self.peek_at(2).clone(),
            self.peek_at(3).clone(),
        ) {
            self.pos += 4;
            self.skip_newlines();
            let key = self.var(&key);
            let array = self.var(&array);
            return Ok(Stmt::ForIn(key, array, Box::new(self.statement()?)));
        }
        let init = if *self.peek() == Tok::Semi {
            None
        } else {
            Some(Box::new(self.simple_statement()?))
        };
        self.expect(Tok::Semi, "; in the for header")?;
        self.skip_newlines();
        let cond = if *self.peek() == Tok::Semi {
            None
        } else {
            Some(self.expr(false)?)
        };
        self.expect(Tok::Semi, "; in the for header")?;
        self.skip_newlines();
        let step = if *self.peek() == Tok::RParen {
            None
        } else {
            Some(Box::new(self.simple_statement()?))
        };
        self.expect(Tok::RParen, ") after the for header")?;
        if *self.peek() == Tok::Semi {
            self.advance();
            return Ok(Stmt::For {
                init,
                cond,
                step,
                body: Box::new(Stmt::Block(Vec::new())),
            });
        }
        self.skip_newlines();
        let body = Box::new(self.statement()?);
        Ok(Stmt::For {
            init,
            cond,
            step,
            body,
        })
    }

    fn simple_statement(&mut self) -> Result<Stmt> {
        match self.peek() {
            Tok::Print | Tok::Printf => self.print_statement(),
            Tok::Next => {
                self.advance();
                Ok(Stmt::Next)
            }
            Tok::NextFile => {
                self.advance();
                Ok(Stmt::NextFile)
            }
            Tok::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Tok::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Tok::Exit => {
                self.advance();
                let value = if starts_expr(self.peek()) {
                    Some(self.expr(false)?)
                } else {
                    None
                };
                Ok(Stmt::Exit(value))
            }
            Tok::Return => {
                if self.locals.is_none() {
                    return self.error("return outside a function");
                }
                self.advance();
                let value = if starts_expr(self.peek()) {
                    Some(self.expr(false)?)
                } else {
                    None
                };
                Ok(Stmt::Return(value))
            }
            Tok::Delete => {
                self.advance();
                let name = match self.advance() {
                    Tok::Name(name) => name,
                    _ => return self.error("expected an array name after delete"),
                };
                let var = self.var(&name);
                if *self.peek() == Tok::LBracket {
                    self.advance();
                    let subscripts = self.expr_list(false)?;
                    self.expect(Tok::RBracket, "]")?;
                    Ok(Stmt::Delete(var, Some(subscripts)))
                } else {
                    Ok(Stmt::Delete(var, None))
                }
            }
            _ => Ok(Stmt::Expr(self.expr(false)?)),
        }
    }

    fn print_statement(&mut self) -> Result<Stmt> {
        let printf = self.advance() == Tok::Printf;
        let mut args = Vec::new();
        let mut grouped = false;
        if *self.peek() == Tok::LParen {
            // `print (a, b) > "file"`: the parentheses group the whole list.
            let save = self.pos;
            self.advance();
            if let Ok(list) = self.expr_list(false) {
                if *self.peek() == Tok::RParen {
                    self.advance();
                    if matches!(
                        self.peek(),
                        Tok::Gt
                            | Tok::Append
                            | Tok::Pipe
                            | Tok::Semi
                            | Tok::Newline
                            | Tok::RBrace
                            | Tok::Eof
                    ) {
                        args = list;
                        grouped = true;
                    }
                }
            }
            if !grouped {
                self.pos = save;
            }
        }
        if !grouped && starts_expr(self.peek()) {
            args = self.expr_list(true)?;
        }
        let redirect = match self.peek() {
            Tok::Gt => Some(Redirect::File),
            Tok::Append => Some(Redirect::Append),
            Tok::Pipe => Some(Redirect::Pipe),
            _ => None,
        };
        let dest = match redirect {
            Some(kind) => {
                self.advance();
                Some((kind, self.concat()?))
            }
            None => None,
        };
        if printf {
            if args.is_empty() {
                return self.error("printf needs a format");
            }
            Ok(Stmt::Printf { args, dest })
        } else {
            Ok(Stmt::Print { args, dest })
        }
    }

    fn expr_list(&mut self, no_gt: bool) -> Result<Vec<Expr>> {
        let mut list = vec![self.expr(no_gt)?];
        while *self.peek() == Tok::Comma {
            self.advance();
            self.skip_newlines();
            list.push(self.expr(no_gt)?);
        }
        Ok(list)
    }

    /// `no_gt` is set inside print arguments, where a bare `>` redirects the output.
    fn expr(&mut self, no_gt: bool) -> Result<Expr> {
        let cond = self.or(no_gt)?;
        if *self.peek() == Tok::Question {
            self.advance();
            self.skip_newlines();
            let then = self.expr(no_gt)?;
            self.skip_newlines();
            self.expect(Tok::Colon, ": in the conditional expression")?;
            self.skip_newlines();
            let otherwise = self.expr(no_gt)?;
            return Ok(Expr::Cond(
                Box::new(cond),
                Box::new(then),
                Box::new(otherwise),
            ));
        }
        if let Some(op) = assign_op(self.peek()) {
            if let Some(target) = as_lvalue(&cond) {
                self.advance();
                self.skip_newlines();
                let value = self.expr(no_gt)?;
                return Ok(Expr::Assign(op, target, Box::new(value)));
            }
        }
        Ok(cond)
    }

    fn or(&mut self, no_gt: bool) -> Result<Expr> {
        let mut left = self.and(no_gt)?;
        while *self.peek() == Tok::Or {
            self.advance();
            self.skip_newlines();
            let right = self.and(no_gt)?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and(&mut self, no_gt: bool) -> Result<Expr> {
        let mut left = self.in_expr(no_gt)?;
        while *self.peek() == Tok::And {
            self.advance();
            self.skip_newlines();
            let right = self.in_expr(no_gt)?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn in_expr(&mut self, no_gt: bool) -> Result<Expr> {
        let mut left = self.matching(no_gt)?;
        while *self.peek() == Tok::In {
            self.advance();
            let name = match self.advance() {
                Tok::Name(name) => name,
                _ => return self.error("expected an array name after in"),
            };
            let array = self.var(&name);
            left = Expr::In(vec![left], array);
        }
        Ok(left)
    }

    fn matching(&mut self, no_gt: bool) -> Result<Expr> {
        let mut left = self.comparison(no_gt)?;
        loop {
            let negate = match self.peek() {
                Tok::Tilde => false,
                Tok::NoMatch => true,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.comparison(no_gt)?;
            left = Expr::Match(negate, Box::new(left), Box::new(right));
        }
    }

    fn comparison(&mut self, no_gt: bool) -> Result<Expr> {
        let mut left = self.concat()?;
        loop {
            if *self.peek() == Tok::Pipe && *self.peek_at(1) == Tok::Getline {
                self.pos += 2;
                let target = self.getline_target()?;
                left = Expr::Getline {
                    source: GetlineSource::Command(Box::new(left)),
                    target,
                };
                continue;
            }
            let op = match self.peek() {
                Tok::Lt => BinOp::Lt,
                Tok::Le => BinOp::Le,
                Tok::Ne => BinOp::Ne,
                Tok::Eq => BinOp::Eq,
                Tok::Ge => BinOp::Ge,
                Tok::Gt if !no_gt => BinOp::Gt,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.concat()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn concat(&mut self) -> Result<Expr> {
        let mut left = self.additive()?;
        while starts_concat(self.peek()) {
            let right = self.additive()?;
            left = Expr::Concat(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Mod,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        match self.peek() {
            Tok::Not => {
                self.advance();
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            Tok::Minus => {
                self.advance();
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Tok::Plus => {
                self.advance();
                Ok(Expr::Plus(Box::new(self.unary()?)))
            }
            _ => self.power(),
        }
    }

    /// `^` is right-associative, and its exponent may carry a sign: `2^-1`.
    fn power(&mut self) -> Result<Expr> {
        let base = self.postfix()?;
        if *self.peek() != Tok::Caret {
            return Ok(base);
        }
        self.advance();
        let exponent = match self.peek() {
            Tok::Minus | Tok::Plus | Tok::Not => self.unary()?,
            _ => self.power()?,
        };
        Ok(Expr::Binary(BinOp::Pow, Box::new(base), Box::new(exponent)))
    }

    fn postfix(&mut self) -> Result<Expr> {
        let expr = self.primary()?;
        let delta = match self.peek() {
            Tok::Incr => 1.0,
            Tok::Decr => -1.0,
            _ => return Ok(expr),
        };
        match as_lvalue(&expr) {
            Some(target) => {
                self.advance();
                Ok(Expr::IncDec {
                    target,
                    delta,
                    prefix: false,
                })
            }
            None => Ok(expr),
        }
    }

    fn lvalue(&mut self) -> Result<LValue> {
        match self.advance() {
            Tok::Dollar => Ok(LValue::Field(Box::new(self.field_operand()?))),
            Tok::Name(name) => self.name_lvalue(name),
            _ => self.error("expected a variable"),
        }
    }

    fn name_lvalue(&mut self, name: String) -> Result<LValue> {
        if *self.peek() == Tok::LBracket {
            self.advance();
            let subscripts = self.expr_list(false)?;
            self.expect(Tok::RBracket, "]")?;
            return Ok(LValue::Index(self.var(&name), subscripts));
        }
        if name == "NF"
            && self
                .locals
                .as_ref()
                .is_none_or(|locals| !locals.contains_key("NF"))
        {
            return Ok(LValue::Nf);
        }
        Ok(LValue::Var(self.var(&name)))
    }

    /// What follows `$`: `$NF`, `$(i+1)`, `$++i`, `$-1`.
    fn field_operand(&mut self) -> Result<Expr> {
        match self.peek() {
            Tok::Incr | Tok::Decr => {
                let delta = if self.advance() == Tok::Incr {
                    1.0
                } else {
                    -1.0
                };
                let target = self.lvalue()?;
                Ok(Expr::IncDec {
                    target,
                    delta,
                    prefix: true,
                })
            }
            Tok::Minus => {
                self.advance();
                Ok(Expr::Neg(Box::new(self.primary()?)))
            }
            _ => self.primary(),
        }
    }

    fn getline_target(&mut self) -> Result<Option<LValue>> {
        match self.peek() {
            Tok::Name(_) | Tok::Dollar => Ok(Some(self.lvalue()?)),
            _ => Ok(None),
        }
    }

    fn call_args(&mut self) -> Result<Vec<Expr>> {
        self.expect(Tok::LParen, "(")?;
        self.skip_newlines();
        if *self.peek() == Tok::RParen {
            self.advance();
            return Ok(Vec::new());
        }
        let args = self.expr_list(false)?;
        self.skip_newlines();
        self.expect(Tok::RParen, ") after the arguments")?;
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Tok::Number(value) => {
                self.advance();
                Ok(Expr::Num(value))
            }
            Tok::Str(text) => {
                self.advance();
                Ok(Expr::Str(text.into()))
            }
            Tok::Regex(pattern) => {
                self.advance();
                Ok(Expr::Regex(self.regex(pattern)))
            }
            Tok::Dollar => {
                self.advance();
                Ok(Expr::Read(LValue::Field(Box::new(self.field_operand()?))))
            }
            Tok::Not | Tok::Minus | Tok::Plus => self.unary(),
            Tok::Incr | Tok::Decr => {
                let delta = if self.advance() == Tok::Incr {
                    1.0
                } else {
                    -1.0
                };
                let target = self.lvalue()?;
                Ok(Expr::IncDec {
                    target,
                    delta,
                    prefix: true,
                })
            }
            Tok::LParen => {
                self.advance();
                self.skip_newlines();
                let first = self.expr(false)?;
                if *self.peek() == Tok::Comma {
                    let mut list = vec![first];
                    while *self.peek() == Tok::Comma {
                        self.advance();
                        self.skip_newlines();
                        list.push(self.expr(false)?);
                    }
                    self.expect(Tok::RParen, ")")?;
                    if *self.peek() != Tok::In {
                        return self.error("a list in parentheses must be followed by in");
                    }
                    self.advance();
                    let name = match self.advance() {
                        Tok::Name(name) => name,
                        _ => return self.error("expected an array name after in"),
                    };
                    let array = self.var(&name);
                    return Ok(Expr::In(list, array));
                }
                self.skip_newlines();
                self.expect(Tok::RParen, ")")?;
                Ok(Expr::Group(Box::new(first)))
            }
            Tok::Name(name) => {
                self.advance();
                Ok(Expr::Read(self.name_lvalue(name)?))
            }
            Tok::FuncName(name) => {
                self.advance();
                let Some(&index) = self.function_index.get(&name) else {
                    return self.error(&format!("function `{name}' is not defined"));
                };
                let args = self.call_args()?;
                Ok(Expr::Call(index, args))
            }
            Tok::Builtin(builtin) => {
                self.advance();
                if *self.peek() == Tok::LParen {
                    let args = self.call_args()?;
                    return Ok(Expr::Builtin(builtin, args));
                }
                if builtin == Builtin::Length {
                    return Ok(Expr::Builtin(builtin, Vec::new()));
                }
                self.error(&format!("{} needs its arguments in ( )", builtin.name()))
            }
            Tok::Getline => {
                self.advance();
                let target = self.getline_target()?;
                if *self.peek() == Tok::Lt {
                    self.advance();
                    let file = self.primary()?;
                    return Ok(Expr::Getline {
                        source: GetlineSource::File(Box::new(file)),
                        target,
                    });
                }
                Ok(Expr::Getline {
                    source: GetlineSource::Input,
                    target,
                })
            }
            _ => self.unexpected(),
        }
    }
}

/// Finds the parameters each function uses as arrays, following calls that pass a
/// parameter on to another function's array parameter.
fn mark_array_params(functions: &mut [Function]) {
    loop {
        let mut changed = false;
        for index in 0..functions.len() {
            let mut found = functions[index].array_params.clone();
            for stmt in &functions[index].body {
                visit_stmt(stmt, functions, &mut found);
            }
            if found != functions[index].array_params {
                functions[index].array_params = found;
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

fn mark(var: &Var, found: &mut [bool]) {
    if let Var::Local(index) = var {
        if let Some(slot) = found.get_mut(*index) {
            *slot = true;
        }
    }
}

fn visit_stmt(stmt: &Stmt, functions: &[Function], found: &mut [bool]) {
    match stmt {
        Stmt::Expr(expr) => visit_expr(expr, functions, found),
        Stmt::Print { args, dest } | Stmt::Printf { args, dest } => {
            for arg in args {
                visit_expr(arg, functions, found);
            }
            if let Some((_, expr)) = dest {
                visit_expr(expr, functions, found);
            }
        }
        Stmt::If(cond, then, otherwise) => {
            visit_expr(cond, functions, found);
            visit_stmt(then, functions, found);
            if let Some(otherwise) = otherwise {
                visit_stmt(otherwise, functions, found);
            }
        }
        Stmt::While(cond, body) | Stmt::DoWhile(body, cond) => {
            visit_expr(cond, functions, found);
            visit_stmt(body, functions, found);
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            for part in [init, step].into_iter().flatten() {
                visit_stmt(part, functions, found);
            }
            if let Some(cond) = cond {
                visit_expr(cond, functions, found);
            }
            visit_stmt(body, functions, found);
        }
        Stmt::ForIn(_, array, body) => {
            mark(array, found);
            visit_stmt(body, functions, found);
        }
        Stmt::Block(stmts) => {
            for stmt in stmts {
                visit_stmt(stmt, functions, found);
            }
        }
        Stmt::Exit(Some(expr)) | Stmt::Return(Some(expr)) => visit_expr(expr, functions, found),
        Stmt::Delete(array, subscripts) => {
            mark(array, found);
            for expr in subscripts.iter().flatten() {
                visit_expr(expr, functions, found);
            }
        }
        _ => {}
    }
}

fn visit_lvalue(target: &LValue, functions: &[Function], found: &mut [bool]) {
    match target {
        LValue::Index(array, subscripts) => {
            mark(array, found);
            for expr in subscripts {
                visit_expr(expr, functions, found);
            }
        }
        LValue::Field(expr) => visit_expr(expr, functions, found),
        LValue::Var(_) | LValue::Nf => {}
    }
}

fn visit_expr(expr: &Expr, functions: &[Function], found: &mut [bool]) {
    match expr {
        Expr::Read(target) => visit_lvalue(target, functions, found),
        Expr::Assign(_, target, value) => {
            visit_lvalue(target, functions, found);
            visit_expr(value, functions, found);
        }
        Expr::Cond(a, b, c) => {
            for part in [a, b, c] {
                visit_expr(part, functions, found);
            }
        }
        Expr::And(a, b)
        | Expr::Or(a, b)
        | Expr::Match(_, a, b)
        | Expr::Binary(_, a, b)
        | Expr::Concat(a, b) => {
            visit_expr(a, functions, found);
            visit_expr(b, functions, found);
        }
        Expr::In(subscripts, array) => {
            mark(array, found);
            for expr in subscripts {
                visit_expr(expr, functions, found);
            }
        }
        Expr::Neg(a) | Expr::Plus(a) | Expr::Not(a) | Expr::Group(a) => {
            visit_expr(a, functions, found)
        }
        Expr::IncDec { target, .. } => visit_lvalue(target, functions, found),
        Expr::Call(index, args) => {
            for (position, arg) in args.iter().enumerate() {
                if let Expr::Read(LValue::Var(var)) = arg {
                    let wants_array = functions
                        .get(*index)
                        .and_then(|function| function.array_params.get(position))
                        .copied()
                        .unwrap_or(false);
                    if wants_array {
                        mark(var, found);
                    }
                }
                visit_expr(arg, functions, found);
            }
        }
        Expr::Builtin(builtin, args) => {
            if *builtin == Builtin::Split {
                if let Some(Expr::Read(LValue::Var(var))) = args.get(1) {
                    mark(var, found);
                }
            }
            for arg in args {
                visit_expr(arg, functions, found);
            }
        }
        Expr::Getline { source, target } => {
            match source {
                GetlineSource::File(expr) | GetlineSource::Command(expr) => {
                    visit_expr(expr, functions, found)
                }
                GetlineSource::Input => {}
            }
            if let Some(target) = target {
                visit_lvalue(target, functions, found);
            }
        }
        Expr::Num(_) | Expr::Str(_) | Expr::Regex(_) => {}
    }
}
