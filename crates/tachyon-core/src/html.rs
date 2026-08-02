use crate::Failure;
use crate::failure::{diagnostic, source_span};
use html5gum::{DefaultEmitter, Token, Tokenizer};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_HTML_BYTES: u64 = 1_048_576;
const CONTROL_TAGS: &[&str] = &["else", "for", "if", "loop"];
const EVENT_HANDLER_ATTRIBUTES: &[&str] = &[
    "onabort",
    "onbeforeinput",
    "onblur",
    "oncancel",
    "oncanplay",
    "onchange",
    "onclick",
    "onclose",
    "oncontextmenu",
    "oncopy",
    "oncut",
    "ondblclick",
    "ondrag",
    "ondragend",
    "ondragenter",
    "ondragleave",
    "ondragover",
    "ondragstart",
    "ondrop",
    "onerror",
    "onfocus",
    "oninput",
    "oninvalid",
    "onkeydown",
    "onkeypress",
    "onkeyup",
    "onload",
    "onmousedown",
    "onmouseenter",
    "onmouseleave",
    "onmousemove",
    "onmouseout",
    "onmouseover",
    "onmouseup",
    "onpaste",
    "onpointercancel",
    "onpointerdown",
    "onpointerenter",
    "onpointerleave",
    "onpointermove",
    "onpointerout",
    "onpointerover",
    "onpointerup",
    "onreset",
    "onscroll",
    "onselect",
    "onsubmit",
    "ontoggle",
    "ontouchend",
    "ontouchmove",
    "ontouchstart",
    "onwheel",
];
const HTML_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "circle",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "defs",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "ellipse",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "foreignobject",
    "g",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "line",
    "lineargradient",
    "main",
    "map",
    "mark",
    "math",
    "menu",
    "meta",
    "meter",
    "mi",
    "mn",
    "mo",
    "ms",
    "mtext",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "path",
    "picture",
    "polygon",
    "polyline",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "rect",
    "s",
    "samp",
    "search",
    "section",
    "semantics",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "svg",
    "symbol",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "text",
    "u",
    "ul",
    "use",
    "var",
    "video",
    "wbr",
];

/// A validated deterministic HTML document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlDocument {
    content: String,
}

impl HtmlDocument {
    /// Returns the complete HTML document.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// The Phase 1 WHATWG-compatible HTML frontend.
#[derive(Clone, Copy, Debug, Default)]
pub struct HtmlFrontend;

impl HtmlFrontend {
    /// Reads and parses one project view.
    ///
    /// # Errors
    ///
    /// Returns stable source diagnostics for I/O failures, excessive size,
    /// invalid UTF-8, tokenizer errors, or syntax deferred beyond Phase 1.
    pub fn parse_file(path: &Path, source_path: &str) -> Result<HtmlDocument, Failure> {
        let bytes = read_bounded(path, source_path)?;
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(error) => {
                let invalid = error.utf8_error().valid_up_to();
                return Err(Failure::one(diagnostic(
                    1102,
                    format!("HTML source '{source_path}' is not valid UTF-8."),
                    Some(String::from("Save Tachyon HTML sources as UTF-8.")),
                    source_span(source_path, invalid, invalid.saturating_add(1)),
                )));
            }
        };
        Self::parse(&source, source_path)
    }

