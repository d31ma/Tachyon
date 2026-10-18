use super::{
    CppAttribute, CppAttributes, CppDeclaration, CppMember, CppMemberEnd, CppMemberKind,
    CppParameter, CppShape, CppStep, CppTokens,
};
use std::ops::Range;

pub(crate) struct CppMemberReader<'a> {
    tokens: &'a CppTokens,
    source: &'a str,
    class: &'a str,
    public: bool,
}

impl<'a> CppMemberReader<'a> {
    const SPECIFIERS: [&'static str; 9] = [
        "static",
        "virtual",
        "inline",
        "constexpr",
        "explicit",
        "mutable",
        "consteval",
        "constinit",
        "thread_local",
    ];
    const RETURN_ENDS: [&'static str; 4] = ["{", "=", "override", "final"];

    pub(crate) fn read(
        tokens: &'a CppTokens,
        source: &'a str,
        class: &'a str,
        public: bool,
        body: Range<usize>,
    ) -> Vec<CppMember> {
        let mut reader = Self {
            tokens,
            source,
            class,
            public,
        };
        reader.members(body)
    }

    fn members(&mut self, body: Range<usize>) -> Vec<CppMember> {
        let mut members = Vec::new();
        let mut at = body.start;
        while at < body.end {
            at = self.next_member(at, body.end, &mut members);
        }
        members
    }

    fn next_member(&mut self, at: usize, limit: usize, members: &mut Vec<CppMember>) -> usize {
        match CppStep::at(self.tokens, at) {
            CppStep::Separator => at + 1,
            CppStep::Access(public) => self.access(public, at),
            CppStep::Member => self.push_member(at, limit, members),
        }
    }

    fn access(&mut self, public: bool, at: usize) -> usize {
        self.public = public;
        at + 2
    }

    fn push_member(&self, at: usize, limit: usize, members: &mut Vec<CppMember>) -> usize {
        let (markers, start) = CppAttributes::read(self.tokens, self.source, at);
        let (end, declared) = CppMemberEnd::find(self.tokens, start, limit);
        members.push(self.member(markers, start..declared));
        end
    }

    fn member(&self, markers: Vec<CppAttribute>, declaration: Range<usize>) -> CppMember {
        let shape = CppShape::of(self.tokens, &declaration);
        let specifiers = self.specifiers(&declaration);
        let mut member = self.empty_member(markers, &declaration, &shape, specifiers);
        match CppDeclaration::of(self.tokens, &declaration, specifiers, &shape) {
            CppDeclaration::Skipped => {}
            CppDeclaration::Nested => self.nested(&mut member, specifiers),
            CppDeclaration::Operator => {
                member.kind = CppMemberKind::Operator;
            }
            CppDeclaration::Function(open) => {
                self.function(&mut member, specifiers, open, declaration.end);
            }
            CppDeclaration::Field(cut) => self.field(&mut member, specifiers, cut),
        }
        member
    }

    fn empty_member(
        &self,
        markers: Vec<CppAttribute>,
        declaration: &Range<usize>,
        shape: &CppShape,
        specifiers: usize,
    ) -> CppMember {
        CppMember {
            kind: CppMemberKind::Other,
            name: String::new(),
            name_at: self.tokens.start(declaration.start).min(self.source.len()),
            type_text: String::new(),
            parameters: Vec::new(),
            public: self.public,
            is_static: (declaration.start..specifiers).any(|index| self.tokens.is(index, "static")),
            has_initializer: shape.initialized,
            declarators: shape.commas.len() + 1,
            markers,
            terminator: self.tokens.start(declaration.end).min(self.source.len()),
            body_end: self.body_end(declaration.end),
            is_const: false,
        }
    }

    fn body_end(&self, end: usize) -> usize {
        let close = self
            .tokens
            .is(end, "{")
            .then(|| self.tokens.partner(end))
            .flatten()
            .unwrap_or(end);
        self.tokens.start(close).min(self.source.len())
    }

    fn specifiers(&self, declaration: &Range<usize>) -> usize {
        (declaration.start..declaration.end)
            .find(|index| !Self::SPECIFIERS.contains(&self.tokens.text(*index)))
            .unwrap_or(declaration.end)
    }

    fn text_between(&self, first: usize, after: usize) -> String {
        let start = self.tokens.start(first).min(self.source.len());
        let end = self
            .tokens
            .end(after.saturating_sub(1))
            .min(self.source.len())
            .max(start);
        let written = [&self.source[start..end], ""][usize::from(first >= after)];
        written.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn nested(&self, member: &mut CppMember, specifiers: usize) {
        member.kind = CppMemberKind::NestedType;
        self.tokens
            .text(specifiers + 1)
            .clone_into(&mut member.name);
    }

    fn function(&self, member: &mut CppMember, specifiers: usize, open: usize, end: usize) {
        let name_index = open.saturating_sub(1);
        self.tokens.text(name_index).clone_into(&mut member.name);
        member.name_at = self.tokens.start(name_index);
        let destructor = self.tokens.is(name_index.saturating_sub(1), "~");
        member.kind = CppDeclaration::function_kind(destructor, member.name == self.class);
        let close = self.tokens.partner(open).unwrap_or(end);
        let written = self.text_between(specifiers, name_index);
        member.type_text = self.trailing_return(close, end).unwrap_or(written);
        member.parameters = self.parameters(open + 1, close);
        member.is_const = (close..end).any(|index| self.tokens.is(index, "const"));
    }

    fn trailing_return(&self, close: usize, end: usize) -> Option<String> {
        let arrow = (close..end).find(|index| self.tokens.is(*index, "->"))?;
        let stop = (arrow..end)
            .find(|index| Self::RETURN_ENDS.contains(&self.tokens.text(*index)))
            .unwrap_or(end);
        Some(self.text_between(arrow + 1, stop))
    }

    fn field(&self, member: &mut CppMember, specifiers: usize, cut: usize) {
        let name_index = (specifiers..cut)
            .rev()
            .find(|index| self.tokens.is_word(*index));
        self.fill_field(member, specifiers, name_index);
    }

    fn fill_field(&self, member: &mut CppMember, specifiers: usize, name_index: Option<usize>) {
        let Some(name_index) = name_index else {
            return;
        };
        member.kind = CppMemberKind::Field;
        self.tokens.text(name_index).clone_into(&mut member.name);
        member.name_at = self.tokens.start(name_index);
        member.type_text = self.text_between(specifiers, name_index);
    }

    fn parameters(&self, start: usize, end: usize) -> Vec<CppParameter> {
        let mut cuts = CppShape::of(self.tokens, &(start..end)).commas;
        cuts.push(end);
        let mut first = start;
        let mut parameters = Vec::new();
        for cut in cuts {
            parameters.extend(self.parameter(first, cut));
            first = cut + 1;
        }
        parameters
    }

    fn parameter(&self, start: usize, end: usize) -> Option<CppParameter> {
        let (markers, first) = CppAttributes::read(self.tokens, self.source, start);
        let cut = (first..end)
            .find(|index| self.tokens.is(*index, "="))
            .unwrap_or(end);
        let only_void = [self.tokens.is(first, "void"), cut == first + 1] == [true, true];
        let present = [first < cut, !only_void] == [true, true];
        present.then(|| self.written_parameter(markers, first, cut))
    }

    fn parameter_name(&self, last: usize, named: bool) -> String {
        if !named {
            return String::new();
        }
        self.tokens.text(last).to_owned()
    }

    fn written_parameter(
        &self,
        markers: Vec<CppAttribute>,
        first: usize,
        cut: usize,
    ) -> CppParameter {
        let last = cut - 1;
        let named = [cut - first > 1, self.tokens.is_word(last)] == [true, true];
        let type_end = last + usize::from(!named);
        CppParameter {
            name: self.parameter_name(last, named),
            type_text: self.text_between(first, type_end),
            markers,
        }
    }
}
