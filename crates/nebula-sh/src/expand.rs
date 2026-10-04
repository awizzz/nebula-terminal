//! Word expansion: `~`, parameters, command substitution, arithmetic, field
//! splitting and globbing.

use crate::parse::{Anchor, ParamOp, Part, Word};
use glob::{MatchOptions, Pattern};

pub trait Context {
    fn var(&mut self, name: &str) -> Option<String>;
    fn set_var(&mut self, name: &str, value: &str);
    /// `$1`, `$2`…
    fn positional(&self) -> Vec<String>;
    fn substitute(&mut self, source: &str) -> String;
    /// Evaluates an arithmetic expression (with its `$` expansions not yet done).
    /// Returns `None` after reporting an error.
    fn arith(&mut self, source: &str) -> Option<i64>;
    fn home(&self) -> Option<String>;
    /// Reports an expansion error; the command that contains it does not run.
    fn fail(&mut self, message: String);
    /// Called when `$name` is used but not set (an error under `set -u`).
    fn unset(&mut self, _name: &str) {}
    /// `${name:?message}` failed; a script stops there.
    fn required(&mut self, message: String) {
        self.fail(message);
    }
    /// Every element of an array, for `name[@]` and `name[*]`. A plain variable is an
    /// array of one; an unset one has none.
    fn elements(&mut self, _name: &str) -> Vec<String> {
        Vec::new()
    }
    /// The indices or keys of an array (`${!name[@]}`).
    fn keys(&mut self, _name: &str) -> Vec<String> {
        Vec::new()
    }
}

/// `name[@]` or `name[*]`: all the elements of an array.
fn all_elements(name: &str) -> bool {
    matches!(crate::parse::split_subscript(name), Some((_, "@" | "*")))
}

/// Adds the elements of an array. Quoted `[@]` gives each element its own field,
/// like `"$@"`; `[*]` joins them with spaces.
fn push_elements(fields: &mut Vec<Field>, values: &[String], quoted: bool, separate: bool) {
    if !separate {
        push_value(fields, &values.join(" "), quoted);
        return;
    }
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            fields.push(Field::default());
        }
        if quoted {
            let field = fields.last_mut().expect("field");
            field.push(value, false);
            field.quoted = true;
        } else {
            push_value(fields, value, false);
        }
    }
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

fn push_value(fields: &mut Vec<Field>, value: &str, quoted: bool) {
    if quoted {
        let field = fields.last_mut().expect("field");
        field.push(value, false);
        field.quoted = true;
    } else {
        split_into(fields, value);
    }
}

/// Glob options for `case`, `[[ == ]]` and `${name#pattern}`: `*` matches `/` too.
pub fn text_match_options() -> MatchOptions {
    MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    }
}

/// Turns a word into a glob pattern: unquoted text keeps its wildcards, quoted text
/// and quoted expansions match literally.
pub fn pattern(word: &Word, context: &mut impl Context) -> String {
    single_with_pattern(word, context).1
}

/// Expands a word to one value and, in the same pass, to the glob pattern it
/// stands for (for `[[ $x == $pattern ]]`).
pub fn single_with_pattern(word: &Word, context: &mut impl Context) -> (String, String) {
    let mut value = String::new();
    let mut pattern = String::new();
    for part in &word.0 {
        match part {
            Part::Lit(text) => {
                value.push_str(text);
                pattern.push_str(&text.replace("[^", "[!"));
            }
            Part::Quoted(text) => {
                value.push_str(text);
                pattern.push_str(&Pattern::escape(text));
            }
            Part::Tilde => {
                let home = context.home().unwrap_or_else(|| "~".to_owned());
                pattern.push_str(&Pattern::escape(&home));
                value.push_str(&home);
            }
            other => {
                let quoted = matches!(
                    other,
                    Part::Var { quoted: true, .. }
                        | Part::Param { quoted: true, .. }
                        | Part::Subst { quoted: true, .. }
                        | Part::Arith { quoted: true, .. }
                );
                let text = expand_single(&Word(vec![other.clone()]), context);
                if quoted {
                    pattern.push_str(&Pattern::escape(&text));
                } else {
                    pattern.push_str(&text);
                }
                value.push_str(&text);
            }
        }
    }
    (value, pattern)
}

