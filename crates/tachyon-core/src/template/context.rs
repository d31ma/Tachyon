use super::Scope;
use crate::failure::{diagnostic, source_span};
use crate::{
    Failure, HandlerCancellation, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
    RouteNode,
};
use serde_json::Value;
use std::collections::BTreeSet;
use tachyon_contracts::{
    HandlerBodyEncoding, HandlerContextContribution, HandlerRequest, HttpMethod, RouteContext,
};

const MAX_CONTEXT_BYTES: usize = 1_048_576;
const MAX_CONTEXT_DEPTH: usize = 32;
const BUILTINS: &[&str] = &["environment", "os", "platform", "target"];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RouteViewContext {
    pub(crate) values: Scope,
    pub(crate) declaration: RouteContext,
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn compose_route_context(
    project_root: &std::path::Path,
    route: &RouteNode,
) -> Result<RouteViewContext, Failure> {
    let mut values = Scope::from([
        (
            String::from("environment"),
            Value::String(String::from("web")),
        ),
        (String::from("os"), Value::String(String::from("web"))),
        (String::from("platform"), Value::String(String::from("web"))),
        (String::from("target"), Value::String(String::from("web"))),
    ]);
    let mut static_exports = BTreeSet::new();
    let mut response_exports = BTreeSet::new();
    if route.handlers().is_empty() {
        return Ok(RouteViewContext {
            values,
            declaration: RouteContext::default(),
        });
    }
    let supervisor = HandlerSupervisor::new(HandlerSupervisorOptions::default())?;
    for (index, handler) in route.handlers().iter().enumerate() {
        let source = HandlerSource::discover(project_root, handler.source_path())?;
        let mut request =
            HandlerRequest::route(format!("context_{index}"), route.route(), HttpMethod::Get);
        request.operation = String::from("view.context");
        let response = supervisor
            .invoke(&source, &request, &HandlerCancellation::default())
            .await?;
        if let Some(error) = response.error {
            return Err(context_failure(
                1501,
                handler.source_path(),
                &format!("Cannot compose Yon view context: {}", error.message),
            ));
        }
        let Some(body) = response.body else {
            return Err(context_failure(
                1501,
                handler.source_path(),
                "Yon context adapter returned no response body.",
            ));
        };
        if body.encoding != HandlerBodyEncoding::Utf8 {
            return Err(context_failure(
                1501,
                handler.source_path(),
                "Yon context adapter returned a non-UTF-8 response.",
            ));
        }
        if body.data.len() > MAX_CONTEXT_BYTES {
            return Err(context_failure(
                1504,
                handler.source_path(),
                "Yon route context exceeds the 1 MiB limit.",
            ));
        }
        let contribution: HandlerContextContribution =
            serde_json::from_str(&body.data).map_err(|error| {
                context_failure(
                    1501,
                    handler.source_path(),
                    &format!("Yon context adapter returned an invalid object: {error}"),
                )
            })?;
        merge_values(
            &mut values,
            contribution.static_values,
            &mut static_exports,
            handler.source_path(),
        )?;
        if values.len().saturating_sub(BUILTINS.len()) > 1_024 {
            return Err(context_failure(
                1504,
                handler.source_path(),
                "Composed Yon route context exceeds the limit of 1,024 exports.",
            ));
        }
        merge_values(
            &mut values,
            contribution.response_values,
            &mut response_exports,
            handler.source_path(),
        )?;
        if values.len().saturating_sub(BUILTINS.len()) > 1_024 {
            return Err(context_failure(
                1504,
                handler.source_path(),
                "Composed Yon route context exceeds the limit of 1,024 exports.",
            ));
        }
        let serialized = serde_json::to_vec(&values).map_err(|error| {
            context_failure(
                1502,
                handler.source_path(),
                &format!("Yon context is not JSON-serializable: {error}"),
            )
        })?;
        if serialized.len() > MAX_CONTEXT_BYTES {
            return Err(context_failure(
                1504,
                handler.source_path(),
                "Composed Yon route context exceeds the 1 MiB limit.",
            ));
        }
    }
    Ok(RouteViewContext {
        values,
        declaration: RouteContext {
            collision_policy: tachyon_contracts::CollisionPolicy::Error,
            static_exports: static_exports.into_iter().collect(),
            response_exports: response_exports.into_iter().collect(),
        },
    })
}

fn merge_values(
    destination: &mut Scope,
    incoming: std::collections::BTreeMap<String, Value>,
    exports: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), Failure> {
    for (key, value) in incoming {
        if !valid_key(&key) || BUILTINS.contains(&key.as_str()) || !valid_json(&value, 0) {
            return Err(context_failure(
                1502,
                source,
                &format!("Yon context export '{key}' is invalid or exceeds the nesting limit."),
            ));
        }
        if destination.contains_key(&key) {
            return Err(context_failure(
                1503,
                source,
                &format!("Yon context export '{key}' collides with an existing export."),
            ));
        }
        exports.insert(key.clone());
        destination.insert(key, value);
    }
    Ok(())
}

fn valid_key(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        && value.len() <= 128
}

fn valid_json(value: &Value, depth: usize) -> bool {
    if depth > MAX_CONTEXT_DEPTH {
        return false;
    }
    match value {
        Value::Array(values) => values.iter().all(|value| valid_json(value, depth + 1)),
        Value::Object(values) => values
            .iter()
            .all(|(key, value)| key.len() <= 1_024 && valid_json(value, depth + 1)),
        _ => true,
    }
}

fn context_failure(number: u16, source: &str, message: &str) -> Failure {
    Failure::one(diagnostic(
        number,
        message,
        Some(String::from(
            "Return one bounded JSON object with unique public export names. A \
             handler in any language is asked for its view context with \
             operation 'view.context', and must answer that request with a body \
             of {\"static_values\":{...},\"response_values\":{...}} rather than \
             with page data.",
        )),
        source_span(source, 0, source.len()),
    ))
}

#[cfg(all(test, not(coverage)))]
mod tests {
    #![allow(clippy::expect_used)]

    use super::compose_route_context;
    use crate::ProjectDiscovery;
    use std::fmt::Write as _;
    use std::fs;

    #[tokio::test]
    async fn javascript_and_python_contributions_compose_canonically() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let routes = root.path().join("server/routes");
        fs::create_dir_all(&routes).unwrap_or_else(|_| unreachable!());
        fs::write(routes.join("yon.html"), "<p>{title} {currency}</p>")
            .unwrap_or_else(|_| unreachable!());
        fs::write(
            routes.join("yon.js"),
            "export class Handler { static title = 'Products'; static GET() { return { products: [1] } } }",
        )
        .unwrap_or_else(|_| unreachable!());
        fs::write(
            routes.join("yon.py"),
            "class Handler:\n    currency = 'CAD'\n",
        )
        .unwrap_or_else(|_| unreachable!());
        let project = ProjectDiscovery::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let context = compose_route_context(project.root(), &project.route_graph().routes()[0])
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(context.declaration.static_exports, ["currency", "title"]);
        assert_eq!(context.declaration.response_exports, ["products"]);
    }

    #[tokio::test]
    async fn invalid_shapes_names_depth_collisions_and_budgets_fail_closed() {
        for (source, code) in [
            (
                "export class Handler { static GET() { return [] } }",
                "TY1501",
            ),
            ("export class Handler { static ['bad-key'] = 1 }", "TY1502"),
            (
                "export class Handler { static platform = 'override' }",
                "TY1502",
            ),
            (
                "export class Handler { static value = 1; static GET() { return { value: 2 } } }",
                "TY1503",
            ),
        ] {
            let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
            let routes = root.path().join("server/routes");
            fs::create_dir_all(&routes).unwrap_or_else(|_| unreachable!());
            fs::write(routes.join("yon.html"), "<p>view</p>").unwrap_or_else(|_| unreachable!());
            fs::write(routes.join("yon.js"), source).unwrap_or_else(|_| unreachable!());
            let project =
                ProjectDiscovery::discover(root.path()).unwrap_or_else(|_| unreachable!());
            let error = compose_route_context(project.root(), &project.route_graph().routes()[0])
                .await
                .expect_err("invalid context");
            assert!(error.to_string().contains(code), "{error}");
        }

        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let routes = root.path().join("server/routes");
        fs::create_dir_all(&routes).unwrap_or_else(|_| unreachable!());
        fs::write(routes.join("yon.html"), "<p>view</p>").unwrap_or_else(|_| unreachable!());
        let mut fields = String::new();
        for index in 0..1_025 {
            write!(fields, "static value{index} = {index};").unwrap_or_else(|_| unreachable!());
        }
        fs::write(
            routes.join("yon.js"),
            format!("export class Handler {{ {fields} }}"),
        )
        .unwrap_or_else(|_| unreachable!());
        let project = ProjectDiscovery::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let error = compose_route_context(project.root(), &project.route_graph().routes()[0])
            .await
            .expect_err("context budget");
        assert!(error.to_string().contains("TY1504"));
    }
}
