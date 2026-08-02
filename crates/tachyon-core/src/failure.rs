use std::fmt;
use tachyon_diagnostics::{Diagnostic, DiagnosticCode, DiagnosticReport, Severity, SourceSpan};

/// A failed Tachyon operation with stable structured diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Failure {
    diagnostics: Vec<Diagnostic>,
}

impl Failure {
    /// Creates a failure from one diagnostic.
    #[must_use]
    pub fn one(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    /// Creates a failure from a non-empty diagnostic collection.
    #[must_use]
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        debug_assert!(!diagnostics.is_empty());
        Self { diagnostics }
    }

    /// Returns the diagnostics in deterministic presentation order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns a serializable Diagnostics v1 report.
    #[must_use]
    pub fn report(&self) -> DiagnosticReport {
        DiagnosticReport::v1(self.diagnostics.clone())
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            writeln!(
                formatter,
                "{}[{}]: {}",
                severity_label(diagnostic.severity),
                diagnostic.code,
                diagnostic.message
            )?;
            for span in &diagnostic.spans {
                writeln!(
                    formatter,
                    "  --> {}:{}..{}",
                    span.file, span.start, span.end
                )?;
            }
            if let Some(help) = &diagnostic.help {
                writeln!(formatter, "  help: {help}")?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for Failure {}

pub(crate) fn diagnostic(
    number: u16,
    message: impl Into<String>,
    help: Option<String>,
    span: Option<SourceSpan>,
) -> Diagnostic {
    let code = DiagnosticCode::from_number(number).unwrap_or_else(|| unreachable!());
    Diagnostic {
        code,
        severity: Severity::Error,
        message: message.into(),
        help,
        spans: span.into_iter().collect(),
    }
}

pub(crate) fn source_span(file: &str, start: usize, end: usize) -> Option<SourceSpan> {
    SourceSpan::new(file, start as u64, end as u64).ok()
}

const fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Information => "information",
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, diagnostic};
    use tachyon_diagnostics::{Diagnostic, Severity, SourceSpan};

    #[test]
    fn failures_have_human_and_machine_presentations() {
        let failure = Failure::one(diagnostic(
            1002,
            "No view source was found.",
            Some(String::from("Add client/pages/tac.html.")),
            None,
        ));
        assert!(failure.to_string().contains("error[TY1002]"));
        assert_eq!(failure.report().contract_version, 1);
        assert_eq!(failure.diagnostics().len(), 1);
    }

    #[test]
    fn multiple_diagnostics_render_spans_and_separation() {
        let failure = Failure::new(vec![
            Diagnostic {
                code: "TY1001".parse().unwrap_or_else(|_| unreachable!()),
                severity: Severity::Warning,
                message: String::from("First"),
                help: None,
                spans: vec![
                    SourceSpan::new("client/pages/tac.html", 1, 4)
                        .unwrap_or_else(|_| unreachable!()),
                ],
            },
            Diagnostic {
                code: "TY1002".parse().unwrap_or_else(|_| unreachable!()),
                severity: Severity::Information,
                message: String::from("Second"),
                help: Some(String::from("Continue.")),
                spans: Vec::new(),
            },
        ]);
        let rendered = failure.to_string();
        assert!(rendered.contains("warning[TY1001]"));
        assert!(rendered.contains("client/pages/tac.html:1..4"));
        assert!(rendered.contains("\n\ninformation[TY1002]"));
    }
}
