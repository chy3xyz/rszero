use uuid::Uuid;

/// Generate a UUID v4 string.
pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a 12-character short ID.
pub fn generate_short_id() -> String {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// Current UTC timestamp in seconds since epoch.
pub fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Current UTC timestamp in ISO 8601 format.
pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        let id = generate_id();
        assert_eq!(id.len(), 36);
        assert!(id.contains('-'));
    }

    #[test]
    fn test_generate_short_id() {
        let id = generate_short_id();
        assert_eq!(id.len(), 12);
    }

    #[test]
    fn test_now_timestamp() {
        let ts = now_timestamp();
        assert!(ts > 1_700_000_000);
    }

    #[test]
    fn test_now_iso8601() {
        let s = now_iso8601();
        assert!(s.contains('T'));
        assert!(s.contains('+') || s.contains('Z'));
    }
}
