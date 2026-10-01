//! Word expansion: `~`, variables, command substitution, field splitting and globbing.

use crate::parse::{Part, Word};
use glob::{MatchOptions, Pattern};

pub trait Context {
    fn var(&self, name: &str) -> Option<String>;
    fn substitute(&mut self, source: &str) -> String;
    fn home(&self) -> Option<String>;
}

/// One output field made of segments; `true` marks text that may be treated as a glob.
#[derive(Default)]
struct Field {
    segments: Vec<(String, bool)>,
    /// A quoted part was present, so the field survives even when empty.
    quoted: bool,
}

impl Field {
    fn push(&mut self, text: &str, glob: bool) {
        self.segments.push((text.to_owned(), glob));
    }

    fn text(&self) -> String {
        self.segments
            .iter()
            .map(|(text, _)| text.as_str())
            .collect()
    }

    fn is_empty(&self) -> bool {
        self.segments.iter().all(|(text, _)| text.is_empty())
    }

    fn glob_pattern(&self) -> Option<String> {
        if !self
            .segments
            .iter()
            .any(|(text, glob)| *glob && text.contains(['*', '?', '[']))
        {
            return None;
        }
        Some(
            self.segments
                .iter()
                .map(|(text, glob)| {
                    if *glob {
                        text.clone()
                    } else {
                        Pattern::escape(text)
                    }
                })
                .collect(),
        )
    }
}

/// Splits the result of an unquoted expansion on whitespace, like bash with the default IFS.
fn split_into(fields: &mut Vec<Field>, value: &str) {
    if value.is_empty() {
        return;
    }
    let ends_field = |field: &Field| !field.is_empty() || field.quoted;
    let pieces: Vec<&str> = value.split_whitespace().collect();
    let leading = value.starts_with(char::is_whitespace);
    let trailing = value.ends_with(char::is_whitespace);
    for (index, piece) in pieces.iter().enumerate() {
        if (index > 0 || leading) && fields.last().is_some_and(ends_field) {
            fields.push(Field::default());
        }
        // Unquoted expansion results are globbed, as in bash.
        fields.last_mut().expect("field").push(piece, true);
    }
    if (trailing || pieces.is_empty()) && fields.last().is_some_and(ends_field) {
        fields.push(Field::default());
    }
}

/// Expands a word into zero or more arguments.
pub fn expand(word: &Word, context: &mut impl Context, glob: bool) -> Vec<String> {
    let mut fields = vec![Field::default()];
    for part in &word.0 {
        match part {
            Part::Lit(text) => fields.last_mut().expect("field").push(text, true),
            Part::Quoted(text) => {
                let field = fields.last_mut().expect("field");
                field.push(text, false);
                field.quoted = true;
            }
            Part::Tilde => {
                let home = context.home().unwrap_or_else(|| "~".to_owned());
                fields.last_mut().expect("field").push(&home, false);
            }
            Part::Var { name, quoted } => {
                let value = context.var(name).unwrap_or_default();
                if *quoted {
                    let field = fields.last_mut().expect("field");
                    field.push(&value, false);
                    field.quoted = true;
                } else {
                    split_into(&mut fields, &value);
                }
            }
            Part::Subst { source, quoted } => {
                let value = context.substitute(source);
                let value = value.trim_end_matches(['\n', '\r']);
                if *quoted {
                    let field = fields.last_mut().expect("field");
                    field.push(value, false);
                    field.quoted = true;
                } else {
                    split_into(&mut fields, value);
                }
            }
        }
    }

    let options = MatchOptions {
        case_sensitive: !cfg!(windows),
        require_literal_separator: true,
        require_literal_leading_dot: true,
    };
    let mut out = Vec::new();
    for field in fields {
        if field.is_empty() && !field.quoted {
            continue;
        }
        if glob {
            if let Some(pattern) = field.glob_pattern() {
                if let Ok(paths) = glob::glob_with(&pattern, options) {
                    let mut matches: Vec<String> = paths
                        .filter_map(Result::ok)
                        .map(|path| {
                            let text = path.to_string_lossy().into_owned();
                            // Keep the separator the user typed.
                            if pattern.contains('/') && !pattern.contains('\\') {
                                text.replace('\\', "/")
                            } else {
                                text
                            }
                        })
                        .collect();
                    if !matches.is_empty() {
                        matches.sort_by_key(|m| m.to_lowercase());
                        out.extend(matches);
                        continue;
                    }
                }
            }
        }
        out.push(field.text());
    }
    out
}

/// Expands a word that must stay a single value (assignment values, redirect targets).
pub fn expand_single(word: &Word, context: &mut impl Context) -> String {
    let mut fields = Vec::new();
    for part in &word.0 {
        let quoted_part = match part {
            Part::Var { name, .. } => Part::Var {
                name: name.clone(),
                quoted: true,
            },
            Part::Subst { source, .. } => Part::Subst {
                source: source.clone(),
                quoted: true,
            },
            other => other.clone(),
        };
        fields.push(quoted_part);
    }
    expand(&Word(fields), context, false).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use std::collections::HashMap;

    struct Vars(HashMap<&'static str, &'static str>);

    impl Context for Vars {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).map(|v| (*v).to_owned())
        }
        fn substitute(&mut self, source: &str) -> String {
            format!("<{source}>\n")
        }
        fn home(&self) -> Option<String> {
            Some("/home/me".into())
        }
    }

    fn run(source: &str) -> Vec<String> {
        let mut vars = Vars(HashMap::from([("A", "one two"), ("E", ""), ("X", "x")]));
        let list = parse(source).unwrap();
        list.0[0].1.commands[0]
            .words
            .iter()
            .flat_map(|w| expand(w, &mut vars, false))
            .collect()
    }

    #[test]
    fn splits_unquoted_variables_only() {
        assert_eq!(run("echo $A \"$A\""), vec!["echo", "one", "two", "one two"]);
    }

    #[test]
    fn drops_empty_unquoted_but_keeps_quoted() {
        assert_eq!(run("echo $E \"$E\" ''"), vec!["echo", "", ""]);
    }

    #[test]
    fn joins_adjacent_parts() {
        assert_eq!(
            run("echo pre$X\"post\" ~/dir"),
            vec!["echo", "prexpost", "/home/me/dir"]
        );
    }

    #[test]
    fn substitutes_commands_without_trailing_newline() {
        assert_eq!(run("echo \"$(date)\""), vec!["echo", "<date>"]);
    }
}