/// Matches `text` against a shell pattern built by [`pattern`].
pub fn matches(pattern: &str, text: &str) -> bool {
    match Pattern::new(pattern) {
        Ok(compiled) => compiled.matches_with(text, text_match_options()),
        Err(_) => pattern == text,
    }
}

fn char_boundaries(text: &str) -> Vec<usize> {
    let mut bounds: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    bounds.push(text.len());
    bounds
}

fn remove_prefix(value: &str, pattern: &str, longest: bool) -> String {
    let bounds = char_boundaries(value);
    let candidates: Box<dyn Iterator<Item = &usize>> = if longest {
        Box::new(bounds.iter().rev())
    } else {
        Box::new(bounds.iter())
    };
    for &end in candidates {
        if matches(pattern, &value[..end]) {
            return value[end..].to_owned();
        }
    }
    value.to_owned()
}

fn remove_suffix(value: &str, pattern: &str, longest: bool) -> String {
    let bounds = char_boundaries(value);
    let candidates: Box<dyn Iterator<Item = &usize>> = if longest {
        Box::new(bounds.iter())
    } else {
        Box::new(bounds.iter().rev())
    };
    for &start in candidates {
        if matches(pattern, &value[start..]) {
            return value[..start].to_owned();
        }
    }
    value.to_owned()
}

fn replace(value: &str, pattern: &str, replacement: &str, all: bool, anchor: Anchor) -> String {
    if pattern.is_empty() {
        return value.to_owned();
    }
    let bounds = char_boundaries(value);
    match anchor {
        Anchor::Start => {
            for &end in bounds.iter().rev() {
                if matches(pattern, &value[..end]) {
                    return format!("{replacement}{}", &value[end..]);
                }
            }
            value.to_owned()
        }
        Anchor::End => {
            for &start in &bounds {
                if matches(pattern, &value[start..]) {
                    return format!("{}{replacement}", &value[..start]);
                }
            }
            value.to_owned()
        }
        Anchor::None => {
            let mut out = String::new();
            let mut index = 0;
            let mut replaced = false;
            while index < bounds.len() - 1 {
                let start = bounds[index];
                let found = (!replaced || all)
                    .then(|| {
                        bounds[index + 1..]
                            .iter()
                            .rev()
                            .find(|&&end| matches(pattern, &value[start..end]))
                            .copied()
                    })
                    .flatten();
                match found {
                    Some(end) => {
                        out.push_str(replacement);
                        replaced = true;
                        index = bounds
                            .iter()
                            .position(|&b| b == end)
                            .unwrap_or(bounds.len());
                    }
                    None => {
                        out.push_str(&value[start..bounds[index + 1]]);
                        index += 1;
                    }
                }
            }
            out
        }
    }
}

fn substring(value: &str, offset: i64, length: Option<i64>) -> String {
    let chars: Vec<char> = value.chars().collect();
    let len = i64::try_from(chars.len()).unwrap_or(i64::MAX);
    let start = if offset < 0 {
        (len + offset).max(0)
    } else {
        offset.min(len)
    };
    let end = match length {
        None => len,
        Some(length) if length < 0 => (len + length).max(start),
        Some(length) => (start + length).min(len),
    };
    let start = usize::try_from(start).unwrap_or(0);
    let end = usize::try_from(end).unwrap_or(0);
    chars[start..end.max(start)].iter().collect()
}

fn change_case(value: &str, upper: bool, all: bool) -> String {
    let convert = |c: char| -> String {
        if upper {
            c.to_uppercase().collect()
        } else {
            c.to_lowercase().collect()
        }
    };
    if all {
        value.chars().map(convert).collect()
    } else {
        let mut chars = value.chars();
        match chars.next() {
            Some(first) => format!("{}{}", convert(first), chars.as_str()),
            None => String::new(),
        }
    }
}

