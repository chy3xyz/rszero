//! Data masking utilities for sensitive information.
//!
//! Provides functions to mask phone numbers, emails, ID cards, and names
//! for safe logging and response display.

/// Mask a Chinese mobile phone number (11 digits).
/// Keeps first 3 and last 4 digits.
///
/// # Example
/// ```
/// use rszero::utils::mask_phone;
/// assert_eq!(mask_phone("13800138000"), "138****8000");
/// ```
pub fn mask_phone(phone: &str) -> String {
    if phone.len() < 7 {
        return "****".to_string();
    }
    format!("{}****{}", &phone[..3], &phone[phone.len()-4..])
}

/// Mask an email address.
/// Keeps first 2 characters of local part and domain.
///
/// # Example
/// ```
/// use rszero::utils::mask_email;
/// assert_eq!(mask_email("user@example.com"), "us***@example.com");
/// ```
pub fn mask_email(email: &str) -> String {
    if let Some(at_pos) = email.find('@') {
        let local = &email[..at_pos];
        let domain = &email[at_pos..];
        let masked_local = if local.is_empty() {
            "***".to_string()
        } else {
            format!("{}***", &local[..local.len().min(2)])
        };
        format!("{}{}", masked_local, domain)
    } else {
        "***".to_string()
    }
}

/// Mask a Chinese ID card number (18 digits).
/// Keeps first 6 and last 4 digits.
///
/// # Example
/// ```
/// use rszero::utils::mask_idcard;
/// assert_eq!(mask_idcard("110101199001011234"), "110101********1234");
/// ```
pub fn mask_idcard(idcard: &str) -> String {
    if idcard.len() < 10 {
        return "****************".to_string();
    }
    let keep_start = 6;
    let keep_end = 4;
    let stars = idcard.len() - keep_start - keep_end;
    format!("{}{}{}", &idcard[..keep_start], "*".repeat(stars), &idcard[idcard.len()-keep_end..])
}

/// Mask a person's name.
/// Keeps the first character and masks the rest.
///
/// # Example
/// ```
/// use rszero::utils::mask_name;
/// assert_eq!(mask_name("张三"), "张*");
/// assert_eq!(mask_name("李小四"), "李**");
/// ```
pub fn mask_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= 1 {
        return "*".to_string();
    }
    format!("{}{}", chars[0], "*".repeat(chars.len() - 1))
}

/// Mask a bank card number.
/// Keeps last 4 digits only.
///
/// # Example
/// ```
/// use rszero::utils::mask_bankcard;
/// assert_eq!(mask_bankcard("6222021234567890123"), "***************0123");
/// ```
pub fn mask_bankcard(card: &str) -> String {
    if card.len() < 4 {
        return "****".to_string();
    }
    let stars = card.len() - 4;
    format!("{}{}", "*".repeat(stars), &card[card.len()-4..])
}

/// Generic mask: keep first `prefix` and last `suffix` characters.
pub fn mask_generic(s: &str, prefix: usize, suffix: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= prefix + suffix {
        return "*".repeat(chars.len());
    }
    let stars = chars.len() - prefix - suffix;
    format!(
        "{}{}{}",
        chars.iter().take(prefix).collect::<String>(),
        "*".repeat(stars),
        chars.iter().skip(chars.len() - suffix).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_phone() {
        assert_eq!(mask_phone("13800138000"), "138****8000");
        assert_eq!(mask_phone("123"), "****");
    }

    #[test]
    fn test_mask_email() {
        assert_eq!(mask_email("user@example.com"), "us***@example.com");
        assert_eq!(mask_email("ab@example.com"), "ab***@example.com");
        assert_eq!(mask_email("invalid"), "***");
    }

    #[test]
    fn test_mask_idcard() {
        assert_eq!(mask_idcard("110101199001011234"), "110101********1234");
    }

    #[test]
    fn test_mask_name() {
        assert_eq!(mask_name("张三"), "张*");
        assert_eq!(mask_name("李小四"), "李**");
        assert_eq!(mask_name("A"), "*");
    }

    #[test]
    fn test_mask_bankcard() {
        assert_eq!(mask_bankcard("6222021234567890123"), "***************0123");
    }

    #[test]
    fn test_mask_generic() {
        assert_eq!(mask_generic("hello_world", 2, 3), "he******rld");
    }
}
