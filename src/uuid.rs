use time::macros::format_description;
use time::OffsetDateTime;
use uuid::Uuid;

/// JS-friendly ISO format: always UTC + exactly 3 subsecond digits.
pub const JS_ISO_FORMAT: &[time::format_description::BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");

/// xBrowserSync wire format: UUIDv4 as 32 lowercase hex chars, no dashes.
pub fn new_sync_id() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn valid_sync_id(s: &str) -> bool {
    if s.len() != 32 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_hexdigit())
}

/// JS `Date.toISOString()` equivalent: `YYYY-MM-DDTHH:MM:SS.sssZ` (UTC, 3ms digits).
pub fn now_iso() -> String {
    let now = OffsetDateTime::now_utc();
    format_rfc3339_millis(now)
}

pub fn format_rfc3339_millis(t: OffsetDateTime) -> String {
    t.format(&JS_ISO_FORMAT)
        .unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".to_string())
}

pub fn parse_rfc3339(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

pub fn timestamps_equal(a: &str, b: &str) -> bool {
    match (parse_rfc3339(a), parse_rfc3339(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_ids_are_hex32_v4() {
        for _ in 0..64 {
            let id = new_sync_id();
            assert_eq!(id.len(), 32);
            assert!(valid_sync_id(&id));
            let parsed = Uuid::parse_str(&id).unwrap();
            assert_eq!(parsed.get_version_num(), 4);
        }
    }

    #[test]
    fn invalid_sync_ids_rejected() {
        assert!(!valid_sync_id(""));
        assert!(!valid_sync_id("short"));
        assert!(!valid_sync_id(&"a".repeat(33)));
        assert!(!valid_sync_id(&"g".repeat(32)));
    }

    #[test]
    fn iso_format_matches_js_to_iso_string() {
        // JS: new Date(Date.UTC(2026, 8, 27, 1, 2, 3, 456)).toISOString()
        let t = parse_rfc3339("2026-09-27T01:02:03.456Z").unwrap();
        assert_eq!(format_rfc3339_millis(t), "2026-09-27T01:02:03.456Z");
        // zero ms must keep 3 digits
        let t0 = parse_rfc3339("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(format_rfc3339_millis(t0), "2024-01-01T00:00:00.000Z");
        // sub-ms digits truncated (a JS Date only holds whole ms)
        let t3 = parse_rfc3339("2024-01-01T00:00:00.999999Z").unwrap();
        assert_eq!(format_rfc3339_millis(t3), "2024-01-01T00:00:00.999Z");
    }

    #[test]
    fn timestamps_compared_by_value() {
        assert!(timestamps_equal(
            "2026-09-27T01:02:03.456Z",
            "2026-09-27T01:02:03.456Z"
        ));
        assert!(timestamps_equal(
            "2026-09-27T01:02:03.4Z",
            "2026-09-27T01:02:03.400Z"
        ));
        assert!(!timestamps_equal(
            "2026-09-27T01:02:03.456Z",
            "2026-09-27T01:02:03.457Z"
        ));
        assert!(!timestamps_equal("2026-09-27T01:02:03.456Z", "garbage"));
    }
}
