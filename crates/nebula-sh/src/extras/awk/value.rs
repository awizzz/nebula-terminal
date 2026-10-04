//! awk values and their conversions: numbers, strings, and "strnums", the strings
//! that come from input and compare as numbers when they look like one.

use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Value {
    Uninit,
    Num(f64),
    Str(Rc<str>),
    /// Text from a field, `getline`, `-v` or the environment.
    StrNum(Rc<str>),
}

impl Value {
    pub fn str(text: impl Into<Rc<str>>) -> Value {
        Value::Str(text.into())
    }

    pub fn strnum(text: impl Into<Rc<str>>) -> Value {
        Value::StrNum(text.into())
    }

    pub fn to_num(&self) -> f64 {
        match self {
            Value::Uninit => 0.0,
            Value::Num(value) => *value,
            Value::Str(text) | Value::StrNum(text) => str_to_num(text),
        }
    }

    /// The string form, numbers formatted with `convfmt` unless they are integers.
    pub fn to_str(&self, convfmt: &str) -> Rc<str> {
        match self {
            Value::Uninit => Rc::from(""),
            Value::Num(value) => Rc::from(num_to_str(*value, convfmt)),
            Value::Str(text) | Value::StrNum(text) => text.clone(),
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Uninit => false,
            Value::Num(value) => *value != 0.0,
            Value::Str(text) => !text.is_empty(),
            Value::StrNum(text) => match looks_numeric(text) {
                Some(value) => value != 0.0,
                None => !text.is_empty(),
            },
        }
    }

    /// The number this value compares as, if it compares as a number at all.
    pub fn numeric(&self) -> Option<f64> {
        match self {
            Value::Uninit => Some(0.0),
            Value::Num(value) => Some(*value),
            Value::Str(_) => None,
            Value::StrNum(text) => looks_numeric(text),
        }
    }
}

/// The leading number of a string, as `strtod` reads it; 0 when there is none.
pub fn str_to_num(text: &str) -> f64 {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c') {
        i += 1;
    }
    let start = i;
    if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - digits_start;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let fraction_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - fraction_start;
    }
    if digits == 0 {
        return 0.0;
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        let mut j = i + 1;
        if j < bytes.len() && matches!(bytes[j], b'+' | b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    text[start..i].parse().unwrap_or(0.0)
}

/// A whole string that reads as a decimal number, blanks around it allowed.
pub fn looks_numeric(text: &str) -> Option<f64> {
    let trimmed = text.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r'));
    if trimmed.is_empty() {
        return None;
    }
    let body = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut chars = body.char_indices().peekable();
    while let Some((_, c)) = chars.peek().copied() {
        match c {
            '0'..='9' => seen_digit = true,
            '.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        chars.next();
    }
    if !seen_digit {
        return None;
    }
    if let Some((index, 'e' | 'E')) = chars.peek().copied() {
        let exponent = &body[index + 1..];
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    } else if chars.peek().is_some() {
        return None;
    }
    trimmed.parse().ok()
}

/// Integers print as integers; anything else goes through `CONVFMT` or `OFMT`.
pub fn num_to_str(value: f64, format: &str) -> String {
    if value.fract() == 0.0 && value.is_finite() {
        if value.abs() < 1e16 {
            return format!("{}", value as i64);
        }
        return format!("{value:.0}");
    }
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan"
        } else {
            "+nan"
        }
        .to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "+inf" }.to_owned();
    }
    sprintf(format, &[Value::Num(value)], format).unwrap_or_else(|_| format!("{value}"))
}

struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: Option<usize>,
    precision: Option<usize>,
}

fn pad(body: String, spec: &Spec, numeric: bool) -> String {
    let Some(width) = spec.width else {
        return body;
    };
    let len = body.chars().count();
    if len >= width {
        return body;
    }
    let fill = width - len;
    if spec.left {
        return body + &" ".repeat(fill);
    }
    if spec.zero && numeric {
        // Zeros go after the sign and any 0x prefix.
        let sign_len = body
            .find(|c: char| !matches!(c, '+' | '-' | ' '))
            .unwrap_or(0);
        let prefix_len = if body[sign_len..].starts_with("0x") || body[sign_len..].starts_with("0X")
        {
            sign_len + 2
        } else {
            sign_len
        };
        return format!(
            "{}{}{}",
            &body[..prefix_len],
            "0".repeat(fill),
            &body[prefix_len..]
        );
    }
    " ".repeat(fill) + &body
}

/// The sign C prints in front of a number, negative zero included.
fn sign(value: f64, spec: &Spec) -> &'static str {
    if value.is_sign_negative() {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    }
}

