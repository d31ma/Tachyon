use super::{CppClass, CppFile, CppMember, CppMemberKind};
use crate::Failure;
use std::path::Path;

pub(crate) struct CppCoverage {
    pub(crate) class: String,
    pub(crate) methods: Vec<(String, usize)>,
}

impl CppCoverage {
    const ROLES: [&'static str; 3] = ["Service", "HTTPClient", "Helper"];

    pub(crate) fn read(relative: &Path, source: &str) -> Result<Option<Self>, Failure> {
        let file = CppFile::read(relative, source)?;
        let class = &file.class;
        Ok(Self::has_role(class).then(|| Self::of_class(source, class)))
    }

    fn has_role(class: &CppClass) -> bool {
        class
            .markers
            .iter()
            .any(|marker| Self::ROLES.contains(&marker.name.as_str()))
    }

    fn of_class(source: &str, class: &CppClass) -> Self {
        let methods = Self::covered_members(source, &class.members)
            .into_iter()
            .map(|member| (member.name.clone(), Self::line(source, member.name_at)))
            .collect();
        Self {
            class: class.name.clone(),
            methods,
        }
    }

    pub(crate) fn covered_members<'a>(
        source: &str,
        members: &'a [CppMember],
    ) -> Vec<&'a CppMember> {
        members
            .iter()
            .filter(|member| member.is_public_instance(CppMemberKind::Method))
            .filter(|member| Self::has_body(source, member))
            .collect()
    }

    fn has_body(source: &str, member: &CppMember) -> bool {
        source.as_bytes().get(member.terminator) == Some(&b'{')
    }

    fn line(source: &str, at: usize) -> usize {
        let before = source.get(..at).unwrap_or(source);
        before.matches('\n').count()
    }
}
