pub(crate) struct WrittenField<'f> {
    pub(crate) name: &'f str,
    pub(crate) type_source: &'f str,
    pub(crate) initializer: Option<&'f str>,
}
