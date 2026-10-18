pub(crate) struct WrittenMarker<'m> {
    pub(crate) name: &'m str,
    pub(crate) arguments: &'m str,
    pub(crate) at: usize,
}
