use super::{
    CppChecks, CppClass, CppFailure, CppFile, CppInjection, CppLifetime, CppListener,
    CppMarkerCheck, CppMember, CppMemberKind, CppRemote,
};
use crate::Failure;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) struct CppService {
    pub(crate) file: CppFile,
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) listeners: Vec<CppListener>,
}

impl CppService {
    const PUBLISHER: &'static str = "ApplicationEventPublisher";
    const LIFECYCLE: [&'static str; 3] = ["OnEvent", "OnStart", "OnStop"];
    const NEEDS_FORM: &'static str = "A C++ [[Needs]] field is a reference with no initial value, for example `[[Needs]] OrderStore& orders;`";
    const NESTED: &'static str =
        "A C++ service declares no nested types; put a model in client/models/<Name>.hpp";
    const ONE_FIELD: &'static str = "Declare one C++ field per line";
    const OVERLOADED: &'static str = "A C++ service method that the page calls has one name; C++ overloads cannot reach the page. Rename one of the methods";
    const NAMED: &'static str = "A C++ class is named after its file";
    const STORED: &'static str = "A C++ [[LocalState]] or [[SessionState]] field is a page field that the browser keeps: public, not static, not const and not a reference, for example `[[LocalState]] int count = 0;`";

    pub(crate) fn read(relative: &Path, source: &str) -> Result<Self, Failure> {
        let file = CppFile::read(relative, source)?;
        let class = &file.class;
        let named = relative
            .file_stem()
            .is_some_and(|stem| stem == class.name.as_str());
        named
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(relative, class.name_at, Self::NAMED))?;
        CppMarkerCheck::check(relative, class)?;
        CppLifetime::check(relative, class)?;
        CppChecks::check_service(relative, class)?;
        Self::check_members(relative, class)?;
        Self::check_remote(relative, class)?;
        let listeners = class
            .members
            .iter()
            .filter(|member| member.marked("OnEvent"))
            .map(|member| CppListener::read(relative, member))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            file,
            path: relative.to_path_buf(),
            source: source.to_owned(),
            listeners,
        })
    }

    pub(crate) fn class(&self) -> &CppClass {
        &self.file.class
    }

    pub(crate) fn is_remote(&self) -> bool {
        Self::remote_class(self.class())
    }

    pub(crate) fn field_at(&self, field: &str) -> usize {
        self.class()
            .members
            .iter()
            .find(|member| member.name == field)
            .map_or(0, |member| member.name_at)
    }

    pub(crate) fn needs(&self) -> Vec<(String, String)> {
        self.injections()
            .into_iter()
            .map(|injection| (injection.field, injection.provider))
            .collect()
    }

    pub(crate) fn injections(&self) -> Vec<CppInjection> {
        CppLifetime::injections(self.class())
    }

    pub(crate) fn is_prototype(&self) -> bool {
        CppLifetime::is_prototype(self.class())
    }

    pub(crate) fn page_fields(&self) -> Vec<&CppMember> {
        self.class()
            .members
            .iter()
            .filter(|member| member.is_public_instance(CppMemberKind::Field))
            .filter(|member| Self::is_page_field(member))
            .collect()
    }

    pub(crate) fn page_methods(&self) -> Vec<&CppMember> {
        Self::page_methods_of(self.class())
    }

    pub(crate) fn binding_name(&self) -> String {
        format!("TachyonBinding{}", self.class().name)
    }

    pub(crate) fn metadata(&self) -> Value {
        let fields: Vec<String> = self
            .page_fields()
            .iter()
            .map(|member| Self::page_name(&member.name))
            .collect();
        let methods: Vec<String> = self
            .page_methods()
            .iter()
            .map(|member| Self::page_name(&member.name))
            .collect();
        let asynchronous: Vec<String> = self
            .page_methods()
            .iter()
            .filter(|member| member.is_task())
            .map(|member| Self::page_name(&member.name))
            .collect();
        json!({
            "page": {
                "fields": fields,
                "methods": methods,
                "asyncMethods": asynchronous,
                "listeners": self.listeners.iter().map(CppListener::page).collect::<Vec<_>>(),
            },
            "starts": self.hooks("OnStart"),
            "stops": self.hooks("OnStop"),
            "listeners": self.listeners.iter().map(CppListener::program).collect::<Vec<_>>(),
            "description": CppMarkerCheck::description(self.class()),
        })
    }

    pub(crate) fn binding(&self) -> String {
        self.binding_text()
    }

    fn binding_text(&self) -> String {
        if self.is_remote() {
            return String::new();
        }
        self.binding_class()
    }

    fn binding_class(&self) -> String {
        let binding = self.binding_name();
        let metadata = self.metadata();
        let fields = self.field_bindings();
        let props = self.prop_bindings();
        let methods = self.method_bindings();
        let hooks = self.hook_bindings();
        let create = self.creation();
        format!(
            "class {binding} {{\npublic:\n    static tachyon::TacServiceType type() {{\n        tachyon::TacServiceType type;\n        type.metadata = tachyon::json::JsonNode::parse(R\"tachyon({metadata})tachyon\");\n        type.create = [] {{ {create} }};\n        type.fields = {{ {fields}}};\n        type.props = {{ {props}}};\n        type.methods = {{ {methods}}};\n        type.hooks = {{ {hooks}}};\n        return type;\n    }}\n}};\n"
        )
    }

    fn creation(&self) -> String {
        let class = &self.class().name;
        if self.is_prototype() {
            return format!("return std::shared_ptr<void>(std::make_shared<{class}>());");
        }
        format!("return std::shared_ptr<void>(&tachyon::TacNeeds::of<{class}>(), [](void*) {{}});")
    }

    fn prop_bindings(&self) -> String {
        let class = &self.class().name;
        let mut text = String::new();
        for member in CppLifetime::props(self.class()) {
            let page = Self::page_name(&member.name);
            let _ = write!(
                text,
                "tachyon::TacBind::field(\"{page}\", &{class}::{}), ",
                member.name
            );
        }
        text
    }

    fn check_remote(relative: &Path, class: &CppClass) -> Result<(), Failure> {
        if !Self::remote_class(class) {
            return Ok(());
        }
        CppRemote::check(relative, class)
    }

    fn remote_class(class: &CppClass) -> bool {
        class
            .markers
            .iter()
            .any(|marker| marker.name == "HTTPClient")
    }

    fn check_members(relative: &Path, class: &CppClass) -> Result<(), Failure> {
        for member in &class.members {
            Self::check_member(relative, member)?;
        }
        Self::check_overloads(relative, class)
    }

    fn check_member(relative: &Path, member: &CppMember) -> Result<(), Failure> {
        Self::member_problem(member).map_or(Ok(()), |message| {
            Err(CppFailure::unreadable(relative, member.name_at, message))
        })
    }

    fn member_problem(member: &CppMember) -> Option<&'static str> {
        let nested = member.kind == CppMemberKind::NestedType;
        let wrong_reference = [
            !member.type_text.ends_with('&'),
            member.has_initializer,
            member.declarators > 1,
        ]
        .contains(&true);
        let wrong_needs = [member.marked("Needs"), wrong_reference] == [true, true];
        let crowded = [member.kind == CppMemberKind::Field, member.declarators > 1] == [true, true];
        let stored = [member.marked("LocalState"), member.marked("SessionState")].contains(&true);
        let page_field = [
            member.is_public_instance(CppMemberKind::Field),
            !member.is_const,
            Self::is_page_field(member),
        ] == [true, true, true];
        [
            (nested, Self::NESTED),
            ([stored, page_field] == [true, false], Self::STORED),
            (wrong_needs, Self::NEEDS_FORM),
            (crowded, Self::ONE_FIELD),
        ]
        .into_iter()
        .find(|(found, _)| *found)
        .map(|(_, message)| message)
    }

    fn check_overloads(relative: &Path, class: &CppClass) -> Result<(), Failure> {
        let methods: Vec<&str> = Self::page_methods_of(class)
            .into_iter()
            .map(|member| member.name.as_str())
            .collect();
        let repeated = methods
            .iter()
            .enumerate()
            .find(|(index, name)| methods[..*index].contains(name))
            .map(|(_, name)| *name);
        repeated.map_or(Ok(()), |name| {
            let at = class
                .members
                .iter()
                .rev()
                .find(|member| member.name == name)
                .map_or(0, |member| member.name_at);
            Err(CppFailure::unreadable(relative, at, Self::OVERLOADED))
        })
    }

    fn is_page_field(member: &CppMember) -> bool {
        let publisher = member.plain_type() == Self::PUBLISHER;
        [member.marked("Needs"), member.is_reference(), publisher] == [false, false, false]
    }

    fn page_methods_of(class: &CppClass) -> Vec<&CppMember> {
        class
            .members
            .iter()
            .filter(|member| member.is_public_instance(CppMemberKind::Method))
            .filter(|member| !Self::LIFECYCLE.iter().any(|name| member.marked(name)))
            .collect()
    }

    fn hooks(&self, marker: &str) -> Vec<String> {
        self.class()
            .members
            .iter()
            .filter(|member| {
                [member.kind == CppMemberKind::Method, member.marked(marker)] == [true, true]
            })
            .map(|member| member.name.clone())
            .collect()
    }

    fn page_name(name: &str) -> String {
        crate::companion::camel_case(name)
    }

    fn field_bindings(&self) -> String {
        let class = &self.class().name;
        let mut text = String::new();
        for member in self.page_fields() {
            let page = Self::page_name(&member.name);
            let _ = write!(
                text,
                "tachyon::TacBind::field(\"{page}\", &{class}::{}), ",
                member.name
            );
        }
        text
    }

    fn method_bindings(&self) -> String {
        let class = &self.class().name;
        let mut text = String::new();
        for member in self.page_methods() {
            let page = Self::page_name(&member.name);
            let _ = write!(
                text,
                "tachyon::TacBind::method(\"{page}\", &{class}::{}), ",
                member.name
            );
        }
        text
    }

    fn hook_bindings(&self) -> String {
        let class = &self.class().name;
        let listened = self
            .listeners
            .iter()
            .map(|listener| listener.method.clone());
        let hooked = listened
            .chain(self.hooks("OnStart"))
            .chain(self.hooks("OnStop"));
        let mut text = String::new();
        for name in hooked {
            let _ = write!(
                text,
                "tachyon::TacBind::method(\"{name}\", &{class}::{name}), "
            );
        }
        text
    }
}
