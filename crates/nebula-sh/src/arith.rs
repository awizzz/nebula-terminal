//! Shell arithmetic, as in `$(( … ))`, `(( … ))`, `let` and `for (( … ))`:
//! 64-bit integers with the C operators, assignments and `++` / `--`.

pub trait Vars {
    fn get(&mut self, name: &str) -> Option<String>;
    fn set(&mut self, name: &str, value: i64);
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(i64),
    Name(String),
    Op(&'static str),
}

const OPERATORS: &[&str] = &[
    "<<=", ">>=", "**", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=",
    "*=", "/=", "%=", "&=", "^=", "|=", "+", "-", "*", "/", "%", "<", ">", "=", "!", "~", "&", "^",
    "|", "?", ":", ",", "(", ")",
];

fn parse_number(text: &str) -> Result<i64, String> {
    let invalid = || format!("{text}: invalid number");
    if let Some((base, digits)) = text.split_once('#') {
        let base: u32 = base.parse().map_err(|_| invalid())?;
        if !(2..=36).contains(&base) {
            return Err(invalid());
        }
        return i64::from_str_radix(digits, base).map_err(|_| invalid());
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).map_err(|_| invalid());
    }
    if text.len() > 1 && text.starts_with('0') {
        return i64::from_str_radix(&text[1..], 8).map_err(|_| invalid());
    }
    text.parse().map_err(|_| invalid())
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '#') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            tokens.push(Token::Num(parse_number(&text)?));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token::Name(chars[start..i].iter().collect()));
            continue;
        }
        let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
        let Some(op) = OPERATORS.iter().find(|op| rest.starts_with(**op)) else {
            return Err(format!("syntax error: invalid character `{c}`"));
        };
        tokens.push(Token::Op(op));
        i += op.chars().count();
    }
    Ok(tokens)
}