fn lookup(name: &str, context: &mut impl Context) -> Option<String> {
    match name {
        "@" | "*" => {
            let params = context.positional();
            (!params.is_empty()).then(|| params.join(" "))
        }
        "#" => Some(context.positional().len().to_string()),
        _ => context.var(name),
    }
}

/// A value that must exist: reports unset variables (for `set -u`).
fn lookup_used(name: &str, context: &mut impl Context) -> String {
    match lookup(name, context) {
        Some(value) => value,
        None => {
            if !matches!(name, "@" | "*") {
                context.unset(name);
            }
            String::new()
        }
    }
}

/// Expands `${name<op>…}` to a single value.
fn parameter(name: &str, op: &ParamOp, context: &mut impl Context) -> String {
    let value = lookup(name, context);
    apply_op(name, value, op, context)
}

/// The operator of `${name<op>…}` applied to `value`, the value of `name` (or one of
/// its elements).
fn apply_op(name: &str, value: Option<String>, op: &ParamOp, context: &mut impl Context) -> String {
    let unset_or_null = |colon: bool| match &value {
        None => true,
        Some(text) => colon && text.is_empty(),
    };
    match op {
        ParamOp::Length => {
            if name == "@" || name == "*" {
                context.positional().len().to_string()
            } else if all_elements(name) {
                context.elements(name).len().to_string()
            } else {
                lookup_used(name, context).chars().count().to_string()
            }
        }
        ParamOp::Keys => context.keys(name).join(" "),
        ParamOp::Indirect => match value {
            Some(target)
                if crate::parse::is_name(&target)
                    || crate::parse::split_subscript(&target).is_some() =>
            {
                lookup(&target, context).unwrap_or_default()
            }
            Some(target) if !target.is_empty() => {
                context.fail(format!("{target}: invalid variable name"));
                String::new()
            }
            _ => String::new(),
        },
        ParamOp::Default { colon, word } => {
            if unset_or_null(*colon) {
                expand_single(word, context)
            } else {
                value.unwrap_or_default()
            }
        }
        ParamOp::Assign { colon, word } => {
            if unset_or_null(*colon) {
                let text = expand_single(word, context);
                if crate::parse::is_name(name) {
                    context.set_var(name, &text);
                } else {
                    context.fail(format!("${name}: cannot assign in this way"));
                }
                text
            } else {
                value.unwrap_or_default()
            }
        }
        ParamOp::Alternative { colon, word } => {
            if unset_or_null(*colon) {
                String::new()
            } else {
                expand_single(word, context)
            }
        }
        ParamOp::Error { colon, word } => {
            if unset_or_null(*colon) {
                let message = expand_single(word, context);
                let message = if message.is_empty() {
                    "parameter null or not set".to_owned()
                } else {
                    message
                };
                context.required(format!("{name}: {message}"));
                String::new()
            } else {
                value.unwrap_or_default()
            }
        }
        ParamOp::RemovePrefix {
            longest,
            pattern: word,
        } => {
            let pattern = pattern(word, context);
            remove_prefix(&value.unwrap_or_default(), &pattern, *longest)
        }
        ParamOp::RemoveSuffix {
            longest,
            pattern: word,
        } => {
            let pattern = pattern(word, context);
            remove_suffix(&value.unwrap_or_default(), &pattern, *longest)
        }
        ParamOp::Replace {
            all,
            anchor,
            pattern: word,
            replacement,
        } => {
            let pattern = pattern(word, context);
            let replacement = expand_single(replacement, context);
            replace(
                &value.unwrap_or_default(),
                &pattern,
                &replacement,
                *all,
                *anchor,
            )
        }
        ParamOp::Substring { offset, length } => {
            let Some(offset) = context.arith(offset) else {
                return String::new();
            };
            let length = match length {
                Some(length) => match context.arith(length) {
                    Some(length) => Some(length),
                    None => return String::new(),
                },
                None => None,
            };
            substring(&value.unwrap_or_default(), offset, length)
        }
        ParamOp::Case { upper, all } => change_case(&value.unwrap_or_default(), *upper, *all),
    }
}

