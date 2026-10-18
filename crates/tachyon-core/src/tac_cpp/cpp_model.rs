use super::{
    CppChecks, CppFailure, CppFile, CppFolder, CppLowering, CppMarkerCheck, CppMemberKind,
    CppSettings, CppStore,
};
use crate::Failure;
use serde_json::Value;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) struct CppModel {
    pub(crate) file: CppFile,
    pub(crate) path: PathBuf,
    pub(crate) entity: Option<Value>,
    plans: String,
    source: String,
}

impl CppModel {
    const NAMED: &'static str = "A C++ model is named after its file";
    const CLIENT: &'static str = "client";
    const MODELS: &'static str = "client/models";
    const TOPIC: &'static str = "Topic";
    const TOPIC_FORM: &'static str = "[[Topic]] goes on a C++ struct or class model with fields, for example `struct [[Topic]] OrderPlaced { int total = 0; };`";
    const DESCRIBED_ALONE: &'static str =
        "[[Description]] describes a model that has another Tachyon marker, such as [[Checked]]";
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
        (!CppSettings::is_settings(&file.class))
            .then(|| CppChecks::check_model(relative, &file.class))
            .transpose()
            .map(drop)?;
        Self::check_topic(relative, source, &file)?;
        CppMarkerCheck::described_alone(&file.class).map_or(Ok(()), |at| {
            Err(CppFailure::unreadable(relative, at, Self::DESCRIBED_ALONE))
        })?;
        let entity = CppStore::entity(relative, source, &file.class)?;
        let plans = CppSettings::plans(relative, source, &file.class)?;
        Ok(Self {
            file,
            path: relative.to_path_buf(),
            entity,
            plans,
            source: source.to_owned(),
        })
    }

    fn check_topic(relative: &Path, source: &str, file: &CppFile) -> Result<(), Failure> {
        let record = [file.class.is_enum, Self::abstract_body(source, file)] == [false, false];
        file.class
            .markers
            .iter()
            .find(|marker| marker.name == Self::TOPIC)
            .map_or(Ok(()), |marker| {
                record
                    .then_some(())
                    .ok_or_else(|| CppFailure::unreadable(relative, marker.at, Self::TOPIC_FORM))
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
        format!(
            "{}{}{}",
            self.codec_text(),
            CppChecks::specialization(&self.file.class),
            self.plans
        )
    }

    pub(crate) fn is_checked(&self) -> bool {
        CppChecks::is_checked(&self.file.class)
    }

    pub(crate) fn fields(&self) -> Vec<(String, String)> {
        self.file
            .class
            .members
            .iter()
            .filter(|member| member.is_public_instance(CppMemberKind::Field))
            .map(|member| (member.name.clone(), member.type_text.clone()))
            .collect()
    }

    fn codec_text(&self) -> String {
        let writers = [Self::struct_codec, Self::enum_codec];
        let is_enum = self.file.class.is_enum;
        let abstract_model = [is_enum, self.is_abstract()] == [false, true];
        std::iter::once(self)
            .filter(|_| !abstract_model)
            .map(writers[usize::from(is_enum)])
            .collect()
    }

    pub(crate) fn is_enum(&self) -> bool {
        self.file.class.is_enum
    }

    pub(crate) fn topics(root: &Path) -> Result<Vec<String>, Failure> {
        let models = CppFolder::files(root, Self::MODELS)?
            .iter()
            .map(|path| Self::read_in(root, path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(models
            .iter()
            .filter(|model| model.is_topic())
            .map(|model| model.name().to_owned())
            .collect())
    }

    pub(crate) fn read_in(root: &Path, path: &Path) -> Result<Self, Failure> {
        let (relative, source) = CppFolder::read(root, path)?;
        Self::read(&relative, &source)
    }

    pub(crate) fn is_topic(&self) -> bool {
        self.file
            .class
            .markers
            .iter()
            .any(|marker| marker.name == Self::TOPIC)
    }

    pub(crate) fn is_abstract(&self) -> bool {
        Self::abstract_body(&self.source, &self.file)
    }

    fn abstract_body(source: &str, file: &CppFile) -> bool {
        let body = &source[file.class.body.clone()];
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
