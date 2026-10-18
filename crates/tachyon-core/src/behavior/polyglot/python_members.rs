use super::{Field, Input, Method, Type};
use crate::Failure;
use crate::route_types::{Shape, TypeText};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) struct PythonMembers;

impl PythonMembers {
    pub(crate) fn read(path: &Path, source: &str, class: &str) -> Result<Value, Failure> {
        let source = super::super::ServiceRoles::without_roles(path, source)?;
        let plain = Self::plain(path, &source)?;
        let input = Self::input(path, &plain)?;
        let types = super::declarations(&input, "py")?;
        let mut description = Self::named(&input, &types, class)?;
        let generated = Self::generated(path, &source, class)?;
        description["inferred"] = json!(generated.is_some());
        description["qualifiers"] = Self::qualifiers(path, &source, class)?;
        Self::set_generated(&mut description, generated);
        Ok(description)
    }

    fn plain(path: &Path, source: &str) -> Result<String, Failure> {
        let input = Self::input(path, source)?;
        Ok(super::python_plain(&input, "py").map_or_else(|| source.to_owned(), |(plain, _)| plain))
    }

    fn generated(path: &Path, source: &str, class: &str) -> Result<Option<Vec<String>>, Failure> {
        let prepared = super::prepare(path, source, "tac")?;
        Ok(Self::generated_fields(prepared.as_ref(), class))
    }

    fn generated_fields(prepared: Option<&super::Prepared>, class: &str) -> Option<Vec<String>> {
        let super::Prepared::Annotated(module) = prepared? else {
            return None;
        };
        module
            .types
            .iter()
            .find(|ty| ty.name == class)?
            .constructor
            .clone()
    }

    fn qualifiers(path: &Path, source: &str, class: &str) -> Result<Value, Failure> {
        let prepared = super::prepare(path, source, "tac")?;
        Ok(Self::prepared_qualifiers(prepared.as_ref(), class))
    }

    fn prepared_qualifiers(prepared: Option<&super::Prepared>, class: &str) -> Value {
        let Some(super::Prepared::Annotated(module)) = prepared else {
            return json!({});
        };
        module
            .types
            .iter()
            .find(|ty| ty.name == class)
            .and_then(|ty| ty.policies.get("qualifiers"))
            .cloned()
            .unwrap_or_else(|| json!({}))
    }

    fn set_generated(description: &mut Value, generated: Option<Vec<String>>) {
        if let Some(fields) = generated {
            description["constructor"] = json!(fields);
        }
    }

    fn input<'a>(path: &'a Path, source: &'a str) -> Result<Input<'a>, Failure> {
        let code = crate::annotation_layout::masked_source(path, source)?;
        Ok(Input { path, source, code })
    }

    fn named(input: &Input<'_>, types: &[Type], class: &str) -> Result<Value, Failure> {
        let ty = types
            .iter()
            .find(|ty| ty.name == class)
            .ok_or_else(|| input.failure(0, "Python service needs its named class"))?;
        Self::describe(input, ty)
    }

    fn describe(input: &Input<'_>, ty: &Type) -> Result<Value, Failure> {
        let methods = Self::methods(input, ty)?;
        let fields = ty
            .fields
            .iter()
            .filter(|field| Self::public_field(field))
            .map(Self::field)
            .collect::<Vec<_>>();
        let public = methods
            .iter()
            .filter(|method| Self::public_method(method))
            .map(|method| Self::method(input, method))
            .collect::<Result<Vec<_>, _>>()?;
        let constructor = methods
            .iter()
            .find(|method| method.name == "__init__")
            .map(|method| {
                method
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(json!({ "fields": fields, "methods": public, "constructor": constructor }))
    }

    fn methods(input: &Input<'_>, ty: &Type) -> Result<Vec<Method>, Failure> {
        let body = &input.code[ty.body.clone()];
        let indent = body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(4);
        body.split_inclusive('\n')
            .scan(ty.body.start, |offset, line| {
                let position = (*offset, indent);
                *offset += line.len();
                Some(Self::method_on_line(input, ty, line, position))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|methods| methods.into_iter().flatten().collect())
    }

    fn method_on_line(
        input: &Input<'_>,
        ty: &Type,
        line: &str,
        position: (usize, usize),
    ) -> Result<Option<Method>, Failure> {
        let (offset, indent) = position;
        Self::method_line(line, indent)
            .then(|| {
                super::parse_method(
                    input,
                    "py",
                    offset + line.len() - line.trim_start().len(),
                    ty,
                )
            })
            .transpose()
    }

    fn method_line(line: &str, indent: usize) -> bool {
        let text = line.trim_start();
        let direct = line.len() - text.len() == indent;
        direct
            && ["def ", "async def "]
                .into_iter()
                .any(|prefix| text.starts_with(prefix))
    }

    fn public_field(field: &Field) -> bool {
        !field.is_static && !field.name.starts_with('_')
    }

    fn field(field: &Field) -> Value {
        json!({
            "name": field.name,
            "page": crate::companion::camel_case(&field.name),
            "type": Self::field_type(&field.type_source),
            "immutable": Self::field_type(&field.type_source) != field.type_source,
            "uninitialized": field.uninitialized,
        })
    }

    fn field_type(written: &str) -> &str {
        ["Final[", "typing.Final["]
            .into_iter()
            .find_map(|prefix| {
                written
                    .strip_prefix(prefix)
                    .and_then(|inner| inner.strip_suffix(']'))
            })
            .map_or(written, str::trim)
    }

    fn public_method(method: &Method) -> bool {
        !method.is_static && !method.name.starts_with('_')
    }

    fn method(input: &Input<'_>, method: &Method) -> Result<Value, Failure> {
        let arguments = method
            .parameters
            .iter()
            .map(|parameter| Self::shape(input, &parameter.type_source))
            .collect::<Result<Vec<_>, _>>()?;
        let returns = Self::shape(input, &method.return_type)?;
        Ok(json!({
            "name": method.name,
            "page": crate::companion::camel_case(&method.name),
            "arguments": arguments,
            "argumentTypes": method.parameters.iter().map(|parameter| &parameter.type_source).collect::<Vec<_>>(),
            "returns": returns,
            "returnType": method.return_type,
            "asynchronous": method.asynchronous,
        }))
    }

    fn shape(input: &Input<'_>, written: &str) -> Result<Shape, Failure> {
        if written.is_empty() {
            return Ok(Shape::Any);
        }
        TypeText::shape("py", super::PythonAnnotatedType::base(written))
            .map_err(|message| input.failure(0, message))
    }
}