/// One piece of a word during brace expansion: a character of unquoted text, or a
/// part (quoted text, a variable…) that braces can't reach into.
#[derive(Clone)]
enum Bit {
    Char(char),
    Part(Part),
}

fn bits(word: &Word) -> Vec<Bit> {
    let mut out = Vec::new();
    for part in &word.0 {
        match part {
            Part::Lit(text) => out.extend(text.chars().map(Bit::Char)),
            other => out.push(Bit::Part(other.clone())),
        }
    }
    out
}

fn rebuild(bits: &[Bit]) -> Word {
    let mut parts = Vec::new();
    let mut literal = String::new();
    for bit in bits {
        match bit {
            Bit::Char(c) => literal.push(*c),
            Bit::Part(part) => {
                if !literal.is_empty() {
                    parts.push(Part::Lit(std::mem::take(&mut literal)));
                }
                parts.push(part.clone());
            }
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Lit(literal));
    }
    Word(parts)
}

/// More would be a mistake rather than a wish (`{1..100000000}`); such a sequence stays
/// as typed.
const MAX_SEQUENCE: i64 = 100_000;

/// `1..5`, `05..10`, `10..1..3`, `a..e`: the words of a brace sequence.
fn sequence(text: &str) -> Option<Vec<String>> {
    let pieces: Vec<&str> = text.split("..").collect();
    if pieces.len() != 2 && pieces.len() != 3 {
        return None;
    }
    let step = match pieces.get(2) {
        Some(step) => step.parse::<i64>().ok()?.checked_abs()?.max(1),
        None => 1,
    };
    if let (Ok(start), Ok(end)) = (pieces[0].parse::<i64>(), pieces[1].parse::<i64>()) {
        if (end - start).abs() / step >= MAX_SEQUENCE {
            return None;
        }
        let padded = |text: &str| {
            let digits = text.trim_start_matches('-');
            digits.len() > 1 && digits.starts_with('0')
        };
        let width = if padded(pieces[0]) || padded(pieces[1]) {
            pieces[0].len().max(pieces[1].len())
        } else {
            0
        };
        let mut out = Vec::new();
        let mut value = start;
        loop {
            out.push(if value < 0 {
                format!("-{:0>width$}", -value, width = width.saturating_sub(1))
            } else {
                format!("{value:0>width$}")
            });
            if value == end {
                break;
            }
            let next = if end > start {
                value + step
            } else {
                value - step
            };
            if (end > start && next > end) || (end < start && next < end) {
                break;
            }
            value = next;
        }
        return Some(out);
    }
    let mut first = pieces[0].chars();
    let mut last = pieces[1].chars();
    match (first.next(), first.next(), last.next(), last.next()) {
        (Some(start), None, Some(end), None)
            if start.is_ascii_alphabetic() && end.is_ascii_alphabetic() =>
        {
            let (start, end) = (start as i64, end as i64);
            let mut out = Vec::new();
            let mut value = start;
            loop {
                out.push(char::from_u32(value as u32)?.to_string());
                let next = if end >= start {
                    value + step
                } else {
                    value - step
                };
                if (end >= start && next > end) || (end < start && next < end) {
                    break;
                }
                value = next;
            }
            Some(out)
        }
        _ => None,
    }
}

