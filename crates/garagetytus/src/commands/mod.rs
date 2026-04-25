pub mod about;
pub mod bootstrap;
pub mod bucket;
pub mod cluster;
pub mod install;
pub mod metrics;
pub mod start;

/// Strict duration grammar: `30m | 1h | 24h | 7d | permanent`.
/// Returns `Ok(None)` for `permanent`. Carved verbatim from
/// `makakoo-os/makakoo/src/commands/perms.rs:72` per Phase A.2.
pub fn parse_duration(s: &str) -> anyhow::Result<Option<chrono::Duration>> {
    if s == "permanent" {
        return Ok(None);
    }
    let trimmed = s.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty duration; expected 30m | 1h | 24h | 7d | permanent");
    }
    // Accept exactly the strict grammar.
    let allowed = [
        ("30m", chrono::Duration::minutes(30)),
        ("1h", chrono::Duration::hours(1)),
        ("24h", chrono::Duration::hours(24)),
        ("7d", chrono::Duration::days(7)),
    ];
    for (lit, d) in allowed {
        if trimmed == lit {
            return Ok(Some(d));
        }
    }
    anyhow::bail!(
        "duration `{}` not recognized; expected 30m | 1h | 24h | 7d | permanent",
        trimmed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_accepts_strict_grammar() {
        assert_eq!(
            parse_duration("30m").unwrap(),
            Some(chrono::Duration::minutes(30))
        );
        assert_eq!(
            parse_duration("1h").unwrap(),
            Some(chrono::Duration::hours(1))
        );
        assert_eq!(
            parse_duration("24h").unwrap(),
            Some(chrono::Duration::hours(24))
        );
        assert_eq!(
            parse_duration("7d").unwrap(),
            Some(chrono::Duration::days(7))
        );
        assert_eq!(parse_duration("permanent").unwrap(), None);
    }

    #[test]
    fn parse_duration_rejects_natural_language() {
        for bad in [
            "30 minutes",
            "1 hour",
            "two hours",
            "forever",
            "until tomorrow",
            "1d", // not on the grammar
            "60m",
        ] {
            assert!(parse_duration(bad).is_err(), "{} should error", bad);
        }
    }

    #[test]
    fn parse_duration_rejects_empty() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("   ").is_err());
    }
}
