use super::context_members::ContextMembers;
use super::contract_declaration::ContractDeclaration;
use super::contract_form::ContractForm;
use super::contract_member::ContractMember;
use super::contract_role::ContractRole;
use super::crud_members::CrudMembers;
use crate::annotation_rules::Messages;
use crate::behavior::HttpClientSource;
use std::ops::Range;
use std::path::Path;

pub(crate) struct ConcreteContract<'a> {
    form: ContractForm,
    source: &'a str,
    code: String,
    tac: bool,
}

type SourceEdit = (Range<usize>, String);

impl<'a> ConcreteContract<'a> {
    const TAC: &'static str = "tac";
    const GO: &'static str = "go";
    const RUST: &'static str = "rs";
    const PYTHON: &'static str = "py";
    const BROWSER: [&'static str; 2] = ["js", "ts"];
    const EMPTY_BODY: &'static str = " {}";
    const ASYNC_EMPTY_BODY: &'static str = " async {}";
    const DEFAULT_INDENT: &'static str = "    ";
    const LEADING_MODIFIERS: [&'static str; 4] = ["public", "abstract", "virtual", "async"];
    const TASKS: [&'static str; 2] = ["Task", "System.Threading.Tasks.Task"];
    const FUTURE: &'static str = "Future";
    const GO_FIELDS: [&'static str; 2] = ["struct{}", "struct {\n\tbehaviors *BehaviorContext\n}"];
    const HELD: [&'static str; 2] = ["streamrequest", "socketrequest"];
    const SOCKET: [&'static str; 1] = ["socketrequest"];
    const CSHARP: &'static str = "cs";
    const CSHARP_SEND: &'static str = "{indent}public System.Threading.Tasks.Task Send(object message) => BehaviorBrowserSocket.Send(\"{owner}\", message);\n";
    const GO_SEND: &'static str = "\nfunc ({receiver} *{owner}) Send(message any) error {\n\treturn behaviorSocketSend(\"{owner}\", message)\n}\n";
    const RUST_FIELDS: [&'static str; 2] = [";", " { behaviors: BehaviorContext }"];
    const TRAIT: &'static str = "trait";
    const RUST_DEFAULTS: [&'static str; 2] = [
        "",
        "impl Default for {name} {\n    fn default() -> Self {\n        Self::new()\n    }\n}\n",
    ];
    const HEADER_WORDS: [(&'static str, &'static str, &'static str); 7] = [
        ("java", "interface", "class"),
        ("groovy", "interface", "class"),
        ("kt", "interface", "class"),
        ("cs", "interface", "class"),
        ("php", "interface", "class"),
        ("dart", "abstract interface class", "class"),
        ("swift", "protocol", "final class"),
    ];
    const PROTOCOL_BASES: [&'static str; 6] = [
        "(Protocol)",
        "(typing.Protocol)",
        ", Protocol",
        ", typing.Protocol",
        "Protocol, ",
        "typing.Protocol, ",
    ];

    pub(crate) fn lower(path: &Path, source: &'a str, environment: &str) -> Option<String> {
        let extension = path.extension().and_then(|value| value.to_str())?;
        let form = ContractForm::of(extension)?;
        let tac = environment == Self::TAC;
        ContractRole::is_mentioned(source)
            .then(|| Self::new(form, source, tac))?
            .lowered()
    }

    fn new(form: ContractForm, source: &'a str, tac: bool) -> Self {
        let mask = crate::lexical::code_mask(form.language, source);
        Self {
            form,
            source,
            code: HttpClientSource::masked(source, &mask.code),
            tac,
        }
    }

    fn lowered(&self) -> Option<String> {
        let edits = self.contract_edits();
        (!edits.is_empty()).then(|| Self::apply(self.source, edits))
    }

    fn contract_edits(&self) -> Vec<SourceEdit> {
        ContractDeclaration::find(self.form, self.source, &self.code)
            .iter()
            .filter(|declaration| self.is_lowered(declaration))
            .flat_map(|declaration| self.declaration_edits(declaration))
            .collect()
    }

    fn is_lowered(&self, declaration: &ContractDeclaration) -> bool {
        let contract = self
            .form
            .declares_contract(&self.code[declaration.header.clone()]);
        let managed = [self.in_browser(), declaration.role.managed_in_browser] == [true, true];
        let lowered = declaration.role.is_lowered_in(self.form.language);
        [contract, managed, lowered] == [true, false, true]
    }

    fn is_provided(&self, declaration: &ContractDeclaration) -> bool {
        [self.in_browser(), declaration.role.names_entity] == [true, true]
    }

    fn in_browser(&self) -> bool {
        [self.tac, Self::BROWSER.contains(&self.form.language)] == [true, true]
    }

    fn apply(source: &str, mut edits: Vec<SourceEdit>) -> String {
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut output = source.to_owned();
        for (range, replacement) in edits {
            output.replace_range(range, &replacement);
        }
        output
    }

    fn declaration_edits(&self, declaration: &ContractDeclaration) -> Vec<SourceEdit> {
        let mut edits = self.shape_edits(declaration);
        edits.extend(self.provider_edit(declaration));
        edits
    }

    fn shape_edits(&self, declaration: &ContractDeclaration) -> Vec<SourceEdit> {
        if self.form.language == Self::GO {
            self.go_edits(declaration)
        } else {
            self.braced_edits(declaration)
        }
    }

    fn provider_edit(&self, declaration: &ContractDeclaration) -> Option<SourceEdit> {
        let at = self.code[..declaration.marker_at]
            .rfind('\n')
            .map_or(0, |line| line + 1);
        let marker = Messages::spelled(self.form.language, declaration.role.marker);
        let wanted = [declaration.role.names_entity, self.tac] == [true, false];
        wanted.then(|| (at..at, format!("{marker}\n")))
    }

    fn braced_edits(&self, declaration: &ContractDeclaration) -> Vec<SourceEdit> {
        let members = declaration.members(self.form, &self.code);
        let needs_context = declaration
            .role
            .needs_context(&self.exchanged_in_page(&members));
        let mut edits: Vec<SourceEdit> = self
            .header(declaration, needs_context)
            .into_iter()
            .map(|header| (declaration.header.clone(), header))
            .collect();
        edits.extend(
            members
                .iter()
                .filter(|member| Self::is_bodiless_method(member))
                .filter_map(|member| self.method_edit(member)),
        );
        edits.extend(self.context_edits(declaration, &members, needs_context));
        edits.extend(self.csharp_send(declaration, &members));
        edits
    }

    fn holds_in_page(&self) -> bool {
        [self.tac, self.form.language == Self::CSHARP] == [true, true]
    }

    fn exchanged_in_page(&self, members: &[ContractMember]) -> Vec<ContractMember> {
        let held: [&[&str]; 2] = [&[], &Self::HELD];
        let held = held[usize::from(self.holds_in_page())];
        members
            .iter()
            .filter(|member| !member.is_marked_with(held))
            .cloned()
            .collect()
    }

    fn csharp_send(
        &self,
        declaration: &ContractDeclaration,
        members: &[ContractMember],
    ) -> Option<SourceEdit> {
        let socket = members
            .iter()
            .any(|member| member.is_marked_with(&Self::SOCKET));
        let text = Self::CSHARP_SEND
            .replace("{indent}", &self.indent(members))
            .replace("{owner}", &declaration.name);
        [self.holds_in_page(), socket]
            .iter()
            .all(|wanted| *wanted)
            .then_some((declaration.close..declaration.close, text))
    }

    fn is_bodiless_method(member: &ContractMember) -> bool {
        [member.method, member.has_block()] == [true, false]
    }

    fn context_edits(
        &self,
        declaration: &ContractDeclaration,
        members: &[ContractMember],
        needs_context: bool,
    ) -> Vec<SourceEdit> {
        let wanted = [needs_context, self.form.language == Self::RUST].contains(&true);
        let indent = self.indent(members);
        let setup = declaration.setup();
        let context = ContextMembers {
            language: self.form.language,
            tac: self.tac,
            provided: self.is_provided(declaration),
            needs_context,
            owner: &declaration.name,
            receiver: declaration.role.noun,
            setup: &setup,
            partition: &declaration.adapter(),
            indent: &indent,
        };
        let (field, text) = context.parts_for(declaration.role.marker);
        let crud = self.crud_members(declaration, &indent);
        let open = declaration.header.end + 1;
        let field_edit = (!field.is_empty()).then_some((open..open, field));
        let member_edit = Some((declaration.close..declaration.close, text + &crud));
        [field_edit, member_edit]
            .into_iter()
            .flatten()
            .filter(|_| wanted)
            .collect()
    }

    fn crud_members(&self, declaration: &ContractDeclaration, indent: &str) -> String {
        declaration
            .repository_base(self.form, &self.code)
            .map_or_else(String::new, |base| {
                let entity = [declaration.entity.as_str(), base.entity.as_str()]
                    [usize::from(declaration.entity.is_empty())];
                CrudMembers {
                    language: self.form.language,
                    base: &base,
                    entity,
                    owner: &declaration.name,
                    receiver: declaration.role.noun,
                    indent,
                }
                .text()
            })
    }

    fn indent(&self, members: &[ContractMember]) -> String {
        members.first().map_or_else(
            || String::from(Self::DEFAULT_INDENT),
            |first| self.indent_before(first.start),
        )
    }

    fn indent_before(&self, at: usize) -> String {
        let line_start = self.code[..at].rfind('\n').map_or(0, |line| line + 1);
        let prefix = &self.source[line_start..at.max(line_start)];
        let width = prefix.len() - prefix.trim_start().len();
        prefix[..width].to_owned()
    }

    fn header(&self, declaration: &ContractDeclaration, needs_context: bool) -> Option<String> {
        let base = declaration.repository_base(self.form, &self.code);
        let written = &self.source[declaration.header.clone()];
        let text = base.as_ref().map_or_else(
            || written.to_owned(),
            |base| base.without_clause(written, declaration.header.start),
        );
        let replaced = match self.form.language {
            Self::PYTHON => Some(Self::without_protocol(&text)),
            Self::RUST => Some(self.rust_header(&text, &declaration.name, needs_context)),
            _ => self.replaced_header(&text),
        };
        replaced.or_else(|| base.map(|_| text))
    }

    fn replaced_header(&self, text: &str) -> Option<String> {
        Self::HEADER_WORDS
            .iter()
            .find(|(language, _, _)| *language == self.form.language)
            .map(|(_, word, replacement)| Self::replace_word(text, word, replacement))
    }

    fn replace_word(text: &str, word: &str, replacement: &str) -> String {
        let padded = format!(" {text}");
        padded.replacen(&format!(" {word} "), &format!(" {replacement} "), 1)[1..].to_owned()
    }

    fn rust_header(&self, text: &str, name: &str, needs_context: bool) -> String {
        let fields = Self::RUST_FIELDS[usize::from(needs_context)];
        let visibility = text.split(Self::TRAIT).next().unwrap_or("");
        let default = Self::RUST_DEFAULTS[usize::from(self.tac)].replace("{name}", name);
        format!("{visibility}struct {name}{fields}\n{default}impl {name} ")
    }

    fn without_protocol(text: &str) -> String {
        Self::PROTOCOL_BASES
            .iter()
            .fold(text.to_owned(), |header, base| header.replacen(base, "", 1))
    }

    fn method_edit(&self, member: &ContractMember) -> Option<SourceEdit> {
        let written = [self.form.has_contract(), self.form.is_indented()] == [true, false];
        let signature = &self.source[member.start..member.end];
        let end = member
            .terminator
            .map_or(member.end, |terminator| terminator + 1);
        written.then(|| (member.start..end, self.concrete_signature(signature)))
    }

    fn concrete_signature(&self, signature: &str) -> String {
        match self.form.language {
            "cs" => Self::csharp_signature(signature),
            "dart" => Self::dart_signature(signature),
            "java" | "groovy" | "kt" => {
                format!("{}{}", Self::without_modifiers(signature), Self::EMPTY_BODY)
            }
            _ => format!("{signature}{}", Self::EMPTY_BODY),
        }
    }

    fn csharp_signature(signature: &str) -> String {
        let bare = Self::without_modifiers(signature);
        let returns_task = Self::TASKS.iter().any(|task| bare.starts_with(task));
        let asynchronous = ["", "async "][usize::from(returns_task)];
        format!("public {asynchronous}{bare}{}", Self::EMPTY_BODY)
    }

    fn dart_signature(signature: &str) -> String {
        let future = signature.trim_start().starts_with(Self::FUTURE);
        let body = [Self::EMPTY_BODY, Self::ASYNC_EMPTY_BODY][usize::from(future)];
        format!("{signature}{body}")
    }

    fn without_modifiers(signature: &str) -> String {
        let words: Vec<&str> = signature
            .split_whitespace()
            .skip_while(|word| Self::LEADING_MODIFIERS.contains(word))
            .collect();
        words.join(" ")
    }

    fn go_edits(&self, declaration: &ContractDeclaration) -> Vec<SourceEdit> {
        let members = declaration.members(self.form, &self.code);
        let exchanged: Vec<ContractMember> = members
            .iter()
            .filter(|member| !member.is_marked_with(&Self::HELD))
            .cloned()
            .collect();
        let needs_context = declaration.role.needs_context(&exchanged);
        let end = (declaration.close + 1).min(self.source.len());
        let text = self.go_type(declaration, &members, needs_context);
        vec![(declaration.header.start..end, text)]
    }

    fn go_type(
        &self,
        declaration: &ContractDeclaration,
        members: &[ContractMember],
        needs_context: bool,
    ) -> String {
        let name = &declaration.name;
        let fields = Self::GO_FIELDS[usize::from(needs_context)];
        let methods: String = members
            .iter()
            .filter(|member| member.method)
            .map(|member| self.go_method(declaration, member))
            .collect();
        let context = self.go_context(declaration, needs_context);
        let crud = self.crud_members(declaration, "");
        let send = self.go_send(declaration, members);
        format!("type {name} {fields}\n{methods}{send}{context}{crud}")
    }

    fn go_send(&self, declaration: &ContractDeclaration, members: &[ContractMember]) -> String {
        let socket = members
            .iter()
            .any(|member| member.is_marked_with(&Self::SOCKET));
        if [self.tac, socket] != [true, true] {
            return String::new();
        }
        Self::GO_SEND
            .replace("{receiver}", declaration.role.noun)
            .replace("{owner}", &declaration.name)
    }

    fn go_method(&self, declaration: &ContractDeclaration, member: &ContractMember) -> String {
        let markers = member.first_marker.map_or_else(String::new, |first| {
            Self::marker_lines(&self.source[first..member.start])
        });
        let signature = self.source[member.start..member.end].trim();
        format!(
            "\n{markers}func ({} *{}) {signature} {{\n}}\n",
            declaration.role.noun, declaration.name
        )
    }

    fn marker_lines(text: &str) -> String {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .flat_map(|line| [line, "\n"])
            .collect()
    }

    fn go_context(&self, declaration: &ContractDeclaration, needs_context: bool) -> String {
        let setup = declaration.setup();
        let context = ContextMembers {
            language: self.form.language,
            tac: self.tac,
            provided: self.is_provided(declaration),
            needs_context,
            owner: &declaration.name,
            receiver: declaration.role.noun,
            setup: &setup,
            partition: &declaration.adapter(),
            indent: "",
        };
        context.parts_for(declaration.role.marker).1
    }
}