/// `{:e}` gives `1.5e3`; C wants `1.5e+03`.
fn c_exponent(text: &str, upper: bool) -> String {
    let (mantissa, exponent) = text.split_once('e').unwrap_or((text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let e = if upper { 'E' } else { 'e' };
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}{e}{sign}{:02}", exponent.abs())
}

fn special(value: f64, upper: bool) -> Option<String> {
    let text = if value.is_nan() {
        "nan"
    } else if value.is_infinite() {
        "inf"
    } else {
        return None;
    };
    Some(if upper {
        text.to_uppercase()
    } else {
        text.to_owned()
    })
}

fn format_e(value: f64, precision: usize, upper: bool, alt: bool) -> String {
    let mut text = format!("{:.*e}", precision, value.abs());
    if alt && precision == 0 {
        text = text.replacen('e', ".e", 1);
    }
    c_exponent(&text, upper)
}

fn format_f(value: f64, precision: usize, alt: bool) -> String {
    let mut text = format!("{:.*}", precision, value.abs());
    if alt && precision == 0 {
        text.push('.');
    }
    text
}

fn format_g(value: f64, precision: usize, upper: bool, alt: bool) -> String {
    let precision = precision.max(1);
    let exponent = if value == 0.0 {
        0
    } else {
        let probe = format!("{:.*e}", precision - 1, value.abs());
        probe
            .split_once('e')
            .and_then(|(_, exp)| exp.parse::<i32>().ok())
            .unwrap_or(0)
    };
    let mut text = if exponent < -4 || exponent >= precision as i32 {
        format_e(value, precision - 1, upper, alt)
    } else {
        format_f(
            value,
            (precision as i32 - 1 - exponent).max(0) as usize,
            alt,
        )
    };
    if !alt {
        // Drop trailing zeros of the fraction, keeping any exponent.
        let (number, exponent_part) = match text.find(['e', 'E']) {
            Some(index) => (text[..index].to_owned(), text[index..].to_owned()),
            None => (text.clone(), String::new()),
        };
        if number.contains('.') {
            let trimmed = number.trim_end_matches('0').trim_end_matches('.');
            text = format!("{trimmed}{exponent_part}");
        }
    }
    text
}

fn to_int(value: f64) -> i64 {
    if value.is_nan() {
        0
    } else {
        value.trunc().clamp(i64::MIN as f64, i64::MAX as f64) as i64
    }
}

/// `printf` and `sprintf`: `%c %d %i %o %x %X %u %e %E %f %F %g %G %s %%`, with flags,
/// width and precision, `*` taking them from the arguments. Like gawk, a format that
/// asks for more arguments than it gets is an error.
pub fn sprintf(format: &str, args: &[Value], convfmt: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut args = args.iter();
    let missing = || format!("not enough arguments for the format `{format}'");
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        let mut spec = Spec {
            left: false,
            plus: false,
            space: false,
            alt: false,
            zero: false,
            width: None,
            precision: None,
        };
        let mut raw = String::from("%");
        while let Some(&flag) = chars.peek() {
            match flag {
                '-' => spec.left = true,
                '+' => spec.plus = true,
                ' ' => spec.space = true,
                '#' => spec.alt = true,
                '0' => spec.zero = true,
                _ => break,
            }
            raw.push(flag);
            chars.next();
        }
        if chars.peek() == Some(&'*') {
            chars.next();
            let width = to_int(args.next().ok_or_else(missing)?.to_num());
            if width < 0 {
                spec.left = true;
            }
            spec.width = Some(width.unsigned_abs() as usize);
        } else {
            let mut digits = String::new();
            while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                digits.push(d);
                chars.next();
            }
            spec.width = digits.parse().ok();
        }
        if chars.peek() == Some(&'.') {
            chars.next();
            if chars.peek() == Some(&'*') {
                chars.next();
                let precision = to_int(args.next().ok_or_else(missing)?.to_num());
                spec.precision = (precision >= 0).then_some(precision as usize);
            } else {
                let mut digits = String::new();
                while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                    digits.push(d);
                    chars.next();
                }
                spec.precision = Some(digits.parse().unwrap_or(0));
            }
        }
        // Length modifiers (`%ld`) mean nothing in awk.
        while matches!(chars.peek(), Some('l' | 'h' | 'L' | 'q' | 'j' | 'z' | 't')) {
            chars.next();
        }
        let Some(conversion) = chars.next() else {
            out.push_str(&raw);
            break;
        };
        if !matches!(
            conversion,
            'd' | 'i' | 'o' | 'x' | 'X' | 'u' | 'c' | 's' | 'e' | 'E' | 'f' | 'F' | 'g' | 'G'
        ) {
            // Not a conversion awk knows: print it as written.
            raw.push(conversion);
            out.push_str(&raw);
            continue;
        }
        let arg = args.next().ok_or_else(missing)?.clone();
        let piece = match conversion {
            'd' | 'i' => {
                let value = arg.to_num();
                if let Some(text) = special(value, false) {
                    pad(format!("{}{text}", sign(value, &spec)), &spec, false)
                } else {
                    let int = to_int(value);
                    let mut digits = int.unsigned_abs().to_string();
                    if let Some(precision) = spec.precision {
                        if precision == 0 && int == 0 {
                            digits.clear();
                        } else if digits.len() < precision {
                            digits = "0".repeat(precision - digits.len()) + &digits;
                        }
                    }
                    let sign = if int < 0 {
                        "-"
                    } else if spec.plus {
                        "+"
                    } else if spec.space {
                        " "
                    } else {
                        ""
                    };
                    let numeric = spec.precision.is_none();
                    pad(format!("{sign}{digits}"), &spec, numeric)
                }
            }
            'o' | 'x' | 'X' | 'u' => {
                let value = arg.to_num();
                let int = to_int(value) as u64;
                let mut digits = match conversion {
                    'o' => format!("{int:o}"),
                    'x' => format!("{int:x}"),
                    'X' => format!("{int:X}"),
                    _ => int.to_string(),
                };
                if let Some(precision) = spec.precision {
                    if precision == 0 && int == 0 {
                        digits.clear();
                    } else if digits.len() < precision {
                        digits = "0".repeat(precision - digits.len()) + &digits;
                    }
                }
                if spec.alt && int != 0 {
                    match conversion {
                        'o' if !digits.starts_with('0') => digits.insert(0, '0'),
                        'x' => digits.insert_str(0, "0x"),
                        'X' => digits.insert_str(0, "0X"),
                        _ => {}
                    }
                }
                let numeric = spec.precision.is_none();
                pad(digits, &spec, numeric)
            }
            'c' => {
                let text = match &arg {
                    Value::Num(value) => char::from_u32(to_int(*value) as u32)
                        .map(String::from)
                        .unwrap_or_default(),
                    // gawk writes a NUL for an empty string.
                    other => other
                        .to_str(convfmt)
                        .chars()
                        .next()
                        .unwrap_or('\0')
                        .to_string(),
                };
                pad(text, &spec, false)
            }
            's' => {
                let text = arg.to_str(convfmt);
                let text: String = match spec.precision {
                    Some(precision) => text.chars().take(precision).collect(),
                    None => text.to_string(),
                };
                pad(text, &spec, false)
            }
            'e' | 'E' | 'f' | 'F' | 'g' | 'G' => {
                let value = arg.to_num();
                let upper = conversion.is_ascii_uppercase();
                let body = match special(value, upper && conversion != 'F') {
                    Some(text) => text,
                    None => {
                        let precision = spec.precision.unwrap_or(6);
                        match conversion {
                            'e' | 'E' => format_e(value, precision, upper, spec.alt),
                            'f' | 'F' => format_f(value, precision, spec.alt),
                            _ => format_g(value, precision, upper, spec.alt),
                        }
                    }
                };
                let finite = value.is_finite();
                pad(format!("{}{body}", sign(value, &spec)), &spec, finite)
            }
            _ => unreachable!("checked above"),
        };
        out.push_str(&piece);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(format: &str, args: &[Value]) -> String {
        sprintf(format, args, "%.6g").unwrap()
    }

    #[test]
    fn converts_numbers_like_awk() {
        assert_eq!(num_to_str(3.0, "%.6g"), "3");
        assert_eq!(num_to_str(0.1 + 0.2, "%.6g"), "0.3");
        assert_eq!(num_to_str(1e20, "%.6g"), "100000000000000000000");
        assert_eq!(num_to_str(123456789.5, "%.6g"), "1.23457e+08");
        assert_eq!(str_to_num(" 12abc"), 12.0);
        assert_eq!(str_to_num("-.5e1x"), -5.0);
        assert_eq!(str_to_num("abc"), 0.0);
        assert_eq!(looks_numeric(" 1e3 "), Some(1000.0));
        assert_eq!(looks_numeric("1e"), None);
        assert_eq!(looks_numeric("0x10"), None);
    }

    #[test]
    fn formats_like_c_printf() {
        assert_eq!(
            fmt(
                "%5.2f|%-6d|%x|%o",
                &[
                    Value::Num(3.256),
                    Value::Num(42.0),
                    Value::Num(255.0),
                    Value::Num(8.0)
                ]
            ),
            " 3.26|42    |ff|10"
        );
        assert_eq!(
            fmt(
                "%e %G %g",
                &[
                    Value::Num(1234.5),
                    Value::Num(0.00001),
                    Value::Num(100000.0)
                ]
            ),
            "1.234500e+03 1E-05 100000"
        );
        assert_eq!(
            fmt("%g %g", &[Value::Num(1e6), Value::Num(0.0001)]),
            "1e+06 0.0001"
        );
        assert_eq!(
            fmt(
                "%05d|%+d|% d|%.3d",
                &[
                    Value::Num(-42.0),
                    Value::Num(7.0),
                    Value::Num(7.0),
                    Value::Num(5.0)
                ]
            ),
            "-0042|+7| 7|005"
        );
        assert_eq!(
            fmt(
                "%c%c|%.2s|%*d",
                &[
                    Value::Num(65.0),
                    Value::str("xyz"),
                    Value::str("hello"),
                    Value::Num(4.0),
                    Value::Num(9.0)
                ]
            ),
            "Ax|he|   9"
        );
        assert_eq!(
            fmt(
                "%#x %#o %5s%%",
                &[Value::Num(255.0), Value::Num(8.0), Value::str("ab")]
            ),
            "0xff 010    ab%"
        );
        assert!(sprintf("%s %s", &[Value::str("a")], "%.6g").is_err());
        assert_eq!(fmt("%k %s", &[Value::str("a")]), "%k a");
    }
}
