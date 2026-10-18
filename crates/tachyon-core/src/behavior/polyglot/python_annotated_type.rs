pub(crate) struct PythonAnnotatedType;

impl PythonAnnotatedType {
    const PREFIXES: [&'static str; 2] = ["Annotated[", "typing.Annotated["];

    pub(crate) fn base(written: &str) -> &str {
        Self::arguments(written)
            .and_then(Self::first_argument)
            .unwrap_or(written)
    }

    fn arguments(written: &str) -> Option<&str> {
        Self::PREFIXES
            .into_iter()
            .find_map(|prefix| written.strip_prefix(prefix))
            .and_then(|inner| inner.strip_suffix(']'))
    }

    fn first_argument(arguments: &str) -> Option<&str> {
        arguments
            .bytes()
            .enumerate()
            .scan(0_i32, |depth, (at, byte)| {
                *depth += Self::depth_change(byte);
                Some((*depth, at, byte))
            })
            .find_map(|(depth, at, byte)| Self::top_level_comma(depth, at, byte))
            .map(|at| arguments[..at].trim())
    }

    fn top_level_comma(depth: i32, at: usize, byte: u8) -> Option<usize> {
        (depth == 0 && byte == b',').then_some(at)
    }

    fn depth_change(byte: u8) -> i32 {
        match byte {
            b'[' | b'(' => 1,
            b']' | b')' => -1,
            _ => 0,
        }
    }
}
