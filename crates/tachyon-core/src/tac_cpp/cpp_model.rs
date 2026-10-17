use super::{CppFailure, CppFile, CppLowering, CppMarkerCheck, CppMemberKind};
use crate::Failure;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) struct CppModel {
    pub(crate) file: CppFile,
    pub(crate) path: PathBuf,
    source: String,
}

impl CppModel {
    const NAMED: &'static str = "A C++ model is named after its file";
    const CLIENT: &'static str = "client";
    const PURE_ENDS: [&'static str; 4] = [")=0;", ")const=0;", ")noexcept=0;", ")constnoexcept=0;"];
    const CLIENT_ONLY: &'static str = "C++ is a Tac language only for now: a C++ model goes in client/models. Yon C++ comes later (ADR 0099)";

    pub(crate) fn read(relative: &Path, source: &str) -> Result<Self, Failure> {
        let file = CppFile::read(relative, source)?;
        let named = relative
            .file_stem()
            .is_some_and(|stem| stem == file.class.name.as_str());
        named
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(relative, file.class.name_at, Self::NAMED))?;
        relative
            .starts_with(Self::CLIENT)
            .then_some(())
            .ok_or_else(|| CppFailure::pending(relative, 0, Self::CLIENT_ONLY))?;
        CppMarkerCheck::check(relative, &file.class)?;
        Ok(Self {
            file,
            path: relative.to_path_buf(),
            source: source.to_owned(),
        })
    }

    pub(crate) fn check(relative: &Path, source: &str) -> Result<(), Failure> {
        Self::read(relative, source).map(drop)
    }

    pub(crate) fn name(&self) -> &str {
        &self.file.class.name
    }

    pub(crate) fn lowered(&self) -> String {
        CppLowering::model(&self.file, &self.source)
    }

    pub(crate) fn field_types(&self) -> Vec<String> {
        self.fields()
            .into_iter()
            .map(|(_, type_text)| type_text)
            .collect()
    }

    pub(crate) fn codec(&self) -> String {
        self.codec_text()
    }

    fn fields(&self) -> Vec<(String, String)> {
        self.file
            .class
            .members
            .iter()
            .filter(|member| member.is_public_instance(CppMemberKind::Field))
            .map(|member| (member.name.clone(), member.type_text.clone()))
            .collect()
    }

    fn codec_text(&self) -> String {
        if self.file.class.is_enum {
            return self.enum_codec();
        }
        if self.is_abstract() {
            return String::new();
        }
        self.struct_codec()
    }

    fn is_abstract(&self) -> bool {
        let body = &self.source[self.file.class.body.clone()];
        let squeezed: String = body
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        Self::PURE_ENDS.iter().any(|end| squeezed.contains(end))
    }

    fn struct_codec(&self) -> String {
        let name = self.name();
        let mut writes = String::new();
        let mut reads = String::new();
        for (field, _) in self.fields() {
            let _ = writeln!(
                writes,
                "        JsonModelFields::write(object, \"{field}\", value.{field});"
            );
            let _ = writeln!(
                reads,
                "        JsonModelFields::read(node, \"{field}\", value.{field});"
            );
        }
        format!(
            "namespace tachyon::json {{\ntemplate <>\nstruct JsonCodec<::{name}> {{\n    static constexpr std::string_view NAME = \"{name}\";\n    static JsonNode toJson(const ::{name}& value) {{\n        JsonObject object;\n{writes}        return object;\n    }}\n    static ::{name} fromJson(const JsonNode& node) {{\n        JsonModelFields::requireObject(node, NAME);\n        ::{name} value{{}};\n{reads}        return value;\n    }}\n}};\n}}\n"
        )
    }

    fn enum_codec(&self) -> String {
        let name = self.name();
        let mut writes = String::new();
        let mut reads = String::new();
        for item in &self.file.class.enumerators {
            let _ = writeln!(
                writes,
                "        if (value == ::{name}::{item}) {{\n            return JsonNode(\"{item}\");\n        }}"
            );
            let _ = writeln!(
                reads,
                "        if (written == \"{item}\") {{\n            return ::{name}::{item};\n        }}"
            );
        }
        format!(
            "namespace tachyon::json {{\ntemplate <>\nstruct JsonCodec<::{name}> {{\n    static constexpr std::string_view NAME = \"{name}\";\n    static JsonNode toJson(const ::{name}& value) {{\n{writes}        JsonModelFields::unknownName(NAME);\n    }}\n    static ::{name} fromJson(const JsonNode& node) {{\n        std::string written = JsonModelFields::enumName(node, NAME);\n{reads}        JsonModelFields::unknownName(NAME);\n    }}\n}};\n}}\n"
        )
    }
}
