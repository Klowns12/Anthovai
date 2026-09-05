//! Text cleanup that runs between parsing and chunking.
//!
//! PDF extraction in particular produces text that is correct but unusable:
//! words split across line breaks, runs of spaces, stray control characters.
//! Cleaning it here means every parser downstream sees the same shape.

/// Collapse whitespace, drop control characters, and rejoin words that a line
/// break split with a hyphen.
pub fn normalize(text: &str) -> String {
    let dehyphenated = rejoin_hyphenated(text);

    let mut out = String::with_capacity(dehyphenated.len());
    let mut blank_run = 0;

    for line in dehyphenated.lines() {
        let cleaned: String = line
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect();
        let collapsed = collapse_spaces(cleaned.trim());

        if collapsed.is_empty() {
            blank_run += 1;
            // At most one blank line survives: paragraph breaks matter, runs of
            // empty lines from a PDF do not.
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        out.push_str(&collapsed);
        out.push('\n');
    }

    out.trim().to_owned()
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = false;
    for ch in text.chars() {
        let is_space = ch == ' ' || ch == '\t';
        if is_space {
            if !last_was_space {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
        last_was_space = is_space;
    }
    out
}

/// `develop-\nment` becomes `development`. Only applied when the next line
/// starts with a lowercase letter, so a real hyphen at a line end survives.
fn rejoin_hyphenated(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::with_capacity(text.len());

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();

        let joins = trimmed.ends_with('-')
            && !trimmed.ends_with("--")
            && lines
                .get(i + 1)
                .map(|next| next.trim_start())
                .is_some_and(|next| next.chars().next().is_some_and(|c| c.is_lowercase()));

        if joins {
            out.push_str(trimmed.trim_end_matches('-'));
            let next = lines[i + 1].trim_start();
            out.push_str(next);
            out.push('\n');
            i += 2;
        } else {
            out.push_str(line);
            out.push('\n');
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_runs_of_spaces() {
        assert_eq!(normalize("a    b\tc"), "a b c");
    }

    #[test]
    fn keeps_one_blank_line_between_paragraphs() {
        // The paragraph break survives, the run of empty lines does not.
        assert_eq!(normalize("first\n\n\n\nsecond"), "first\n\nsecond");
    }

    #[test]
    fn adjacent_lines_stay_adjacent() {
        assert_eq!(normalize("first\nsecond"), "first\nsecond");
    }

    #[test]
    fn rejoins_words_split_across_lines() {
        let text = "the develop-\nment team";
        assert_eq!(normalize(text), "the development team");
    }

    #[test]
    fn leaves_a_real_trailing_hyphen_alone() {
        let text = "the score was 3-\nNext heading";
        assert!(normalize(text).contains("3-"));
    }

    #[test]
    fn strips_control_characters() {
        let text = "clean\u{0}text\u{7}here";
        assert_eq!(normalize(text), "cleantexthere");
    }

    #[test]
    fn leaves_thai_text_intact() {
        let thai = "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์";
        assert_eq!(normalize(thai), thai);
    }

    #[test]
    fn trims_the_whole_document() {
        assert_eq!(normalize("\n\n  hello  \n\n"), "hello");
    }

    #[test]
    fn an_empty_document_stays_empty() {
        assert_eq!(normalize("   \n\n  "), "");
    }
}
