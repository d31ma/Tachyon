use super::RouteCalls;
use crate::yon::error_calls::{ErrorCalls, rust::RustErrors};
use crate::yon::plan::{ParameterSource, Plan, Route};
use std::fmt::Write as _;

pub(super) struct RustRoutes;

impl RustRoutes {
    pub(crate) const STREAM_SHAPE_HELP: &'static str = "A Rust @StreamRoute method returns impl Iterator<Item = Model>, or impl Iterator<Item = Result<Model, E>> when the stream can fail; Box<dyn Iterator<Item = Model>> works too";

    pub(super) fn functions(plan: &Plan) -> Result<String, String> {
        let mut output = String::new();
        let parameters = ErrorCalls::parameters(plan, "rs");
        let error_arguments = ErrorCalls::arguments(plan, "rs");
        for verb in RouteCalls::verbs(plan) {
            let _ = write!(
                output,
                "\n#[allow(non_snake_case, unused_variables)]\nfn __yon_route_{verb}__(controller: &{}, request: &__YonFrame{parameters}) -> __YonAnswer {{\n    let values = request.arguments();\n    let value = |index: usize| values.get(index).cloned().unwrap_or(Json::Null);\n    match request.route().as_str() {{\n",
                plan.controller
            );
            for route in RouteCalls::routes_for(plan, verb) {
                if route.streaming {
                    Self::check_stream(plan, route)?;
                    let _ = writeln!(
                        output,
                        "        {} => {}(request, controller.{}({})),",
                        RouteCalls::pattern(route),
                        Self::stream_call(&route.returns),
                        route.method,
                        Self::arguments(route)
                    );
                    continue;
                }
                if let Some(error_type) =
                    RustErrors::result_error(&route.returns).filter(|_| !plan.errors.is_empty())
                {
                    let reference = RustErrors::reference(&error_type);
                    let _ = writeln!(
                        output,
                        "        {} => match controller.{}({}) {{ Ok(value) => yon_response(value), Err(error) => __yon_handle_error__({reference}, request{error_arguments}) }},",
                        RouteCalls::pattern(route),
                        route.method,
                        Self::arguments(route)
                    );
                    continue;
                }
                let _ = writeln!(
                    output,
                    "        {} => yon_response(controller.{}({})),",
                    RouteCalls::pattern(route),
                    route.method,
                    Self::arguments(route)
                );
            }
            output.push_str("        _ => __YonAnswer::json(\"{\\\"error\\\":\\\"No controller method answers this route.\\\"}\").status(500),\n    }\n}\n");
        }
        Ok(output)
    }

    fn is_stream_shape(returns: &str) -> bool {
        let written: String = returns.split_whitespace().collect();
        let opaque = written.starts_with("impl") | written.starts_with("Box<dyn");
        [written.contains("Iterator<Item="), opaque] == [true, true]
    }

    fn stream_call(returns: &str) -> &'static str {
        let written: String = returns.split_whitespace().collect();
        if written.contains("Item=Result<") {
            return "YonSocket::stream_failing";
        }
        "YonSocket::stream"
    }

    fn check_stream(plan: &Plan, route: &Route) -> Result<(), String> {
        if Self::is_stream_shape(&route.returns) {
            return Ok(());
        }
        Err(format!(
            "{}; {}::{} returns '{}'",
            Self::STREAM_SHAPE_HELP,
            plan.controller,
            route.method,
            route.returns.trim()
        ))
    }

    fn arguments(route: &Route) -> String {
        let mut arguments = Vec::new();
        for (index, parameter) in route.parameters.iter().enumerate() {
            if parameter.source == ParameterSource::Request {
                arguments.push(String::from("&yon_native_request(request)"));
            } else {
                arguments.push(format!("YonRead::read(&value({index}))"));
            }
        }
        arguments.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::RustRoutes;

    #[test]
    fn a_rust_stream_returns_an_iterator_of_items() {
        for returns in [
            "impl Iterator<Item = Tick>",
            "impl Iterator<Item = Result<Tick, String>> + '_",
            "Box<dyn Iterator<Item = Tick>>",
        ] {
            assert!(RustRoutes::is_stream_shape(returns), "{returns}");
        }
    }

    #[test]
    fn other_rust_stream_results_are_refused() {
        for returns in ["Vec<Tick>", "Json", "", "std::vec::IntoIter<Tick>"] {
            assert!(!RustRoutes::is_stream_shape(returns), "{returns}");
        }
    }
}