    /// Parses one in-memory view and emits a complete document.
    ///
    /// # Errors
    ///
    /// Returns stable source diagnostics when the input is empty, contains a
    /// NUL byte, has tokenizer errors, or uses deferred dynamic semantics.
    pub fn parse(source: &str, source_path: &str) -> Result<HtmlDocument, Failure> {
        if source.trim().is_empty() {
            return Err(Failure::one(diagnostic(
                1107,
                format!("HTML source '{source_path}' is empty."),
                Some(String::from("Add at least one semantic HTML element.")),
                source_span(source_path, 0, source.len()),
            )));
        }
        if let Some(index) = source.find('\0') {
            return Err(Failure::one(diagnostic(
                1102,
                format!("HTML source '{source_path}' contains a NUL byte."),
                Some(String::from(
                    "Remove the NUL byte and save the source as UTF-8.",
                )),
                source_span(source_path, index, index + 1),
            )));
        }

        let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        let analysis = analyze(&normalized, source_path)?;
        let content = if analysis.is_document {
            complete_document(&normalized, analysis.has_doctype)
        } else {
            wrap_fragment(&normalized)
        };
        Ok(HtmlDocument { content })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct HtmlAnalysis {
    has_doctype: bool,
    is_document: bool,
}

fn read_bounded(path: &Path, source_path: &str) -> Result<Vec<u8>, Failure> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return Err(html_read_error(source_path, &error)),
    };
    let mut bytes = Vec::new();
    if let Err(error) = file.take(MAX_HTML_BYTES + 1).read_to_end(&mut bytes) {
        return Err(html_read_error(source_path, &error));
    }
    if bytes.len() as u64 > MAX_HTML_BYTES {
        return Err(Failure::one(diagnostic(
            1101,
            format!("HTML source '{source_path}' exceeds the 1 MiB Phase 1 limit."),
            Some(String::from("Reduce the view below 1 MiB.")),
            source_span(source_path, 0, bytes.len()),
        )));
    }
    Ok(bytes)
}

fn html_read_error(source_path: &str, error: &std::io::Error) -> Failure {
    Failure::one(diagnostic(
        1101,
        format!("Cannot read HTML source '{source_path}': {error}"),
        None,
        source_span(source_path, 0, 0),
    ))
}

fn analyze(source: &str, source_path: &str) -> Result<HtmlAnalysis, Failure> {
    let emitter = DefaultEmitter::<usize>::new_with_span();
    let tokenizer = Tokenizer::new_with_emitter(source, emitter);
    let mut analysis = HtmlAnalysis::default();
    for result in tokenizer {
        let token = match result {
            Ok(token) => token,
            Err(infallible) => match infallible {},
        };
        match token {
            Token::Doctype(_) => {
                analysis.has_doctype = true;
                analysis.is_document = true;
            }
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).into_owned();
                if name == "html" {
                    analysis.is_document = true;
                }
                validate_tag(&name, tag.span.start, tag.span.end, source_path)?;
                for attribute in tag.attributes.keys() {
                    let attribute = String::from_utf8_lossy(attribute);
                    if EVENT_HANDLER_ATTRIBUTES.contains(&attribute.as_ref()) {
                        return Err(deferred_dynamic_html(
                            source_path,
                            tag.span.start,
                            tag.span.end,
                        ));
                    }
                }
            }
            Token::Error(error) => {
                return Err(Failure::one(diagnostic(
                    1104,
                    format!("HTML source '{source_path}' contains a tokenizer error."),
                    Some(String::from(
                        "Correct the malformed token before building the page.",
                    )),
                    source_span(source_path, error.span.start, error.span.end),
                )));
            }
            Token::EndTag(_) | Token::String(_) | Token::Comment(_) => {}
        }
    }
    Ok(analysis)
}

fn validate_tag(name: &str, start: usize, end: usize, source_path: &str) -> Result<(), Failure> {
    if CONTROL_TAGS.contains(&name) {
        return Err(Failure::one(diagnostic(
            1103,
            format!("Control tag '<{name}>' is not available until Phase 3."),
            Some(String::from(
                "Use static HTML in the Phase 1 vertical slice.",
            )),
            source_span(source_path, start, end),
        )));
    }
    if name == "script" {
        return Err(deferred_dynamic_html(source_path, start, end));
    }
    if !is_standard_html_tag(name) && !valid_custom_element(name) {
        return Err(Failure::one(diagnostic(
            1106,
            format!("Tac component '<{name}>' is not available until Phase 3."),
            Some(String::from(
                "Use a standard HTML tag or a hyphenated web-component name.",
            )),
            source_span(source_path, start, end),
        )));
    }
    Ok(())
}

fn deferred_dynamic_html(source_path: &str, start: usize, end: usize) -> Failure {
    Failure::one(diagnostic(
        1105,
        "Inline executable HTML is not supported in Phase 1.",
        Some(String::from(
            "Use static semantic HTML; controller execution arrives in Phase 3.",
        )),
        source_span(source_path, start, end),
    ))
}

