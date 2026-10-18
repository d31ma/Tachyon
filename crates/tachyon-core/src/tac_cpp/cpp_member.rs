use super::{CppAttribute, CppMemberKind, CppParameter, CppTypeText};

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CppMember {
    pub(crate) kind: CppMemberKind,
    pub(crate) name: String,
    pub(crate) name_at: usize,
    pub(crate) type_text: String,
    pub(crate) parameters: Vec<CppParameter>,
    pub(crate) public: bool,
    pub(crate) is_static: bool,
    pub(crate) has_initializer: bool,
    pub(crate) declarators: usize,
    pub(crate) markers: Vec<CppAttribute>,
    pub(crate) terminator: usize,
    pub(crate) body_end: usize,
    pub(crate) is_const: bool,
}

impl CppMember {
    pub(crate) fn marked(&self, name: &str) -> bool {
        self.markers.iter().any(|marker| marker.name == name)
    }

    pub(crate) fn plain_type(&self) -> String {
        CppTypeText::plain(&self.type_text)
    }

    pub(crate) fn is_reference(&self) -> bool {
        [self.type_text.contains('&'), self.type_text.contains('*')].contains(&true)
    }

    pub(crate) fn is_instance(&self, kind: CppMemberKind) -> bool {
        [self.kind == kind, !self.is_static] == [true, true]
    }

    pub(crate) fn is_task(&self) -> bool {
        let plain = self.plain_type();
        [
            plain.starts_with("Task<"),
            plain.starts_with("tachyon::Task<"),
        ]
        .contains(&true)
    }

    pub(crate) fn is_public_instance(&self, kind: CppMemberKind) -> bool {
        [self.is_instance(kind), self.public] == [true, true]
    }
}
