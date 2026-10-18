use std::fmt::Write as _;

pub(super) struct CsharpBrowserBindings;

impl CsharpBrowserBindings {
    const HTTP: &'static str = "internal static class BehaviorBrowserHttp";
    const SOCKET: &'static str = "internal static class BehaviorBrowserSocket";

    #[cfg(test)]
    pub(super) fn read(authored: &str) -> String {
        Self::read_with_runtime(authored, "")
    }

    pub(super) fn read_with_runtime(authored: &str, runtime: &str) -> String {
        let http = Self::declaring(authored, runtime, Self::HTTP);
        let socket = Self::declaring(authored, runtime, Self::SOCKET);
        Self::render(&http, &socket)
    }

    fn declaring(authored: &str, runtime: &str, class: &str) -> Vec<usize> {
        let mut namespaces = Self::namespaces(authored, class);
        namespaces.extend(Self::namespaces(runtime, class));
        namespaces.sort_unstable();
        namespaces.dedup();
        namespaces
    }

    fn namespaces(authored: &str, class: &str) -> Vec<usize> {
        authored
            .split("namespace TachyonService")
            .skip(1)
            .filter(|source| source.contains(class))
            .filter_map(Self::namespace_index)
            .collect()
    }

    fn namespace_index(source: &str) -> Option<usize> {
        let end = source.find(|character: char| !character.is_ascii_digit())?;
        source[end..]
            .trim_start()
            .starts_with('{')
            .then(|| source[..end].parse().ok())
            .flatten()
    }

    fn socket_bindings(sockets: &[usize]) -> String {
        sockets
            .iter()
            .map(|index| {
                format!(
                    "\n        TachyonService{index}.BehaviorBrowserSocket.Host ??= Tac.SendSocket;"
                )
            })
            .collect::<Vec<_>>()
            .concat()
    }

    fn render(namespaces: &[usize], sockets: &[usize]) -> String {
        let mut bindings = Self::socket_bindings(sockets);
        let mut patterns = String::new();
        for index in namespaces {
            let _ = write!(
                bindings,
                "\n        TachyonService{index}.BehaviorBrowserHttp.Capture ??= Tac.CaptureHttpTransport;"
            );
            let _ = write!(
                patterns,
                "TachyonService{index}.BehaviorError failure{index} => failure{index}.Code, "
            );
        }
        format!(
            "{bindings}\n        TacTaskAnswer.FailureCode ??= error => error switch {{ {patterns}_ => null }};"
        )
    }
}
