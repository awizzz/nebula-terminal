//! Windows command-line rules for the arguments of a custom profile: the user types one
//! line, the profile stores the list of arguments, and the editor shows the line again.

use std::iter::repeat_n;

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// Splits a line into arguments the way the Microsoft C runtime does for `argv[1..]`:
/// whitespace separates arguments outside double quotes, `\"` is a literal quote, and
/// backslashes are only special right before a quote. Inside quotes, `""` is a literal
/// quote.
pub fn split(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| is_space(*c)).is_some() {}
        if chars.peek().is_none() {
            return args;
        }

        let mut arg = String::new();
        let mut quoted = false;
        while let Some(&c) = chars.peek() {
            match c {
                c if is_space(c) && !quoted => break,
                '\\' => {
                    let mut count = 0;
                    while chars.next_if_eq(&'\\').is_some() {
                        count += 1;
                    }
                    if chars.peek() == Some(&'"') {
                        arg.extend(repeat_n('\\', count / 2));
                        if count % 2 == 1 {
                            arg.push('"');
                            chars.next();
                        }
                    } else {
                        arg.extend(repeat_n('\\', count));
                    }
                }
                '"' => {
                    chars.next();
                    if quoted && chars.next_if_eq(&'"').is_some() {
                        arg.push('"');
                    } else {
                        quoted = !quoted;
                    }
                }
                _ => {
                    arg.push(c);
                    chars.next();
                }
            }
        }
        args.push(arg);
    }
}

/// Quotes one argument so that [`split`] gives it back unchanged.
pub fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains(|c: char| is_space(c) || c == '"') {
        return arg.to_owned();
    }
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.extend(repeat_n('\\', backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    quoted.extend(repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// The inverse of [`split`].
pub fn join<S: AsRef<str>>(args: &[S]) -> String {
    args.iter()
        .map(|arg| quote(arg.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn splits_on_whitespace_outside_quotes() {
        assert_eq!(
            split("  -NoLogo\t-File  run.ps1 "),
            strings(&["-NoLogo", "-File", "run.ps1"])
        );
        assert_eq!(split(""), Vec::<String>::new());
        assert_eq!(split("   "), Vec::<String>::new());
    }

    #[test]
    fn keeps_quoted_spaces_and_empty_arguments() {
        assert_eq!(
            split(r#"-File "C:\My Scripts\start.ps1" "" end"#),
            strings(&["-File", r"C:\My Scripts\start.ps1", "", "end"])
        );
        assert_eq!(split(r#"a"b c"d"#), strings(&["ab cd"]));
    }

    #[test]
    fn follows_the_backslash_rules() {
        // Backslashes are literal unless they precede a quote.
        assert_eq!(split(r"C:\dir\ next"), strings(&[r"C:\dir\", "next"]));
        assert_eq!(split(r#"a\"b"#), strings(&[r#"a"b"#]));
        assert_eq!(split(r#"a\\"b c""#), strings(&[r"a\b c"]));
        assert_eq!(split(r#"a\\\"b"#), strings(&[r#"a\"b"#]));
        assert_eq!(split(r#""C:\dir\\""#), strings(&[r"C:\dir\"]));
    }

    #[test]
    fn doubled_quotes_inside_quotes_are_literal() {
        assert_eq!(split(r#""say ""hi"" now""#), strings(&[r#"say "hi" now"#]));
    }

    #[test]
    fn unterminated_quote_runs_to_the_end() {
        assert_eq!(split(r#"-c "echo hi"#), strings(&["-c", "echo hi"]));
    }

    #[test]
    fn quoting_round_trips() {
        let cases = [
            vec![],
            strings(&["plain", "-x"]),
            strings(&["", "two words", "tab\there"]),
            strings(&[
                r"C:\Program Files\",
                r#"quote " inside"#,
                r#"\""#,
                r"\\server\share",
            ]),
            strings(&[r#"ends with \"#, r#"a"b"#, r#""""#, "ünïcode ✓"]),
        ];
        for args in cases {
            assert_eq!(split(&join(&args)), args, "line: {}", join(&args));
        }
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(join(&["-d", "Ubuntu", "--cd", "~"]), "-d Ubuntu --cd ~");
        assert_eq!(quote(r"C:\Program Files\x"), r#""C:\Program Files\x""#);
        assert_eq!(quote(""), r#""""#);
    }
}
