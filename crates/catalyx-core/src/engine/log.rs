//! Lines for the activity log: what happened, in which area, and how it went.
//!
//! The engine names the area and the outcome; a skin decides how that looks. That is the whole
//! point: while the skin guessed a colour from the words of a line, every rewording silently
//! changed what the person saw, and a new message had to be added to a keyword list in the skin.

use std::fmt;

/// Bits on a [`LogLine`].
///
/// At most one of `OK`, `WARN` and `ERROR` is set; no tone bit at all means a plain, factual line
/// (the common case). The area bits say what the line is about, which a skin may use to group or
/// filter without reading the text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Tags(u16);

impl Tags {
    // ---- how it went (at most one)
    /// The change was made and checked (or the game was found): the good news.
    pub const OK: Tags = Tags(1 << 0);
    /// Something the person asked for did not happen, but nothing is broken.
    pub const WARN: Tags = Tags(1 << 1);
    /// Something failed and may need attention.
    pub const ERROR: Tags = Tags(1 << 2);

    // ---- what it was about (at most one)
    pub const POWER: Tags = Tags(1 << 3);
    pub const WINDOWS: Tags = Tags(1 << 4);
    pub const APPS: Tags = Tags(1 << 5);
    pub const SERVICES: Tags = Tags(1 << 6);
    pub const MEMORY: Tags = Tags(1 << 7);
    pub const GPU: Tags = Tags(1 << 8);
    pub const GAME: Tags = Tags(1 << 9);
    pub const DESKTOP: Tags = Tags(1 << 10);
    pub const RESTORE: Tags = Tags(1 << 11);

    /// Both sets together, for a line that belongs to an area and has an outcome.
    pub const fn and(self, other: Tags) -> Tags {
        Tags(self.0 | other.0)
    }

    /// Whether any of `other`'s bits is set.
    pub const fn has(self, other: Tags) -> bool {
        self.0 & other.0 != 0
    }
}

/// One line of the activity log. `Display` prints the text, so the command line keeps printing
/// exactly what it printed before without knowing about tags.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub text: String,
    pub tags: Tags,
}

impl LogLine {
    pub fn new(tags: Tags, text: impl Into<String>) -> Self {
        LogLine { text: text.into(), tags }
    }
}

impl fmt::Display for LogLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_combine_and_test_without_losing_either_part() {
        let tags = Tags::POWER.and(Tags::ERROR);
        assert!(tags.has(Tags::POWER));
        assert!(tags.has(Tags::ERROR));
        assert!(!tags.has(Tags::OK));
        assert!(!tags.has(Tags::GPU));
    }

    #[test]
    fn a_plain_line_has_no_tone() {
        let plain = LogLine::new(Tags::SERVICES, "Stopped a service");
        assert!(!plain.tags.has(Tags::OK) && !plain.tags.has(Tags::WARN) && !plain.tags.has(Tags::ERROR));
    }

    #[test]
    fn a_line_prints_its_text_and_nothing_else() {
        let line = LogLine::new(Tags::GAME.and(Tags::OK), "Found the game: game.exe (pid 7)");
        assert_eq!(line.to_string(), "Found the game: game.exe (pid 7)");
    }
}