/// The alternatives of the brace group that opens at `open`, or `None` when it isn't
/// one: no closing brace, or neither a comma nor a sequence inside.
fn brace_group(bits: &[Bit], open: usize) -> Option<(usize, Vec<Vec<Bit>>)> {
    let mut depth = 0;
    let mut commas = Vec::new();
    for (index, bit) in bits.iter().enumerate().skip(open) {
        match bit {
            Bit::Char('{') => depth += 1,
            Bit::Char('}') => {
                depth -= 1;
                if depth == 0 {
                    let inner = &bits[open + 1..index];
                    if !commas.is_empty() {
                        let mut alternatives = Vec::new();
                        let mut start = open + 1;
                        for comma in commas.iter().copied().chain([index]) {
                            alternatives.push(bits[start..comma].to_vec());
                            start = comma + 1;
                        }
                        return Some((index, alternatives));
                    }
                    let text: Option<String> = inner
                        .iter()
                        .map(|bit| match bit {
                            Bit::Char(c) => Some(*c),
                            Bit::Part(_) => None,
                        })
                        .collect();
                    let words = sequence(&text?)?;
                    return Some((
                        index,
                        words
                            .into_iter()
                            .map(|word| word.chars().map(Bit::Char).collect())
                            .collect(),
                    ));
                }
            }
            Bit::Char(',') if depth == 1 => commas.push(index),
            _ => {}
        }
    }
    None
}

fn expand_bits(bits: Vec<Bit>, out: &mut Vec<Word>) {
    for open in 0..bits.len() {
        if !matches!(bits[open], Bit::Char('{')) {
            continue;
        }
        if let Some((close, alternatives)) = brace_group(&bits, open) {
            for alternative in alternatives {
                let mut combined = bits[..open].to_vec();
                combined.extend(alternative);
                combined.extend_from_slice(&bits[close + 1..]);
                expand_bits(combined, out);
            }
            return;
        }
    }
    out.push(rebuild(&bits));
}

/// Brace expansion, which comes before every other one: `{a,b}`, `{1..5}`,
/// `{01..10}`, `{1..10..3}`, `{a..e}`, nested or several in a word. Only unquoted
/// braces count, and `{}` or `{x}` stay as they are.
pub fn braces(word: &Word) -> Vec<Word> {
    let has_brace = word.0.iter().any(|part| match part {
        Part::Lit(text) => text.contains('{'),
        _ => false,
    });
    if !has_brace {
        return vec![word.clone()];
    }
    let mut out = Vec::new();
    expand_bits(bits(word), &mut out);
    out
}

/// `${name[@]<op>}`: the operator applied to every element, `${name[@]:1:2}` a slice
/// of the elements, `${!name[@]}` the keys.
fn element_values(name: &str, op: &ParamOp, context: &mut impl Context) -> Vec<String> {
    let values = context.elements(name);
    match op {
        ParamOp::Keys => context.keys(name),
        ParamOp::Substring { offset, length } => {
            let Some(offset) = context.arith(offset) else {
                return Vec::new();
            };
            let count = values.len() as i64;
            let start = if offset < 0 {
                (count + offset).max(0)
            } else {
                offset.min(count)
            };
            let end = match length {
                Some(length) => match context.arith(length) {
                    Some(length) if length < 0 => (count + length).max(start),
                    Some(length) => (start + length).min(count),
                    None => return Vec::new(),
                },
                None => count,
            };
            values[start as usize..end as usize].to_vec()
        }
        ParamOp::Default { .. }
        | ParamOp::Assign { .. }
        | ParamOp::Alternative { .. }
        | ParamOp::Error { .. } => {
            let joined = (!values.is_empty()).then(|| values.join(" "));
            vec![apply_op(name, joined, op, context)]
        }
        _ => values
            .into_iter()
            .map(|value| apply_op(name, Some(value), op, context))
            .collect(),
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
            Part::Var { name, quoted: true } if name == "@" => {
                // "$@" keeps every argument as its own word.
                for (index, param) in context.positional().iter().enumerate() {
                    if index > 0 {
                        fields.push(Field::default());
                    }
                    let field = fields.last_mut().expect("field");
                    field.push(param, false);
                    field.quoted = true;
                }
            }
            Part::Var { name, quoted } if all_elements(name) => {
                let values = context.elements(name);
                push_elements(&mut fields, &values, *quoted, name.ends_with("[@]"));
            }
            Part::Var { name, quoted } => {
                let value = lookup_used(name, context);
                push_value(&mut fields, &value, *quoted);
            }
            Part::Param { name, op, quoted }
                if all_elements(name) && !matches!(**op, ParamOp::Length) =>
            {
                let values = element_values(name, op, context);
                push_elements(&mut fields, &values, *quoted, name.ends_with("[@]"));
            }
            Part::Param { name, op, quoted } => {
                let value = parameter(name, op, context);
                push_value(&mut fields, &value, *quoted);
            }
            // Only assignments hold these; anywhere else they mean nothing.
            Part::ArrayLit(_) => {}
            Part::Subst { source, quoted } => {
                let value = context.substitute(source);
                let value = value.trim_end_matches(['\n', '\r']);
                push_value(&mut fields, value, *quoted);
            }
            Part::Arith { source, quoted } => {
                let value = context
                    .arith(source)
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                push_value(&mut fields, &value, *quoted);
            }
        }
    }

    let mut out = Vec::new();
    for field in fields {
        if field.is_empty() && !field.quoted {
            continue;
        }
        if glob {
            if let Some(pattern) = field.glob_pattern() {
                let mut matches = crate::pathnames::expand(&pattern, !cfg!(windows));
                if !matches.is_empty() {
                    matches.sort_by_key(|m| m.to_lowercase());
                    out.extend(matches);
                    continue;
                }
            }
        }
        out.push(field.text());
    }
    out
}

