use super::super::testing::Unit;
use super::Type;

pub(super) struct TypedFakeReset {
    language: &'static str,
    untyped: &'static [&'static str],
    empty_values: &'static [(&'static str, &'static str)],
    indent: &'static str,
    end: &'static str,
}

impl TypedFakeReset {
    const NULLABLE: char = '?';
    const NULL: &'static str = "null";

    pub(super) const KOTLIN: Self = Self {
        language: "Kotlin",
        untyped: &[""],
        empty_values: &[
            ("Boolean", "false"),
            ("Byte", "0"),
            ("Short", "0"),
            ("Int", "0"),
            ("Long", "0L"),
            ("Float", "0.0f"),
            ("Double", "0.0"),
            ("Char", "'\\u0000'"),
        ],
        indent: "",
        end: "",
    };

    pub(super) fn statements(&self, owner: &Type, unit: &Unit) -> Result<String, String> {
        unit.mocks
            .iter()
            .map(|name| self.statement(owner, name))
            .collect()
    }

    fn statement(&self, owner: &Type, name: &str) -> Result<String, String> {
        let kind = Self::declared_kind(owner, name);
        let empty = self
            .empty_value(kind)
            .ok_or_else(|| self.refusal(name, kind))?;
        Ok(format!(
            "{}this.{name} = {empty}{}\n",
            self.indent, self.end
        ))
    }

    fn empty_value(&self, kind: &str) -> Option<&'static str> {
        let nullable = [kind.ends_with(Self::NULLABLE), self.untyped.contains(&kind)];
        if nullable.contains(&true) {
            return Some(Self::NULL);
        }
        self.empty_values
            .iter()
            .find(|(declared, _)| *declared == kind)
            .map(|(_, empty)| *empty)
    }

    fn refusal(&self, name: &str, kind: &str) -> String {
        let types: Vec<&str> = self
            .empty_values
            .iter()
            .map(|(declared, _)| *declared)
            .collect();
        format!(
            "{} Fake {name} has the type {kind}, so the test driver cannot clear it before each case. Give it the nullable type {kind}? or one of the types {}.",
            self.language,
            types.join(", ")
        )
    }

    fn declared_kind<'b>(owner: &'b Type, name: &str) -> &'b str {
        let declared = owner.fields.iter().find(|field| field.name == name);
        declared.map_or("", |field| field.type_source.trim())
    }
}
