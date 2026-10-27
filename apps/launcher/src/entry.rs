use std::io::BufRead;

/// Something to pick, as it was given on stdin: `title`, or `icon<TAB>title`, optionally
/// followed by more tab separated fields which the launcher does not look at (they are for
/// whoever gave the entry, to find out what was picked).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// A name from the icon theme, or the path of an image
    pub icon: Option<String>,
    pub title: String,
    /// The whole line, which is what is printed when this is picked
    pub line: String,
}

impl Entry {
    /// `None` for lines that have no title.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\r', '\n']);
        let mut fields = line.split('\t');

        let (icon, title) = match (fields.next(), fields.next()) {
            (Some(title), None) => (None, title),
            (Some(icon), Some(title)) => (Some(icon).filter(|icon| !icon.is_empty()).map(str::to_owned), title),
            _ => return None,
        };

        let title = title.trim();
        (!title.is_empty()).then(|| Self {
            icon,
            title: title.to_owned(),
            line: line.to_owned(),
        })
    }
}

/// Reads all the entries there are.
pub fn read(input: impl BufRead) -> Vec<Entry> {
    input
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| Entry::parse(&line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title() {
        let entry = Entry::parse("Firefox").unwrap();

        assert_eq!(entry.icon, None);
        assert_eq!(entry.title, "Firefox");
        assert_eq!(entry.line, "Firefox");
    }

    #[test]
    fn an_icon_and_a_title() {
        let entry = Entry::parse("firefox\tFirefox Web Browser").unwrap();

        assert_eq!(entry.icon.as_deref(), Some("firefox"));
        assert_eq!(entry.title, "Firefox Web Browser");
    }

    #[test]
    fn no_icon_is_an_empty_icon() {
        let entry = Entry::parse("\tgrinning face").unwrap();

        assert_eq!(entry.icon, None);
        assert_eq!(entry.title, "grinning face");
    }

    #[test]
    fn more_fields_are_kept_for_the_line_only() {
        let entry = Entry::parse("firefox\tFirefox\t/usr/share/applications/firefox.desktop\r\n").unwrap();

        assert_eq!(entry.title, "Firefox");
        assert_eq!(entry.line, "firefox\tFirefox\t/usr/share/applications/firefox.desktop");
    }

    #[test]
    fn nothing_to_show() {
        assert_eq!(Entry::parse(""), None);
        assert_eq!(Entry::parse("   "), None);
        assert_eq!(Entry::parse("icon\t"), None);
        assert_eq!(Entry::parse("\t\t"), None);
    }

    #[test]
    fn reads_what_there_is() {
        let entries = read("a\n\nb\tB\n  \nc".as_bytes());

        assert_eq!(entries.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["a", "B", "c"]);
    }
}