#[derive(Debug, Clone)]
enum Expr {
    Num(i64),
    Var(String),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `name op= value`; `op` is `=` for plain assignment.
    Assign(String, &'static str, Box<Expr>),
    /// `++name` / `--name` (`prefix`) or `name++` / `name--`.
    Step {
        name: String,
        delta: i64,
        prefix: bool,
    },
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

fn binary_precedence(op: &str) -> Option<(u8, bool)> {
    // (precedence, right associative)
    Some(match op {
        "||" => (1, false),
        "&&" => (2, false),
        "|" => (3, false),
        "^" => (4, false),
        "&" => (5, false),
        "==" | "!=" => (6, false),
        "<" | ">" | "<=" | ">=" => (7, false),
        "<<" | ">>" => (8, false),
        "+" | "-" => (9, false),
        "*" | "/" | "%" => (10, false),
        "**" => (11, true),
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_op(&self) -> Option<&'static str> {
        match self.peek() {
            Some(Token::Op(op)) => Some(op),
            _ => None,
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), String> {
        if self.peek_op() == Some(op) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("syntax error: `{op}` expected"))
        }
    }

    fn comma(&mut self) -> Result<Expr, String> {
        let mut expr = self.assignment()?;
        while self.peek_op() == Some(",") {
            self.pos += 1;
            let right = self.assignment()?;
            expr = Expr::Binary(",", Box::new(expr), Box::new(right));
        }
        Ok(expr)
    }

    fn assignment(&mut self) -> Result<Expr, String> {
        if let (Some(Token::Name(name)), Some(Token::Op(op))) =
            (self.tokens.get(self.pos), self.tokens.get(self.pos + 1))
        {
            if matches!(
                *op,
                "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "^=" | "|="
            ) {
                let name = name.clone();
                let op = *op;
                self.pos += 2;
                let value = self.assignment()?;
                return Ok(Expr::Assign(name, op, Box::new(value)));
            }
        }
        self.ternary()
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let test = self.binary(1)?;
        if self.peek_op() != Some("?") {
            return Ok(test);
        }
        self.pos += 1;
        let then = self.assignment()?;
        self.expect(":")?;
        let otherwise = self.assignment()?;
        Ok(Expr::Ternary(
            Box::new(test),
            Box::new(then),
            Box::new(otherwise),
        ))
    }

    fn binary(&mut self, min: u8) -> Result<Expr, String> {
        let mut left = self.unary()?;
        while let Some(op) = self.peek_op() {
            let Some((precedence, right_assoc)) = binary_precedence(op) else {
                break;
            };
            if precedence < min {
                break;
            }
            self.pos += 1;
            let next = if right_assoc {
                precedence
            } else {
                precedence + 1
            };
            let right = self.binary(next)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek_op() {
            Some(op @ ("++" | "--")) => {
                self.pos += 1;
                match self.tokens.get(self.pos) {
                    Some(Token::Name(name)) => {
                        let name = name.clone();
                        self.pos += 1;
                        Ok(Expr::Step {
                            name,
                            delta: if op == "++" { 1 } else { -1 },
                            prefix: true,
                        })
                    }
                    _ => {
                        // `--5` is minus minus five.
                        let operand = self.unary()?;
                        let sign = if op == "++" { "+" } else { "-" };
                        Ok(Expr::Unary(
                            sign,
                            Box::new(Expr::Unary(sign, Box::new(operand))),
                        ))
                    }
                }
            }
            Some(op @ ("!" | "~" | "-" | "+")) => {
                self.pos += 1;
                let operand = self.unary()?;
                Ok(Expr::Unary(op, Box::new(operand)))
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let expr = self.primary()?;
        if let Expr::Var(name) = &expr {
            if let Some(op @ ("++" | "--")) = self.peek_op() {
                self.pos += 1;
                return Ok(Expr::Step {
                    name: name.clone(),
                    delta: if op == "++" { 1 } else { -1 },
                    prefix: false,
                });
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.tokens.get(self.pos).cloned() {
            Some(Token::Num(value)) => {
                self.pos += 1;
                Ok(Expr::Num(value))
            }
            Some(Token::Name(name)) => {
                self.pos += 1;
                Ok(Expr::Var(name))
            }
            Some(Token::Op("(")) => {
                self.pos += 1;
                let expr = self.comma()?;
                self.expect(")")?;
                Ok(expr)
            }
            Some(Token::Op(op)) => Err(format!("syntax error: operand expected (near `{op}`)")),
            None => Err("syntax error: operand expected".to_owned()),
        }
    }
}

fn parse(source: &str) -> Result<Option<Expr>, String> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.comma()?;
    if let Some(token) = parser.peek() {
        let text = match token {
            Token::Num(value) => value.to_string(),
            Token::Name(name) => name.clone(),
            Token::Op(op) => (*op).to_owned(),
        };
        return Err(format!("syntax error near `{text}`"));
    }
    Ok(Some(expr))
}

struct Evaluator<'a> {
    vars: &'a mut dyn Vars,
    depth: usize,
}

impl Evaluator<'_> {
    fn variable(&mut self, name: &str) -> Result<i64, String> {
        let text = self.vars.get(name).unwrap_or_default();
        let text = text.trim();
        if text.is_empty() {
            return Ok(0);
        }
        if let Ok(value) = parse_number(text) {
            return Ok(value);
        }
        // Like bash, a variable may hold an expression.
        if self.depth > 16 {
            return Err(format!("{name}: expression recursion level exceeded"));
        }
        let expr = parse(text)?;
        self.depth += 1;
        let value = match expr {
            Some(expr) => self.eval(&expr),
            None => Ok(0),
        };
        self.depth -= 1;
        value
    }

    fn apply(op: &str, left: i64, right: i64) -> Result<i64, String> {
        Ok(match op {
            "+" => left.wrapping_add(right),
            "-" => left.wrapping_sub(right),
            "*" => left.wrapping_mul(right),
            "/" | "%" if right == 0 => return Err("division by 0".to_owned()),
            "/" => left.wrapping_div(right),
            "%" => left.wrapping_rem(right),
            "**" => {
                if right < 0 {
                    return Err("exponent less than 0".to_owned());
                }
                left.wrapping_pow(u32::try_from(right).unwrap_or(u32::MAX))
            }
            "<<" => left.wrapping_shl(u32::try_from(right & 63).unwrap_or(0)),
            ">>" => left.wrapping_shr(u32::try_from(right & 63).unwrap_or(0)),
            "<" => i64::from(left < right),
            ">" => i64::from(left > right),
            "<=" => i64::from(left <= right),
            ">=" => i64::from(left >= right),
            "==" => i64::from(left == right),
            "!=" => i64::from(left != right),
            "&" => left & right,
            "^" => left ^ right,
            "|" => left | right,
            "," => right,
            other => return Err(format!("unknown operator `{other}`")),
        })
    }

    fn eval(&mut self, expr: &Expr) -> Result<i64, String> {
        match expr {
            Expr::Num(value) => Ok(*value),
            Expr::Var(name) => self.variable(name),
            Expr::Unary(op, operand) => {
                let value = self.eval(operand)?;
                Ok(match *op {
                    "!" => i64::from(value == 0),
                    "~" => !value,
                    "-" => value.wrapping_neg(),
                    _ => value,
                })
            }
            Expr::Binary("&&", left, right) => {
                Ok(i64::from(self.eval(left)? != 0 && self.eval(right)? != 0))
            }
            Expr::Binary("||", left, right) => {
                Ok(i64::from(self.eval(left)? != 0 || self.eval(right)? != 0))
            }
            Expr::Binary(op, left, right) => {
                let left = self.eval(left)?;
                let right = self.eval(right)?;
                Self::apply(op, left, right)
            }
            Expr::Ternary(test, then, otherwise) => {
                if self.eval(test)? != 0 {
                    self.eval(then)
                } else {
                    self.eval(otherwise)
                }
            }
            Expr::Assign(name, op, value) => {
                let value = self.eval(value)?;
                let value = if *op == "=" {
                    value
                } else {
                    let current = self.variable(name)?;
                    Self::apply(op.trim_end_matches('='), current, value)?
                };
                self.vars.set(name, value);
                Ok(value)
            }
            Expr::Step {
                name,
                delta,
                prefix,
            } => {
                let current = self.variable(name)?;
                let next = current.wrapping_add(*delta);
                self.vars.set(name, next);
                Ok(if *prefix { next } else { current })
            }
        }
    }
}

/// Evaluates an expression whose `$` expansions have already been done.
/// An empty expression is 0.
pub fn eval(source: &str, vars: &mut dyn Vars) -> Result<i64, String> {
    match parse(source)? {
        Some(expr) => Evaluator { vars, depth: 0 }.eval(&expr),
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Map(HashMap<String, String>);

    impl Vars for Map {
        fn get(&mut self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
        fn set(&mut self, name: &str, value: i64) {
            self.0.insert(name.to_owned(), value.to_string());
        }
    }

    fn calc(source: &str) -> i64 {
        eval(source, &mut Map::default()).unwrap()
    }

    #[test]
    fn follows_c_precedence() {
        assert_eq!(calc("1 + 2 * 3"), 7);
        assert_eq!(calc("(1 + 2) * 3"), 9);
        assert_eq!(calc("2 ** 3 ** 2"), 512);
        assert_eq!(calc("-2 ** 2"), 4);
        assert_eq!(calc("7 / 2 + 7 % 2"), 4);
        assert_eq!(calc("1 < 2 && 3 > 4 || !0"), 1);
        assert_eq!(calc("5 > 3 ? 10 : 20"), 10);
        assert_eq!(calc("1 << 4 | 1"), 17);
        assert_eq!(calc(""), 0);
    }

    #[test]
    fn reads_number_bases() {
        assert_eq!(calc("0x1f + 010 + 2#101"), 31 + 8 + 5);
    }

    #[test]
    fn assigns_and_steps() {
        let mut vars = Map::default();
        vars.0.insert("i".into(), "5".into());
        assert_eq!(eval("i++", &mut vars).unwrap(), 5);
        assert_eq!(vars.0["i"], "6");
        assert_eq!(eval("++i", &mut vars).unwrap(), 7);
        assert_eq!(eval("i += 3, i * 2", &mut vars).unwrap(), 20);
        assert_eq!(vars.0["i"], "10");
        assert_eq!(eval("x = y = 4", &mut vars).unwrap(), 4);
        assert_eq!(vars.0["x"], "4");
    }

    #[test]
    fn short_circuits() {
        let mut vars = Map::default();
        assert_eq!(eval("0 && (x = 1)", &mut vars).unwrap(), 0);
        assert!(!vars.0.contains_key("x"));
        assert_eq!(eval("1 || (x = 1)", &mut vars).unwrap(), 1);
        assert!(!vars.0.contains_key("x"));
    }

    #[test]
    fn evaluates_variables_holding_expressions() {
        let mut vars = Map::default();
        vars.0.insert("a".into(), "2 + 3".into());
        assert_eq!(eval("a * 2", &mut vars).unwrap(), 10);
        assert_eq!(eval("unset_var + 1", &mut vars).unwrap(), 1);
    }

    #[test]
    fn reports_errors() {
        let mut vars = Map::default();
        assert_eq!(eval("1 / 0", &mut vars).unwrap_err(), "division by 0");
        assert!(eval("1 +", &mut vars).is_err());
        assert!(eval("2 3", &mut vars).is_err());
        assert!(eval("09", &mut vars).is_err());
    }
}
