use unicode_width::UnicodeWidthStr;

pub fn wrap_text(s: &str, width: usize) -> String {
    s.chars().fold(String::from(""), |acc: String, c: char| {
        let last_line = acc.lines().last().unwrap_or(&acc);
        if last_line.width() + c.to_string().width() > width {
            format!("{}\n{}", acc, c)
        } else {
            format!("{}{}", acc, c)
        }
    })
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
}