/// Expands a word that must stay a single value (assignment values, redirect targets,
/// `case` words): no field splitting, no globbing.
pub fn expand_single(word: &Word, context: &mut impl Context) -> String {
    let quoted: Vec<Part> = word
        .0
        .iter()
        .map(|part| match part {
            Part::Var { name, .. } if name == "@" => Part::Var {
                name: "*".into(),
                quoted: true,
            },
            Part::Var { name, .. } => Part::Var {
                name: name.clone(),
                quoted: true,
            },
            Part::Param { name, op, .. } => Part::Param {
                name: name.clone(),
                op: op.clone(),
                quoted: true,
            },
            Part::Subst { source, .. } => Part::Subst {
                source: source.clone(),
                quoted: true,
            },
            Part::Arith { source, .. } => Part::Arith {
                source: source.clone(),
                quoted: true,
            },
            other => other.clone(),
        })
        .collect();
    expand(&Word(quoted), context, false).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse, Command};
    use std::collections::HashMap;

    struct Vars {
        vars: HashMap<String, String>,
        args: Vec<String>,
        errors: Vec<String>,
    }

    impl Context for Vars {
        fn var(&mut self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
        fn set_var(&mut self, name: &str, value: &str) {
            self.vars.insert(name.to_owned(), value.to_owned());
        }
        fn positional(&self) -> Vec<String> {
            self.args.clone()
        }
        fn substitute(&mut self, source: &str) -> String {
            format!("<{source}>\n")
        }
        fn arith(&mut self, source: &str) -> Option<i64> {
            source.trim().parse().ok()
        }
        fn home(&self) -> Option<String> {
            Some("/home/me".into())
        }
        fn fail(&mut self, message: String) {
            self.errors.push(message);
        }
    }

    fn vars() -> Vars {
        Vars {
            vars: [
                ("A", "one two"),
                ("E", ""),
                ("X", "x"),
                ("F", "archive.tar.gz"),
                ("P", "/usr/local/bin"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
            args: vec!["a b".into(), "c".into()],
            errors: Vec::new(),
        }
    }

    fn run_with(context: &mut Vars, source: &str) -> Vec<String> {
        let list = parse(source).unwrap();
        let Command::Simple(command) = &list.0[0].1.commands[0] else {
            panic!("not a simple command");
        };
        command
            .words
            .iter()
            .flat_map(|w| expand(w, context, false))
            .collect()
    }

    fn run(source: &str) -> Vec<String> {
        run_with(&mut vars(), source)
    }

    fn brace_words(source: &str) -> Vec<String> {
        let list = parse(source).unwrap();
        let Command::Simple(simple) = &list.0[0].1.commands[0] else {
            panic!("not a simple command")
        };
        let mut context = vars();
        simple
            .words
            .iter()
            .flat_map(braces)
            .flat_map(|word| expand(&word, &mut context, false))
            .collect()
    }

    #[test]
    fn expands_braces() {
        assert_eq!(brace_words("echo a{b,c}d"), ["echo", "abd", "acd"]);
        assert_eq!(
            brace_words("echo {1..3} {c..a}"),
            ["echo", "1", "2", "3", "c", "b", "a"]
        );
        assert_eq!(
            brace_words("echo {01..3} {0..10..5}"),
            ["echo", "01", "02", "03", "0", "5", "10"]
        );
        assert_eq!(
            brace_words("echo {a,b{1,2}}x"),
            ["echo", "ax", "b1x", "b2x"]
        );
        assert_eq!(
            brace_words("echo {a,b}{1,2}"),
            ["echo", "a1", "a2", "b1", "b2"]
        );
        assert_eq!(
            brace_words("echo {} {x} {a,b"),
            ["echo", "{}", "{x}", "{a,b"]
        );
        assert_eq!(
            brace_words("echo '{a,b}' {\"q\",$X}"),
            ["echo", "{a,b}", "q", "x"]
        );
        assert_eq!(
            brace_words("echo file.{txt,md,}"),
            ["echo", "file.txt", "file.md", "file."]
        );
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

    #[test]
    fn expands_positional_parameters() {
        assert_eq!(run("echo \"$@\""), vec!["echo", "a b", "c"]);
        assert_eq!(run("echo $@"), vec!["echo", "a", "b", "c"]);
        assert_eq!(run("echo \"$*\" $#"), vec!["echo", "a b c", "2"]);
        assert_eq!(run("echo \"x$@y\""), vec!["echo", "xa b", "cy"]);
        let mut empty = vars();
        empty.args.clear();
        assert_eq!(run_with(&mut empty, "echo \"$@\""), vec!["echo"]);
    }

    #[test]
    fn applies_parameter_operators() {
        assert_eq!(
            run("echo ${MISSING:-a b} \"${E:-default}\" ${X:+set} ${#F}"),
            vec!["echo", "a", "b", "default", "set", "14"]
        );
        assert_eq!(
            run("echo ${F%.*} ${F%%.*} ${F#*.} ${F##*.}"),
            vec!["echo", "archive.tar", "archive", "tar.gz", "gz"]
        );
        assert_eq!(
            run("echo ${P/\\/usr/~} ${P//\\//:} ${F/#arch/ARCH} ${F/%gz/xz}"),
            vec![
                "echo",
                "~/local/bin",
                ":usr:local:bin",
                "ARCHive.tar.gz",
                "archive.tar.xz"
            ]
        );
        assert_eq!(
            run("echo ${F:0:7} ${F: -2} ${F:8:-3} ${X^^} ${A^}"),
            vec!["echo", "archive", "gz", "tar", "X", "One", "two"]
        );
    }

    #[test]
    fn assigns_defaults() {
        let mut context = vars();
        assert_eq!(
            run_with(&mut context, "echo ${NEW:=value}"),
            vec!["echo", "value"]
        );
        assert_eq!(context.vars["NEW"], "value");
    }

    #[test]
    fn reports_missing_required_values() {
        let mut context = vars();
        run_with(&mut context, "echo ${MISSING:?give a value}");
        assert_eq!(context.errors, vec!["MISSING: give a value"]);
    }

    #[test]
    fn replaces_with_patterns() {
        assert_eq!(
            replace("hello world", "o", "0", true, Anchor::None),
            "hell0 w0rld"
        );
        assert_eq!(replace("hello", "l*", "L", false, Anchor::None), "heL");
        assert_eq!(replace("aaa", "a", "b", false, Anchor::None), "baa");
        assert_eq!(remove_prefix("héllo", "h?", false), "llo");
    }
}
