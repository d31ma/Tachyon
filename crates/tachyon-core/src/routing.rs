//! Request-time route matching.
//!
//! The compiler produces route patterns whose dynamic segments keep their
//! authored `_name` form. This module binds a concrete request path to one of
//! those patterns and extracts the parameter values, with a fixed specificity
//! rule so matching never depends on discovery order.

use std::collections::BTreeMap;

/// Maximum path segments considered when matching one request.
const MAX_SEGMENTS: usize = 64;
/// Maximum bytes accepted in one bound parameter value.
const MAX_PARAMETER_BYTES: usize = 1_024;

/// One route pattern bound to a concrete request path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteMatch {
    /// The matched route pattern, in its authored `_name` form.
    pub route: String,
    /// Parameter values bound from the request path.
    pub parameters: BTreeMap<String, String>,
}

/// Splits a request path into non-empty segments.
fn segments(path: &str) -> Option<Vec<&str>> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() > MAX_SEGMENTS || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    Some(parts)
}

/// Attempts to bind one request path to one route pattern.
///
/// Returns the bound parameters and the pattern's specificity, where a static
/// segment is more specific than a dynamic one.
fn bind(pattern: &str, path: &[&str]) -> Option<(BTreeMap<String, String>, u32)> {
    let expected = segments(pattern)?;
    if expected.len() != path.len() {
        return None;
    }
    let mut parameters = BTreeMap::new();
    let mut specificity = 0;
    for (expected, actual) in expected.iter().zip(path) {
        if let Some(name) = expected.strip_prefix('_') {
            // A dynamic segment never matches an empty or oversized value, and
            // never spans a path separator.
            if actual.is_empty() || actual.len() > MAX_PARAMETER_BYTES {
                return None;
            }
            let decoded = percent_decode(actual)?;
            parameters.insert(String::from(name), decoded);
        } else {
            if expected != actual {
                return None;
            }
            specificity += 1;
        }
    }
    Some((parameters, specificity))
}

/// Decodes percent-encoded bytes, rejecting anything that is not valid UTF-8
/// or that smuggles a path separator or control character.
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let text = String::from_utf8(decoded).ok()?;
    if text.contains('/') || text.contains('\\') || text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Selects the single best route for a request path.
///
/// The most specific pattern wins: a route whose segments are static is
/// preferred over one that binds a parameter in the same position. Ties are
/// broken by route order, which the route graph already makes deterministic.
#[must_use]
pub fn match_route<'a, I>(routes: I, path: &str) -> Option<RouteMatch>
where
    I: IntoIterator<Item = &'a str>,
{
    let actual = segments(path)?;
    let mut best: Option<(u32, RouteMatch)> = None;
    for route in routes {
        let Some((parameters, specificity)) = bind(route, &actual) else {
            continue;
        };
        let candidate = RouteMatch {
            route: String::from(route),
            parameters,
        };
        match &best {
            Some((best_specificity, _)) if *best_specificity >= specificity => {}
            _ => best = Some((specificity, candidate)),
        }
    }
    best.map(|(_, matched)| matched)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{RouteMatch, match_route};

    const ROUTES: [&str; 5] = [
        "/",
        "/items",
        "/items/_id",
        "/items/new",
        "/items/_id/reviews/_review",
    ];

    fn matched(path: &str) -> Option<RouteMatch> {
        match_route(ROUTES, path)
    }

    #[test]
    fn static_routes_win_over_dynamic_ones_in_the_same_position() {
        // `/items/new` and `/items/_id` both match; the static one must win,
        // regardless of the order routes are declared in.
        let new = matched("/items/new").expect("static match");
        assert_eq!(new.route, "/items/new");
        assert!(new.parameters.is_empty());

        let dynamic = matched("/items/42").expect("dynamic match");
        assert_eq!(dynamic.route, "/items/_id");
        assert_eq!(dynamic.parameters["id"], "42");
    }

    #[test]
    fn nested_parameters_bind_in_order() {
        let nested = matched("/items/42/reviews/7").expect("nested match");
        assert_eq!(nested.route, "/items/_id/reviews/_review");
        assert_eq!(nested.parameters["id"], "42");
        assert_eq!(nested.parameters["review"], "7");
    }

    #[test]
    fn index_and_trailing_slashes_resolve_to_one_route() {
        for path in ["/", "", "//"] {
            assert_eq!(matched(path).expect("index").route, "/");
        }
        assert_eq!(matched("/items/").expect("trailing").route, "/items");
    }

    #[test]
    fn parameters_are_percent_decoded() {
        let decoded = matched("/items/a%20b").expect("decoded");
        assert_eq!(decoded.parameters["id"], "a b");
    }

    #[test]
    fn traversal_and_control_characters_never_bind() {
        // A parameter must never smuggle a separator or a control character
        // into a handler, however it is encoded.
        for path in [
            "/items/%2e%2e%2fetc",
            "/items/%2f",
            "/items/%5c",
            "/items/%00",
        ] {
            assert!(matched(path).is_none(), "{path} bound a parameter");
        }
        // A literal traversal segment simply does not match any pattern.
        assert!(matched("/items/../secret").is_none());
    }

    #[test]
    fn arity_and_unknown_paths_do_not_match() {
        assert!(matched("/items/42/extra").is_none());
        assert!(matched("/unknown").is_none());
        assert!(matched("/items/42/reviews").is_none());
    }

    #[test]
    fn oversized_paths_are_rejected() {
        let deep = format!("/{}", vec!["a"; 128].join("/"));
        assert!(matched(&deep).is_none());
        let long = format!("/items/{}", "x".repeat(2_048));
        assert!(matched(&long).is_none());
    }
}
