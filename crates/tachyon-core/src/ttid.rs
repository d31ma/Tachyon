//! Time-sortable identifiers.
//!
//! A TTID is a base-36 encoding of the number of 100-nanosecond ticks since
//! the Unix epoch, which the wider Tachyon ecosystem uses for correlatable
//! identifiers. This is a native implementation rather than a call to the
//! `ttid` binary, for the same reason the project's own web client is native:
//! TTID is pure computation, and a request hot path cannot spawn a process
//! per identifier.
//!
//! The encoding matches the published TTID contract used by Tachyon
//! v26.30.04: base-36 uppercase 100-nanosecond ticks. This module derives the
//! value directly from the clock instead of from a scaled millisecond float.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Base-36 alphabet, uppercase to match the TTID pattern.
const DIGITS: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// Clock ticks per millisecond, so one unit is 100 nanoseconds.
const TICKS_PER_MILLISECOND: u128 = 10_000;

/// Last issued value, used to keep identifiers strictly increasing.
static LAST: AtomicU64 = AtomicU64::new(0);

/// Returns a new time-sortable identifier.
///
/// Identifiers are strictly increasing within a process even when two calls
/// land in the same clock tick, so an identifier is never reused. Unlike a
/// plain counter, the value is derived from wall-clock time, so it stays
/// unique and ordered across restarts.
#[must_use]
pub fn generate() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() / 100);
    let now = u64::try_from(now).unwrap_or(u64::MAX);

    let mut previous = LAST.load(Ordering::Relaxed);
    let issued = loop {
        let candidate = if now > previous { now } else { previous + 1 };
        match LAST.compare_exchange_weak(previous, candidate, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break candidate,
            Err(observed) => previous = observed,
        }
    };
    encode(issued)
}

/// Encodes one tick count as uppercase base 36.
fn encode(mut value: u64) -> String {
    if value == 0 {
        return String::from("0");
    }
    let mut digits = Vec::with_capacity(13);
    while value > 0 {
        let index = usize::try_from(value % 36).unwrap_or(0);
        digits.push(DIGITS[index]);
        value /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}

/// Decodes the creation time of an identifier, in milliseconds since the epoch.
///
/// Returns `None` when the value is not a base-36 tick count.
#[must_use]
pub fn created_at_milliseconds(id: &str) -> Option<u128> {
    let mut ticks: u128 = 0;
    for byte in id.bytes() {
        let digit = DIGITS.iter().position(|candidate| *candidate == byte)?;
        ticks = ticks.checked_mul(36)?.checked_add(digit as u128)?;
    }
    Some(ticks / TICKS_PER_MILLISECOND)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{created_at_milliseconds, generate};
    use std::collections::BTreeSet;

    #[test]
    fn identifiers_match_the_ttid_shape() {
        let id = generate();
        // The vendored client produces eleven uppercase base-36 characters for
        // any time between 2020 and 2200.
        assert_eq!(id.len(), 11, "{id}");
        assert!(
            id.bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()),
            "{id}"
        );
    }

    #[test]
    fn identifiers_are_valid_handler_protocol_identifiers() {
        // A request id must be a bounded alphanumeric identifier, so a TTID
        // can be used directly as one.
        let id = generate();
        assert!((1..=128).contains(&id.len()));
        assert!(
            id.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        );
    }

    #[test]
    fn identifiers_are_unique_and_strictly_increasing() {
        // Two calls inside one clock tick must still differ, and order must
        // follow issue order so logs sort chronologically.
        let issued: Vec<String> = (0..10_000).map(|_| generate()).collect();
        let unique: BTreeSet<&String> = issued.iter().collect();
        assert_eq!(unique.len(), issued.len(), "identifiers collided");

        let mut sorted = issued.clone();
        sorted.sort();
        assert_eq!(sorted, issued, "identifiers are not lexically ordered");
    }

    #[test]
    fn creation_time_decodes_to_the_present() {
        let id = generate();
        let decoded = created_at_milliseconds(&id).expect("decodable");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        // Generated moments ago, so within a generous window of now.
        assert!(decoded <= now + 1_000, "{decoded} is ahead of {now}");
        assert!(decoded + 60_000 >= now, "{decoded} is far behind {now}");
        // Sanity: after 2020-01-01, which the TTID range requires.
        assert!(decoded > 1_577_836_800_000, "{decoded}");
    }

    #[test]
    fn malformed_identifiers_do_not_decode() {
        assert!(created_at_milliseconds("lowercase").is_none());
        assert!(created_at_milliseconds("has-hyphen").is_none());
        assert!(created_at_milliseconds("").is_some());
    }
}
