//! Fuzzy matching for history search (Ctrl+R) and `z`: every character of the query
//! appears in the text, in order. Matches at word starts and runs of characters rank
//! higher, and a plain substring ranks above any scattered match.

fn word_start(previous: Option<char>) -> bool {
    match previous {
        None => true,
        Some(c) => matches!(c, ' ' | '/' | '\\' | '-' | '_' | '.' | ':' | '=' | '|'),
    }
}

/// How well one term matches `text`, or `None` when it doesn't.
fn term_score(term: &str, text: &str) -> Option<i64> {
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();
    let term_lower: Vec<char> = term.to_lowercase().chars().collect();
    if term_lower.is_empty() {
        return Some(0);
    }
    let haystack: String = text_lower.iter().collect();
    let needle: String = term_lower.iter().collect();
    if let Some(byte) = haystack.find(&needle) {
        let position = haystack[..byte].chars().count();
        let at_start = word_start(position.checked_sub(1).map(|i| text_lower[i]));
        return Some(1000 + 10 * term_lower.len() as i64 + if at_start { 200 } else { 0 });
    }
    let mut score = 0;
    let mut wanted = 0;
    let mut previous_match: Option<usize> = None;
    for (index, c) in text_lower.iter().enumerate() {
        if wanted == term_lower.len() {
            break;
        }
        if *c != term_lower[wanted] {
            continue;
        }
        score += 10;
        if previous_match == index.checked_sub(1) && previous_match.is_some() {
            score += 15;
        }
        if word_start(index.checked_sub(1).map(|i| text_lower[i])) {
            score += 20;
        }
        if let Some(previous) = previous_match {
            score -= (index - previous - 1).min(10) as i64;
        }
        previous_match = Some(index);
        wanted += 1;
    }
    (wanted == term_lower.len()).then_some(score)
}

/// The score of `query` against `text`: each space-separated term must match.
pub fn score(query: &str, text: &str) -> Option<i64> {
    let mut total = 0;
    for term in query.split_whitespace() {
        total += term_score(term, text)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn ranks_substrings_and_word_starts_first() {
        assert!(score("gst", "git status").is_some());
        assert!(score("xyz", "git status").is_none());
        assert!(score("status", "git status") > score("gsts", "git status"));
        assert!(score("push", "git push origin") > score("ush", "git push origin"));
        assert!(score("git push", "git push origin").is_some());
        assert!(score("push git", "git push").is_some());
        assert!(score("PROJ", "~/projects/app").is_some());
    }
}
