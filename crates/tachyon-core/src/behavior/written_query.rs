use serde_json::Value;

pub(crate) struct WrittenQuery<'q> {
    pub(crate) at: usize,
    pub(crate) method: &'q str,
    pub(crate) arity: usize,
    pub(crate) query: Option<&'q Value>,
    pub(crate) page: Option<&'q Value>,
}
