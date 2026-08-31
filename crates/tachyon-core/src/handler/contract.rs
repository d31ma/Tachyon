//! Immutable, language-independent route declarations captured at discovery.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tachyon_contracts::HttpMethod;

/// Conventional route declaration file, colocated with `yon.*`.
pub(crate) const CONTRACT_FILE: &str = "OPTIONS.schema.json";
const MAX_CONTRACT_BYTES: usize = 256 * 1_024;

/// One route's declared operations and CHEX schemas.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteContract {
    /// Description presented in the generated API reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Operations keyed by upper-case HTTP method.
    pub methods: BTreeMap<String, MethodContract>,
}

/// Independently optional request boundaries enforced before invoking Yon.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestContract {
    /// Declared headers only; unrelated HTTP headers are not offered to CHEX.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<serde_json::Value>,
    /// The closed set of matched dynamic route parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    /// Request JSON body, interpreted by CHEX rather than the framework.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

/// One operation's request rules and documented response shapes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodContract {
    /// Description of the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Enforced incoming request rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<RequestContract>,
    /// Documented 2xx response body; response validation is not performed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<serde_json::Value>,
    /// Documented 4xx response body.
    #[serde(
        default,
        rename = "clientError",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_error: Option<serde_json::Value>,
    /// Documented 5xx response body.
    #[serde(
        default,
        rename = "serverError",
        skip_serializing_if = "Option::is_none"
    )]
    pub server_error: Option<serde_json::Value>,
}

impl RouteContract {
    /// Parses already captured source bytes; never reopens authored paths.
    ///
    /// # Errors
    /// Rejects oversized, malformed, empty, or unknown-method declarations.
    pub fn from_bytes(portable: &str, bytes: &[u8]) -> Result<Self, Failure> {
        if bytes.len() > MAX_CONTRACT_BYTES {
            return Err(contract_failure(
                portable,
                "Route contract exceeds 256 KiB.",
            ));
        }
        let contract: Self = serde_json::from_slice(bytes).map_err(|_| {
            contract_failure(portable, "Route contract is not a valid declaration.")
        })?;
        if contract.methods.is_empty() || contract.methods.keys().any(|name| method(name).is_none())
        {
            return Err(contract_failure(
                portable,
                "Declare at least one upper-case HTTP method.",
            ));
        }
        Ok(contract)
    }

    /// Declared methods in canonical order, including implicit HEAD and OPTIONS.
    #[must_use]
    pub fn methods(&self) -> Vec<HttpMethod> {
        let mut methods: Vec<_> = self
            .methods
            .keys()
            .filter_map(|name| method(name))
            .collect();
        if methods.contains(&HttpMethod::Get) && !methods.contains(&HttpMethod::Head) {
            methods.push(HttpMethod::Head);
        }
        if !methods.contains(&HttpMethod::Options) {
            methods.push(HttpMethod::Options);
        }
        methods.sort_by_key(|value| format!("{value:?}"));
        methods
    }
}

fn method(name: &str) -> Option<HttpMethod> {
    Some(match name {
        "DELETE" => HttpMethod::Delete,
        "GET" => HttpMethod::Get,
        "HEAD" => HttpMethod::Head,
        "OPTIONS" => HttpMethod::Options,
        "PATCH" => HttpMethod::Patch,
        "POST" => HttpMethod::Post,
        "PUT" => HttpMethod::Put,
        _ => return None,
    })
}

fn contract_failure(portable: &str, message: &str) -> Failure {
    Failure::one(diagnostic(
        2005,
        message,
        Some(String::from(
            "Declare methods and optional CHEX request headers, parameters and body in OPTIONS.schema.json.",
        )),
        source_span(portable, 0, portable.len()),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::RouteContract;
    use tachyon_contracts::HttpMethod;

    #[test]
    fn parses_a_contract_and_implicit_methods() {
        let contract = RouteContract::from_bytes("OPTIONS.schema.json", br#"{"methods":{"GET":{"ok":{"id":"^.+$"}},"POST":{"request":{"body":{"name":"^.+$"}}}}}"#).expect("contract");
        assert_eq!(
            contract.methods(),
            [
                HttpMethod::Get,
                HttpMethod::Head,
                HttpMethod::Options,
                HttpMethod::Post
            ]
        );
        assert!(
            contract.methods["POST"]
                .request
                .as_ref()
                .expect("request")
                .body
                .is_some()
        );
    }

    #[test]
    fn malformed_or_oversized_contracts_fail_closed() {
        for source in [
            "{}",
            r#"{"methods":{}}"#,
            r#"{"methods":{"get":{}}}"#,
            r#"{"methods":{"FETCH":{}}}"#,
            r#"{"methods":{"GET":{}},"unknown":1}"#,
            r#"{"methods":{"GET":{"request":{"unknown":1}}}}"#,
            "not json",
        ] {
            assert!(RouteContract::from_bytes("OPTIONS.schema.json", source.as_bytes()).is_err());
        }
        assert!(
            RouteContract::from_bytes("OPTIONS.schema.json", &vec![b' '; 256 * 1_024 + 1]).is_err()
        );
    }
}
