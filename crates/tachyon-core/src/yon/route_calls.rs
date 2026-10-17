use super::plan::{Plan, Route};
use crate::route_types::{Shape, TypeText};

mod csharp;
mod dart;
mod go;
mod java;
mod javascript;
mod kotlin;
mod php;
mod python;
mod ruby;
mod rust;

pub(crate) struct RouteCalls;

impl RouteCalls {
    pub(crate) fn error_owners(plan: &Plan) -> Vec<(usize, &str)> {
        let mut owners = Vec::new();
        for (index, provider) in plan.providers.iter().enumerate() {
            if plan
                .errors
                .iter()
                .any(|method| method.owner == provider.name)
            {
                owners.push((index, provider.name.as_str()));
            }
        }
        owners
    }

    pub(crate) fn invocation_errors(
        route: &Route,
        controller: &str,
        separator: &str,
        request: &str,
        errors: &[String],
    ) -> String {
        if !route.dispatcher || errors.is_empty() {
            return Self::invocation_with(route, controller, separator, request);
        }
        format!(
            "{}({controller}, {request}, {})",
            route.method,
            errors.join(", ")
        )
    }

    pub(crate) fn present(plan: &Plan) -> bool {
        plan.routes.iter().any(|route| route.pattern.is_some())
    }

    pub(crate) fn verb_plan(plan: &Plan, extension: &str) -> Result<Plan, String> {
        let mut routes: Vec<Route> = Vec::new();
        for route in &plan.routes {
            if routes.iter().any(|added| added.verb == route.verb) {
                continue;
            }
            let streaming = Self::streaming(plan, &route.verb)?;
            routes.push(Route {
                verb: route.verb.clone(),
                method: Self::function_name(extension, &route.verb),
                streaming,
                takes_request: true,
                dispatcher: true,
                ..Route::default()
            });
        }
        Ok(Plan {
            providers: plan.providers.clone(),
            controller: plan.controller.clone(),
            routes,
            errors: plan.errors.clone(),
        })
    }

    pub(crate) fn borrowed(route: &Route, controller: &str) -> String {
        if route.dispatcher {
            return format!("&{controller}");
        }
        controller.to_owned()
    }

    pub(crate) fn invocation(route: &Route, controller: &str, request: &str) -> String {
        Self::invocation_with(route, controller, ".", request)
    }

    pub(crate) fn invocation_with(
        route: &Route,
        controller: &str,
        separator: &str,
        request: &str,
    ) -> String {
        if route.dispatcher {
            return format!("{}({controller}, {request})", route.method);
        }
        format!("{controller}{separator}{}({request})", route.method)
    }

    pub(crate) fn functions(
        extension: &str,
        plan: &Plan,
        stripped: &str,
    ) -> Result<String, String> {
        match extension {
            "js" => Ok(javascript::JavaScriptRoutes::functions(plan, false)),
            "ts" => Ok(javascript::JavaScriptRoutes::functions(plan, true)),
            "py" => Ok(python::PythonRoutes::functions(plan)),
            "rb" => Ok(ruby::RubyRoutes::functions(plan)),
            "php" => Ok(php::PhpRoutes::functions(plan)),
            "java" => Ok(java::JavaRoutes::functions(plan)),
            "kt" => Ok(kotlin::KotlinRoutes::functions(plan)),
            "cs" => Ok(csharp::CSharpRoutes::functions(plan)),
            "rs" => rust::RustRoutes::functions(plan),
            "dart" => Ok(dart::DartRoutes::functions(plan)),
            "go" => go::GoRoutes::functions(plan, stripped),
            _ => Err(format!(
                "@RequestRoute controllers are not available in .{extension} files yet"
            )),
        }
    }

    fn streaming(plan: &Plan, verb: &str) -> Result<bool, String> {
        let mut streaming = 0;
        let mut total = 0;
        for route in &plan.routes {
            if route.verb != verb {
                continue;
            }
            total += 1;
            if route.streaming {
                streaming += 1;
            }
        }
        if streaming != 0 && streaming != total {
            return Err(format!(
                "In one controller, either every {verb} route is a @StreamRoute or none is. Move the @StreamRoute methods to their own controller"
            ));
        }
        Ok(streaming != 0)
    }

    fn function_name(extension: &str, verb: &str) -> String {
        if extension == "rb" {
            return format!("YonGenerated.route_{verb}");
        }
        if matches!(extension, "java" | "cs") {
            return format!("__YonRoutes.__yon_route_{verb}__");
        }
        format!("__yon_route_{verb}__")
    }

    pub(crate) fn verbs(plan: &Plan) -> Vec<&str> {
        let mut verbs: Vec<&str> = Vec::new();
        for route in &plan.routes {
            if !verbs.contains(&route.verb.as_str()) {
                verbs.push(&route.verb);
            }
        }
        verbs
    }

    pub(crate) fn routes_for<'a>(plan: &'a Plan, verb: &str) -> Vec<&'a Route> {
        plan.routes
            .iter()
            .filter(|route| route.verb == verb)
            .collect()
    }

    pub(crate) fn pattern(route: &Route) -> String {
        serde_json::Value::String(route.pattern.clone().unwrap_or_default()).to_string()
    }

    pub(super) fn needs_conversion(extension: &str, type_text: &str) -> bool {
        let Ok(shape) = TypeText::shape(extension, type_text) else {
            return false;
        };
        Self::holds_model(&shape)
    }

    pub(crate) fn ruby_descriptor(shape: &Shape) -> Option<String> {
        ruby::RubyRoutes::descriptor(shape)
    }

    fn holds_model(shape: &Shape) -> bool {
        match shape {
            Shape::Model(_) => true,
            Shape::List(inner) | Shape::Optional(inner) => Self::holds_model(inner),
            _ => false,
        }
    }
}
