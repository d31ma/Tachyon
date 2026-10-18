use super::CppFill;

pub(crate) struct CppConversion {
    pub(crate) typed: bool,
    pub(crate) fills: Vec<(String, CppFill)>,
}
