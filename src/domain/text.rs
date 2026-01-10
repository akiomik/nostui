use unicode_width::UnicodeWidthStr;

pub fn wrap_text(s: &str, width: usize) -> String {
    if width == 0 {
        return String::from("");
    }

    s.chars().fold(String::from(""), |acc: String, c: char| {
        let last_line = acc.lines().last().unwrap_or(&acc);
        if last_line.width() + c.to_string().width() > width {
            format!("{acc}\n{c}")
        } else {
            format!("{acc}{c}")
        }
    })
}

pub fn truncate_text(s: &str, max_height: usize) -> String {
    if max_height == 0 {
        return String::from("");
    }

    let lines: Vec<&str> = s.lines().collect();
    if lines.len() > max_height {
        if max_height == 1 {
            String::from("...")
        } else {
            #[cfg(windows)]
            {
                format!("{}\r\n...", lines[..max_height - 1].join("\r\n"))
            }
            #[cfg(not(windows))]
            {
                format!("{}\n...", lines[..max_height - 1].join("\n"))
            }
        }
    } else {
        s.to_string()
    }
}

pub fn shorten_npub(npub: impl Into<String>) -> String {
    let npub_string: String = npub.into();
    match npub_string.strip_prefix("npub1") {
        Some(stripped) => {
            let len = stripped.len();
            let heading = &stripped[0..5];
            let trail = &stripped[(len - 5)..len];
            format!("{heading}:{trail}")
        }
        None => npub_string,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_wrap_text_no_wrap_alnum() {
        let actual = wrap_text("hello, world!", 13);
        let expected = "hello, world!";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_wrap_alnum() {
        let actual = wrap_text("hello, world!", 4);
        let expected = "hell\no, w\norld\n!";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_no_wrap_double_width() {
        let actual = wrap_text("こんにちは、世界！", 18);
        let expected = "こんにちは、世界！";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_wrap_double_width() {
        let actual = wrap_text("こんにちは、世界！", 7);
        let expected = "こんに\nちは、\n世界！";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_no_wrap_emoji() {
        let actual = wrap_text("🫲🫱🫲🫱🫲🫱", 12);
        let expected = "🫲🫱🫲🫱🫲🫱";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_wrap_emoji() {
        let actual = wrap_text("🫲🫱🫲🫱🫲🫱", 5);
        let expected = "🫲🫱\n🫲🫱\n🫲🫱";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_zero_width() {
        let actual = wrap_text("hello, world!", 0);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncate_text_no_truncate() {
        let actual = truncate_text("foo\nbar\nbaz", 3);
        let expected = "foo\nbar\nbaz";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncate_text_truncate() {
        let actual = truncate_text("foo\nbar\nbaz", 2);
        let expected = "foo\n...";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncate_text_single_line() {
        let actual = truncate_text("foo\nbar", 1);
        let expected = "...";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncate_text_zero_height() {
        let actual = truncate_text("foo\nbar\nbaz", 0);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shorten_npub() {
        assert_eq!(
            shorten_npub("npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug"),
            "f5uuy:mjmug"
        );

        assert_eq!(
            shorten_npub("4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25"),
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25"
        );
    }
}
