use super::Scope;
use serde_json::Value;

/// Built-in browser context for a client-rendered Tac route.
pub(crate) fn client_route_context() -> Scope {
    Scope::from([
        (
            String::from("environment"),
            Value::String(String::from("web")),
        ),
        (String::from("os"), Value::String(String::from("web"))),
        (String::from("platform"), Value::String(String::from("web"))),
        (String::from("target"), Value::String(String::from("web"))),
    ])
}

#[cfg(test)]
mod tests {
    use super::client_route_context;

    #[test]
    fn client_context_contains_only_browser_builtins() {
        let context = client_route_context();
        assert_eq!(context.len(), 4);
        for name in ["environment", "os", "platform", "target"] {
            assert_eq!(context[name], "web");
        }
    }
}
