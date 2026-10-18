use super::{CppLexer, CppToken};
use std::ops::Range;
use std::path::Path;

pub(crate) struct CppSource {
    pub(crate) tokens: Vec<CppToken>,
    pub(crate) directives: Vec<Range<usize>>,
}

impl CppSource {
    pub(crate) const EXTENSION: &'static str = "hpp";

    pub(crate) fn is_cpp(path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension == Self::EXTENSION)
    }

    pub(crate) fn either<T>(
        path: &Path,
        source: &str,
        cpp: impl FnOnce(&Path, &str) -> T,
        other: impl FnOnce(&Path, &str) -> T,
    ) -> T {
        Self::routed(Self::is_cpp(path), path, source, cpp, other)
    }

    fn routed<T>(
        is_cpp: bool,
        path: &Path,
        source: &str,
        cpp: impl FnOnce(&Path, &str) -> T,
        other: impl FnOnce(&Path, &str) -> T,
    ) -> T {
        if is_cpp {
            return cpp(path, source);
        }
        other(path, source)
    }

    pub(crate) fn read(source: &str) -> Result<Self, &'static str> {
        let mask = crate::lexical::code_mask(Self::EXTENSION, source);
        let code = Self::visible(source, &mask.code);
        mask.error.map_or(Ok(()), Err)?;
        let directives = Self::directive_lines(&code);
        let without = Self::without_directives(&code, &directives);
        Ok(Self {
            tokens: CppLexer::tokens(&without),
            directives,
        })
    }

    fn visible(source: &str, code: &[bool]) -> String {
        let bytes: Vec<u8> = source
            .bytes()
            .zip(code.iter())
            .map(|(byte, shown)| Self::shown_byte(byte, *shown))
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn shown_byte(byte: u8, shown: bool) -> u8 {
        if shown || byte == b'\n' { byte } else { b' ' }
    }

    fn directive_lines(code: &str) -> Vec<Range<usize>> {
        let mut found = Vec::new();
        let mut offset = 0;
        for line in code.split_inclusive('\n') {
            found.extend(Self::directive_at(line, offset));
            offset += line.len();
        }
        found
    }

    fn directive_at(line: &str, offset: usize) -> Option<Range<usize>> {
        let trimmed = line.trim_start();
        let start = offset + line.len() - trimmed.len();
        trimmed
            .starts_with('#')
            .then(|| start..offset + line.trim_end_matches(['\n', '\r']).len())
    }

    fn without_directives(code: &str, directives: &[Range<usize>]) -> String {
        let mut text = code.to_owned();
        for range in directives {
            text.replace_range(range.clone(), &" ".repeat(range.len()));
        }
        text
    }
}