pub(crate) fn is_standard_html_tag(name: &str) -> bool {
    HTML_TAGS.contains(&name)
}

pub(crate) fn is_event_handler_attribute(name: &str) -> bool {
    EVENT_HANDLER_ATTRIBUTES.contains(&name)
}

pub(crate) fn valid_custom_element(name: &str) -> bool {
    name.contains('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn complete_document(source: &str, has_doctype: bool) -> String {
    let source = source.trim();
    if has_doctype {
        format!("{source}\n")
    } else {
        format!("<!doctype html>\n{source}\n")
    }
}

fn wrap_fragment(source: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Tachyon</title>\n</head>\n<body>\n{}\n</body>\n</html>\n",
        source.trim()
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::HtmlFrontend;
    use std::fs;

    #[test]
    fn fragments_are_wrapped_and_line_endings_are_stable() {
        let document = HtmlFrontend::parse("<main>\r\nHello</main>\r\n", "client/pages/tac.html")
            .expect("valid HTML");
        assert!(document.content().starts_with("<!doctype html>"));
        assert!(document.content().contains("<main>\nHello</main>"));
        assert!(!document.content().contains('\r'));
    }

    #[test]
    fn complete_documents_are_preserved_with_a_doctype() {
        let document = HtmlFrontend::parse(
            "<html><head><title>X</title></head><body>X</body></html>",
            "client/pages/tac.html",
        )
        .expect("valid document");
        assert!(document.content().starts_with("<!doctype html>\n<html>"));
    }

    #[test]
    fn dynamic_and_unknown_tags_are_rejected() {
        for (source, code) in [
            ("<if>Later</if>", "TY1103"),
            ("<script>bad()</script>", "TY1105"),
            ("<hero>Later</hero>", "TY1106"),
        ] {
            let error =
                HtmlFrontend::parse(source, "client/pages/tac.html").expect_err("deferred syntax");
            assert!(error.to_string().contains(code));
        }
        assert!(HtmlFrontend::parse("<user-card></user-card>", "client/pages/tac.html").is_ok());
        assert!(
            HtmlFrontend::parse(
                "<svg><path d=\"M0 0\"></path></svg><div onboarding=\"done\"></div>",
                "client/pages/tac.html"
            )
            .is_ok()
        );
    }

    #[test]
    fn empty_nul_event_and_tokenizer_errors_are_rejected() {
        for (source, code) in [
            (" \n\t", "TY1107"),
            ("<main>\0</main>", "TY1102"),
            ("<button onclick=\"go()\">Go</button>", "TY1105"),
            ("<div a=\"first\" a=\"second\">Duplicate</div>", "TY1104"),
            ("<-bad></-bad>", "TY1104"),
            ("<bad-></bad->", "TY1106"),
        ] {
            let error =
                HtmlFrontend::parse(source, "client/pages/tac.html").expect_err("invalid HTML");
            assert!(error.to_string().contains(code), "{source}: {error}");
        }
    }

    #[test]
    fn file_reads_are_bounded_and_utf8_only() {
        let root = tempfile::tempdir().expect("workspace");
        let missing = root.path().join("missing.html");
        assert!(
            HtmlFrontend::parse_file(&missing, "client/pages/tac.html")
                .expect_err("missing")
                .to_string()
                .contains("TY1101")
        );

        let invalid = root.path().join("invalid.html");
        fs::write(&invalid, [0xff, 0xfe]).expect("invalid UTF-8 fixture");
        assert!(
            HtmlFrontend::parse_file(&invalid, "client/pages/tac.html")
                .expect_err("UTF-8")
                .to_string()
                .contains("TY1102")
        );

        let large = root.path().join("large.html");
        fs::write(&large, vec![b'x'; 1_048_577]).expect("large fixture");
        assert!(
            HtmlFrontend::parse_file(&large, "client/pages/tac.html")
                .expect_err("size")
                .to_string()
                .contains("TY1101")
        );
    }
}
