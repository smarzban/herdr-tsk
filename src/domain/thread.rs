//! Normalization rules for task thread names.

/// Shape errors returned when a thread name cannot be normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadError {
    Empty,
    BadCharacter(char),
    BadFirstCharacter(char),
    OverLength { max: usize },
}

fn is_thread_body_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-' || character == '.'
}

/// Normalize one task thread name.
pub fn normalize_thread(input: &str) -> Result<String, ThreadError> {
    let mut characters = input.chars();
    let Some(first) = characters.next() else {
        return Err(ThreadError::Empty);
    };
    if !first.is_ascii_alphanumeric() {
        return Err(ThreadError::BadFirstCharacter(first));
    }
    if let Some(character) = characters.find(|character| !is_thread_body_char(*character)) {
        return Err(ThreadError::BadCharacter(character));
    }
    if input.chars().count() > 32 {
        return Err(ThreadError::OverLength { max: 32 });
    }
    Ok(input.to_ascii_lowercase())
}

/// Why a thread name was refused, for TUI and CLI presentation.
pub fn thread_refusal_message(error: ThreadError) -> String {
    match error {
        ThreadError::Empty => "thread name is empty".into(),
        ThreadError::BadFirstCharacter(_) => "thread must start with a letter or number".into(),
        ThreadError::BadCharacter(_) => {
            "thread: use letters, numbers, hyphens, and dots".into()
        }
        ThreadError::OverLength { max } => format!("thread is at most {max} characters"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_accepts_valid_shapes() {
        assert_eq!(normalize_thread("Release-2026"), Ok("release-2026".into()));
        assert_eq!(normalize_thread("a"), Ok("a".into()));
        assert_eq!(normalize_thread("9-start"), Ok("9-start".into()));
        assert_eq!(normalize_thread("V0.0.6"), Ok("v0.0.6".into()));
        assert_eq!(normalize_thread("v0.5.0"), Ok("v0.5.0".into()));
    }

    #[test]
    fn refuses_bad_chars_over_length_and_bad_first_char() {
        assert_eq!(normalize_thread(""), Err(ThreadError::Empty));
        assert_eq!(
            normalize_thread("-start"),
            Err(ThreadError::BadFirstCharacter('-'))
        );
        assert_eq!(
            normalize_thread("start_name"),
            Err(ThreadError::BadCharacter('_'))
        );
        assert_eq!(normalize_thread(&"a".repeat(32)), Ok("a".repeat(32)));
        assert_eq!(
            normalize_thread(&"a".repeat(33)),
            Err(ThreadError::OverLength { max: 32 })
        );
    }

    #[test]
    fn refusal_message_explains_the_rule() {
        assert!(thread_refusal_message(ThreadError::BadCharacter('_')).contains("dots"));
        assert!(thread_refusal_message(ThreadError::BadFirstCharacter('-')).contains("start"));
        assert!(thread_refusal_message(ThreadError::OverLength { max: 32 }).contains("32"));
    }
}
