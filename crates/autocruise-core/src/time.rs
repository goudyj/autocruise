//! Date/time handling for French services.
//!
//! Rules used across autocruise:
//! - **Inputs** may carry an explicit offset (`2026-10-19T06:30:00Z`,
//!   `2026-10-19T08:30+02:00`). Without an offset they are read as local
//!   Europe/Paris time (`2026-10-19T08:30`).
//! - **Outputs** are RFC 3339 strings in Europe/Paris with an explicit offset
//!   (`2026-10-19T08:30:00+02:00`), so they are never ambiguous.
//! - During the autumn DST change, an ambiguous local time resolves to the
//!   earliest instant. Nonexistent local times (spring gap) are rejected.

use chrono::{
    DateTime, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeZone, Utc,
};
use chrono_tz::Tz;

/// Time zone of metropolitan France.
pub const FRANCE_TZ: Tz = chrono_tz::Europe::Paris;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimeError {
    #[error(
        "invalid date/time {0:?}: use ISO 8601, e.g. 2026-10-19T08:30 (Europe/Paris) or 2026-10-19T06:30:00Z"
    )]
    Invalid(String),
    #[error("invalid date {0:?}: use YYYY-MM-DD")]
    InvalidDate(String),
    #[error("{0} does not exist in Europe/Paris (daylight saving time change)")]
    Nonexistent(NaiveDateTime),
}

/// Current time in Europe/Paris.
pub fn now() -> DateTime<Tz> {
    Utc::now().with_timezone(&FRANCE_TZ)
}

/// Interprets a naive date-time as Europe/Paris local time.
pub fn localize(naive: NaiveDateTime) -> Result<DateTime<Tz>, TimeError> {
    match FRANCE_TZ.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Ok(dt),
        LocalResult::Ambiguous(earliest, _) => Ok(earliest),
        LocalResult::None => Err(TimeError::Nonexistent(naive)),
    }
}

/// Parses a user-supplied date-time. See the module docs for accepted forms.
pub fn parse_datetime(input: &str) -> Result<DateTime<Tz>, TimeError> {
    let s = input.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&FRANCE_TZ));
    }

    let with_offset = match s.strip_suffix(['Z', 'z']) {
        Some(stripped) => format!("{stripped}+00:00"),
        None => s.to_owned(),
    };
    for fmt in [
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%d %H:%M%:z",
        "%Y-%m-%d %H:%M:%S%:z",
    ] {
        if let Ok(dt) = DateTime::parse_from_str(&with_offset, fmt) {
            return Ok(dt.with_timezone(&FRANCE_TZ));
        }
    }

    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return localize(naive);
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return localize(date.and_time(NaiveTime::MIN));
    }

    Err(TimeError::Invalid(s.to_owned()))
}

/// Parses a user-supplied date (`YYYY-MM-DD`). A full date-time is accepted
/// too; its Europe/Paris calendar date is used.
pub fn parse_date(input: &str) -> Result<NaiveDate, TimeError> {
    let s = input.trim();
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date);
    }
    parse_datetime(s)
        .map(|dt| dt.date_naive())
        .map_err(|_| TimeError::InvalidDate(s.to_owned()))
}

/// Formats as RFC 3339 in Europe/Paris with an explicit offset.
pub fn format(dt: &DateTime<Tz>) -> String {
    dt.with_timezone(&FRANCE_TZ)
        .to_rfc3339_opts(SecondsFormat::Secs, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(input: &str) -> String {
        format(&parse_datetime(input).unwrap())
    }

    #[test]
    fn naive_input_is_paris_local_time() {
        assert_eq!(fmt("2026-10-19T08:30"), "2026-10-19T08:30:00+02:00");
        assert_eq!(fmt("2026-10-19 08:30:15"), "2026-10-19T08:30:15+02:00");
        assert_eq!(fmt("2026-12-01T08:30"), "2026-12-01T08:30:00+01:00");
    }

    #[test]
    fn explicit_offsets_are_converted() {
        assert_eq!(fmt("2026-10-19T06:30:00Z"), "2026-10-19T08:30:00+02:00");
        assert_eq!(fmt("2026-10-19T06:30Z"), "2026-10-19T08:30:00+02:00");
        assert_eq!(fmt("2026-10-19T08:30+02:00"), "2026-10-19T08:30:00+02:00");
        assert_eq!(
            fmt("2026-12-01T12:00:00+00:00"),
            "2026-12-01T13:00:00+01:00"
        );
    }

    #[test]
    fn date_only_means_midnight() {
        assert_eq!(fmt("2026-10-19"), "2026-10-19T00:00:00+02:00");
    }

    #[test]
    fn autumn_ambiguity_resolves_to_earliest() {
        // 2026-10-25 03:00 CEST -> 02:00 CET: 02:30 happens twice.
        assert_eq!(fmt("2026-10-25T02:30"), "2026-10-25T02:30:00+02:00");
    }

    #[test]
    fn spring_gap_is_rejected() {
        // 2026-03-29 02:00 CET -> 03:00 CEST: 02:30 does not exist.
        assert!(matches!(
            parse_datetime("2026-03-29T02:30"),
            Err(TimeError::Nonexistent(_))
        ));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(matches!(
            parse_datetime("tomorrow"),
            Err(TimeError::Invalid(_))
        ));
    }

    #[test]
    fn dates() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        assert_eq!(parse_date("2026-10-19").unwrap(), d);
        // 23:30 UTC on the 18th is already the 19th in Paris.
        assert_eq!(parse_date("2026-10-18T23:30:00Z").unwrap(), d);
        assert!(parse_date("19/10/2026").is_err());
    }
}
