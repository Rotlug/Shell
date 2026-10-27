//! Deciding which entries match what was typed, and which match it best.

/// How well an entry matches, the lower the better.
pub type Score = u32;

const TIER: Score = 1000;

/// How well `title` matches `query`: every word of the query has to be in it. `None` if they
/// don't all are. An empty query matches everything equally.
pub fn score(title: &str, query: &str) -> Option<Score> {
    let title = title.to_lowercase();
    let query = query.to_lowercase();

    query
        .split_whitespace()
        .map(|term| score_term(&title, term))
        .sum::<Option<Score>>()
}

/// In the order of what matches best: the beginning of the title, the beginning of a word in
/// it, anywhere in it, and last the letters in order (`ffx` for `firefox`). Earlier is better
/// within each.
fn score_term(title: &str, term: &str) -> Option<Score> {
    if let Some(position) = title.find(term) {
        let chars_before = title[..position].chars().count();

        let tier = if chars_before == 0 {
            0
        } else if title[..position].ends_with(|c: char| !c.is_alphanumeric()) {
            1
        } else {
            2
        };

        return Some(tier * TIER + score_from(chars_before));
    }

    let mut wanted = term.chars().peekable();
    let mut first = None;
    for (position, c) in title.chars().enumerate() {
        if wanted.peek() == Some(&c) {
            wanted.next();
            first.get_or_insert(position);
        }
    }

    wanted.peek().is_none().then(|| 3 * TIER + score_from(first.unwrap_or(0)))
}

#[allow(clippy::cast_possible_truncation)]
fn score_from(position: usize) -> Score {
    position.min(TIER as usize - 1) as Score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_matches_nothing() {
        assert_eq!(score("Firefox", ""), Some(0));
        assert_eq!(score("Firefox", "   "), Some(0));
    }

    #[test]
    fn ignores_case() {
        assert!(score("Firefox", "FIRE").is_some());
        assert!(score("ÉCOLE", "école").is_some());
    }

    #[test]
    fn what_does_not_match() {
        assert_eq!(score("Firefox", "chrome"), None);
        assert_eq!(score("Firefox", "xf"), None);
    }

    #[test]
    fn all_words_are_needed() {
        assert!(score("grinning face with big eyes", "face eyes").is_some());
        assert!(score("grinning face with big eyes", "eyes face").is_some());
        assert_eq!(score("grinning face", "face nose"), None);
    }

    #[test]
    fn the_better_the_match_the_lower() {
        let prefix = score("Firefox", "fire").unwrap();
        let word = score("Mozilla Firefox", "fire").unwrap();
        let inside = score("Unfired", "fire").unwrap();
        let letters = score("Fantastic Ice Recorder Experience", "fire").unwrap();

        assert!(prefix < word);
        assert!(word < inside);
        assert!(inside < letters);
    }

    #[test]
    fn earlier_is_better_in_the_same_tier() {
        assert!(score("firefox", "fox").unwrap() < score("a firefox", "fox").unwrap());
    }
}
