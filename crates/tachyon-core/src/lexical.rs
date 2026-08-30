//! Language-aware masking for source-level annotation discovery.

/// Executable bytes in one source and the first malformed lexical construct.
pub(crate) struct LexicalMask {
    pub(crate) code: Vec<bool>,
    pub(crate) error: Option<&'static str>,
}

impl LexicalMask {
    fn complete(code: Vec<bool>) -> Self {
        Self { code, error: None }
    }

    fn malformed(mut code: Vec<bool>, start: usize, message: &'static str) -> Self {
        code[start..].fill(false);
        Self {
            code,
            error: Some(message),
        }
    }
}

fn csharp_verbatim_end(bytes: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let quote = bytes[at..].iter().position(|byte| *byte == b'"')?;
        at += quote + 1;
        if bytes.get(at) == Some(&b'"') {
            at += 1;
        } else {
            return Some(at);
        }
    }
}

fn php_heredoc_end(source: &str, start: usize) -> Result<usize, &'static str> {
    let bytes = source.as_bytes();
    let line_end = source[start..]
        .find('\n')
        .map(|offset| start + offset)
        .ok_or("Malformed PHP heredoc")?;
    let label = source[start + 3..line_end].trim().trim_matches(['\'', '"']);
    if label.is_empty()
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("Malformed PHP heredoc label");
    }
    let mut at = line_end + 1;
    while at <= bytes.len() {
        let next = source[at..]
            .find('\n')
            .map_or(bytes.len(), |offset| at + offset);
        if source[at..next].trim().trim_end_matches(';') == label {
            return Ok(if next < bytes.len() { next + 1 } else { next });
        }
        if next == bytes.len() {
            break;
        }
        at = next + 1;
    }
    Err("Unterminated PHP heredoc")
}

/// Marks bytes that are executable source, excluding comments and every
/// multiline/string form accepted by Tachyon's eight Yon languages.
#[allow(clippy::too_many_lines)] // One ordered lexical state machine; splitting changes precedence.
pub(crate) fn code_mask(extension: &str, source: &str) -> LexicalMask {
    let bytes = source.as_bytes();
    let mut code = vec![true; bytes.len()];
    let mut at = 0;
    while at < bytes.len() {
        let line_comment = bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'/');
        let hash_comment = matches!(extension, "php" | "py")
            && bytes[at] == b'#'
            && bytes.get(at + 1) != Some(&b'[');
        if line_comment || hash_comment {
            let start = at;
            at += if line_comment { 2 } else { 1 };
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            code[start..at].fill(false);
            continue;
        }
        if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'*') {
            let start = at;
            at += 2;
            let mut depth = 1usize;
            while at + 1 < bytes.len() {
                if extension == "rs" && bytes[at] == b'/' && bytes[at + 1] == b'*' {
                    depth += 1;
                    at += 2;
                } else if bytes[at] == b'*' && bytes[at + 1] == b'/' {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    at += 1;
                }
            }
            if depth != 0 {
                return LexicalMask::malformed(code, start, "Unterminated block comment");
            }
            code[start..at].fill(false);
            continue;
        }
        if matches!(extension, "java" | "kt" | "cs" | "py")
            && bytes.get(at..at + 3) == Some(&b"\"\"\""[..])
        {
            let start = at;
            at += 3;
            let Some(close) = source[at..].find("\"\"\"") else {
                return LexicalMask::malformed(code, start, "Unterminated multiline string");
            };
            at += close + 3;
            code[start..at].fill(false);
            continue;
        }
        if extension == "py" && bytes.get(at..at + 3) == Some(&b"'''"[..]) {
            let start = at;
            at += 3;
            let Some(close) = source[at..].find("'''") else {
                return LexicalMask::malformed(code, start, "Unterminated Python multiline string");
            };
            at += close + 3;
            code[start..at].fill(false);
            continue;
        }
        if matches!(extension, "js" | "ts") && bytes[at] == b'`' {
            let start = at;
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == b'`' {
                    at += 1;
                    closed = true;
                    break;
                } else {
                    at += 1;
                }
            }
            if !closed {
                return LexicalMask::malformed(
                    code,
                    start,
                    "Unterminated JavaScript template string",
                );
            }
            code[start..at].fill(false);
            continue;
        }
        if extension == "rs" && bytes[at] == b'r' {
            let start = at;
            let mut quote = at + 1;
            while bytes.get(quote) == Some(&b'#') {
                quote += 1;
            }
            if bytes.get(quote) == Some(&b'"') {
                let hashes = quote - at - 1;
                at = quote + 1;
                let closing = format!("\"{}", "#".repeat(hashes));
                let Some(close) = source[at..].find(&closing) else {
                    return LexicalMask::malformed(code, start, "Unterminated Rust raw string");
                };
                at += close + closing.len();
                code[start..at].fill(false);
                continue;
            }
        }
        if extension == "cs" && bytes[at] == b'@' && bytes.get(at + 1) == Some(&b'"') {
            let start = at;
            let Some(end) = csharp_verbatim_end(bytes, at + 2) else {
                return LexicalMask::malformed(code, start, "Unterminated C# verbatim string");
            };
            at = end;
            code[start..at].fill(false);
            continue;
        }
        if extension == "php" && bytes.get(at..at + 3) == Some(&b"<<<"[..]) {
            let start = at;
            match php_heredoc_end(source, at) {
                Ok(end) => at = end,
                Err(message) => return LexicalMask::malformed(code, start, message),
            }
            code[start..at].fill(false);
            continue;
        }
        if bytes[at] == b'"' || bytes[at] == b'\'' {
            if extension == "rs"
                && bytes[at] == b'\''
                && bytes
                    .get(at + 1)
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
                && bytes.get(at + 2) != Some(&b'\'')
            {
                // A Rust lifetime is code, not a character literal.
                at += 1;
                continue;
            }
            let start = at;
            let quote = bytes[at];
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == quote {
                    at += 1;
                    closed = true;
                    break;
                } else {
                    at += 1;
                }
            }
            if !closed {
                return LexicalMask::malformed(code, start, "Unterminated string");
            }
            code[start..at].fill(false);
            continue;
        }
        at += 1;
    }
    LexicalMask::complete(code)
}
