use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CppFill {
    pub(crate) path: Vec<String>,
    pub(crate) constant: Option<Value>,
}

impl CppFill {
    pub(crate) fn from_path(path: Vec<String>) -> Self {
        Self {
            path,
            constant: None,
        }
    }

    pub(crate) fn from_constant(constant: Value) -> Self {
        Self {
            path: Vec::new(),
            constant: Some(constant),
        }
    }
}
