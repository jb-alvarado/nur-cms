use emval::{ValidationError, validate_email};

const HUMAN_TEXT_SCORE: i32 = 10;

/// Result of the text evaluation
#[derive(Debug)]
pub struct TextScore {
    pub score: i32,
    pub passed: bool,
}

pub async fn validate_email_address(email: String) -> Result<String, ValidationError> {
    let val_email = validate_email(email).await?;
    let normalized_email = val_email.normalized;

    Ok(normalized_email)
}

/// Public API: evaluate whether a text looks like meaningful human input
pub fn evaluate_text(text: &str, valid_score: Option<i32>) -> TextScore {
    let mut score = 0;

    score += score_length_and_words(text);
    score += score_letter_ratio(text);
    score += score_vowel_ratio(text);
    score += score_repetition(text);
    score += score_word_structure(text);
    score += score_character_diversity(text);

    // Thresholds:
    // ≥ 15	 very likely legitimate human text
    // 10–14 legitimate human text
    // 5–9   borderline (log or soft-reject)
    // < 5   very likely bot or random input
    let passed = score >= valid_score.unwrap_or(HUMAN_TEXT_SCORE);

    TextScore { score, passed }
}

/* ------------------------------------------------------------ */
/* Heuristics                                                   */
/* ------------------------------------------------------------ */

fn score_length_and_words(text: &str) -> i32 {
    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();

    match (char_count, word_count) {
        (c, w) if c >= 50 && w >= 7 => 5,
        (c, w) if c >= 30 && w >= 5 => 3,
        (c, w) if c >= 20 && w >= 4 => 1,
        (c, _) if c >= 15 => 0,
        _ => -5,
    }
}

fn score_letter_ratio(text: &str) -> i32 {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let total = text.chars().count().max(1);
    let ratio = letters as f32 / total as f32;

    if ratio >= 0.7 {
        3
    } else if ratio >= 0.55 {
        1
    } else if ratio >= 0.4 {
        0
    } else {
        -4
    }
}

fn score_vowel_ratio(text: &str) -> i32 {
    // Broad coverage of common European vowels
    let vowels = "aeiouyäöüáéíóúàèìòùâêîôûåæœAEIOUYÄÖÜÁÉÍÓÚÀÈÌÒÙÂÊÎÔÛÅÆŒ";

    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 10 {
        return -2;
    }

    let vowel_count = letters.iter().filter(|c| vowels.contains(**c)).count();
    let ratio = vowel_count as f32 / letters.len() as f32;

    if letters.len() < 20 {
        // Short text: less reward for good vowel ratio
        if (0.30..=0.50).contains(&ratio) {
            1
        } else if (0.25..=0.55).contains(&ratio) {
            0
        } else {
            -2
        }
    } else if (0.25..=0.55).contains(&ratio) {
        4
    } else if (0.20..=0.65).contains(&ratio) {
        1
    } else {
        -3
    }
}

fn score_repetition(text: &str) -> i32 {
    let mut last = None;
    let mut run = 0;
    let mut max_run = 0;

    for c in text.chars() {
        if Some(c) == last {
            run += 1;
        } else {
            run = 1;
            last = Some(c);
        }
        max_run = max_run.max(run);
    }

    match max_run {
        1..=3 => 2,
        4 => 0,
        5..=6 => -2,
        _ => -5,
    }
}

fn score_word_structure(text: &str) -> i32 {
    let mut long_words = 0;

    for word in text.split_whitespace() {
        let len = word.chars().count();
        if len > 30 {
            return -5;
        }
        if len > 20 {
            long_words += 1;
        }
    }

    match long_words {
        0 => 2,
        1 => 0,
        _ => -2,
    }
}

fn score_character_diversity(text: &str) -> i32 {
    use std::collections::HashSet;

    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 10 {
        return -3;
    }

    let unique: HashSet<char> = chars.iter().cloned().collect();
    let ratio = unique.len() as f32 / chars.len() as f32;

    if chars.len() < 20 {
        // Short text: less reward for diversity
        if ratio >= 0.4 {
            1
        } else if ratio >= 0.25 {
            0
        } else {
            -2
        }
    } else if ratio >= 0.4 {
        2
    } else if ratio >= 0.25 {
        0
    } else {
        -3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /* ------------------------------------------------------------ */
    /* Clearly valid human input                                    */
    /* ------------------------------------------------------------ */

    #[test]
    fn valid_german_text_passes() {
        let text = "Hallo, ich habe eine Frage zu Ihrem Produkt und würde mich über eine kurze Rückmeldung freuen.";
        let result = evaluate_text(text, None);

        assert!(result.passed, "Expected valid German text to pass");
        assert!(result.score >= 10);
    }

    #[test]
    fn valid_english_text_passes() {
        let text = "Hello, I would like to know more about your services and pricing options. Thank you in advance.";
        let result = evaluate_text(text, None);

        assert!(result.passed, "Expected valid English text to pass");
        assert!(result.score >= 10);
    }

    #[test]
    fn valid_french_text_passes() {
        let text = "Bonjour, je souhaiterais obtenir plus d'informations concernant votre offre. Merci beaucoup.";
        let result = evaluate_text(text, None);

        assert!(result.passed, "Expected valid French text to pass");
        assert!(result.score >= 10);
    }

    /* ------------------------------------------------------------ */
    /* Clearly invalid / bot-like input                              */
    /* ------------------------------------------------------------ */

    #[test]
    fn random_alphanumeric_string_fails() {
        let text = "a8S9D0asD90ASD09asd09ASD09";
        let result = evaluate_text(text, None);

        assert!(!result.passed, "Random alphanumeric input should fail");
        assert!(result.score < 5);
    }

    #[test]
    fn repeated_characters_fail() {
        let text = "aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let result = evaluate_text(text, None);

        assert!(!result.passed, "Repeated characters should fail");
        assert!(result.score < 0);
    }

    #[test]
    fn extremely_short_text_fails() {
        let text = "Hi";
        let result = evaluate_text(text, None);

        assert!(!result.passed, "Extremely short input should fail");
        assert!(result.score < 5);
    }

    /* ------------------------------------------------------------ */
    /* Borderline cases                                              */
    /* ------------------------------------------------------------ */

    #[test]
    fn short_but_meaningful_text_is_borderline() {
        let text = "Please contact me.";
        let result = evaluate_text(text, None);

        println!("{}", result.score);

        assert!(
            result.score >= 5 && result.score < 10,
            "Short but meaningful text should be borderline"
        );
    }

    #[test]
    fn mixed_text_with_numbers_is_borderline() {
        let text = "Hello, my order number is 483920 and I need help.";
        let result = evaluate_text(text, None);

        assert!(
            result.passed || (result.score >= 5 && result.score < 10),
            "Mixed text with numbers should not be hard-rejected"
        );
    }

    /* ------------------------------------------------------------ */
    /* Edge cases                                                    */
    /* ------------------------------------------------------------ */

    #[test]
    fn long_single_word_fails() {
        let text = "ThisIsAnExtremelyLongSingleWordThatShouldDefinitelyBeRejectedByTheValidator";
        let result = evaluate_text(text, None);

        assert!(!result.passed, "Single very long word should fail");
    }

    #[test]
    fn high_character_diversity_passes() {
        let text = "This message contains a wide variety of characters and should be accepted without issues.";
        let result = evaluate_text(text, None);

        assert!(result.passed);
        assert!(result.score >= 10);
    }

    #[test]
    fn whitespace_only_fails() {
        let text = "          ";
        let result = evaluate_text(text, None);

        assert!(!result.passed, "Whitespace-only input should fail");
    }
}
