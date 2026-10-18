#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CppInjection {
    pub(crate) field: String,
    pub(crate) provider: String,
    pub(crate) terminator: usize,
}
