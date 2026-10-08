/// A unit of message text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// A printable character; `char_index` is its position in the text
    /// (used to look up its color).
    Char { ch: char, char_index: usize },
    /// A built-in icon written as `~0`..`~9` or `~a`..`~f`.
    Icon { index: usize, char_index: usize },
}

/// Splits text into characters and `~x` icon escapes. Line breaks are
/// dropped (the badge has a single line), and a `~` not followed by a valid
/// icon digit is kept as a literal character.
pub fn parse_text(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '~'
            && let Some(index) = chars.get(i + 1).and_then(|c| c.to_digit(16))
        {
            let next = chars[i + 1];
            if !next.is_ascii_uppercase() {
                out.push(Token::Icon {
                    index: index as usize,
                    char_index: i,
                });
                i += 2;
                continue;
            }
        }
        if ch != '\r' && ch != '\n' {
            out.push(Token::Char { ch, char_index: i });
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        let t = parse_text("a~1~f~~x~G\n");
        assert_eq!(
            t,
            vec![
                Token::Char { ch: 'a', char_index: 0 },
                Token::Icon {
                    index: 1,
                    char_index: 1
                },
                Token::Icon {
                    index: 15,
                    char_index: 3
                },
                Token::Char { ch: '~', char_index: 5 },
                Token::Char { ch: '~', char_index: 6 },
                Token::Char { ch: 'x', char_index: 7 },
                Token::Char { ch: '~', char_index: 8 },
                Token::Char { ch: 'G', char_index: 9 },
            ]
        );
    }
}
