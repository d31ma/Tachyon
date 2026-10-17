use super::super::testing::Unit;
use super::test_clients::TestClients;
use super::{Module, Type};

pub(super) struct RustTestConstruction<'a> {
    module: &'a Module,
    owner: &'a Type,
    unit: &'a Unit,
}

impl<'a> RustTestConstruction<'a> {
    const FAKE_START: &'static str = "std::cell::RefCell<Option<";
    const FAKE_END: &'static str = ">>";
    const SHARED_START: &'static str = "std::sync::Arc<";
    const SHARED_END: &'static str = ">";
    const CASE_INPUT: char = '[';
    const NO_SETUP: &'static str = "Test.construct needs an active test setup.";
    const CONSTRUCT_TRAIT: &'static str =
        "pub trait BehaviorConstruct<T> { fn construct(&self) -> T; }\n";
    const CELL_STARTS: [&'static str; 4] = [
        "std::cell::RefCell<",
        "std::cell::Cell<",
        "RefCell<",
        "Cell<",
    ];

    pub(super) fn members(module: &'a Module, owner: &'a Type, unit: &'a Unit) -> String {
        Self {
            module,
            owner,
            unit,
        }
        .source()
    }

    pub(super) fn cleared_fakes(owner: &Type, unit: &Unit) -> String {
        let lines: Vec<String> = owner
            .fields
            .iter()
            .filter(|field| unit.mocks.contains(&field.name))
            .filter(|field| Self::is_cell(&field.type_source))
            .map(|field| format!("self.{}.take();\n", field.name))
            .collect();
        lines.concat()
    }

    fn is_cell(kind: &str) -> bool {
        let kind = kind.trim();
        Self::CELL_STARTS
            .iter()
            .any(|start| kind.starts_with(start))
    }

    pub(super) fn wrapped_call(module: &Module, name: &str, call: &str) -> String {
        let case = name.split(Self::CASE_INPUT).next().unwrap_or(name);
        let waits = module.source.contains(&format!("async fn {case}("));
        [call.to_owned(), format!("BehaviorTasks::finish({call})")][usize::from(waits)].clone()
    }

    fn source(&self) -> String {
        let implementations = self
            .module
            .types
            .iter()
            .filter(|target| self.constructible(target))
            .filter_map(|target| target.constructor.as_ref().map(|fields| (target, fields)))
            .map(|(target, fields)| self.implementation(target, fields))
            .collect::<String>();
        format!("{}{implementations}", Self::CONSTRUCT_TRAIT)
    }

    fn constructible(&self, target: &Type) -> bool {
        [
            !target.declared,
            target.name != self.owner.name,
            !TestClients::sends_requests(target),
        ] == [true; 3]
    }

    fn implementation(&self, target: &Type, fields: &[String]) -> String {
        format!(
            "impl BehaviorConstruct<{0}> for {1} {{ fn construct(&self) -> {0} {{ {2} }} }}\n",
            target.name,
            self.owner.name,
            self.body(target, fields)
        )
    }

    fn body(&self, target: &Type, fields: &[String]) -> String {
        super::typed_test_fakes::TypedTestFakes::new(self.owner, self.unit, Self::faked)
            .names(target, fields)
            .map_or_else(
                |message| format!("panic!({message:?})"),
                |names| {
                    let arguments = names
                        .iter()
                        .map(|name| self.fake(target, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}::new({arguments})", target.name)
                },
            )
    }

    fn fake(&self, target: &Type, name: &str) -> String {
        let assigned = format!("self.{name}.borrow().clone()");
        self.client_of(target, name).map_or_else(
            || Self::required(&assigned, name),
            |client| Self::defaulted(&assigned, &client),
        )
    }

    fn client_of(&self, target: &Type, name: &str) -> Option<String> {
        target
            .fields
            .iter()
            .find(|field| field.name == name)
            .and_then(|field| {
                field
                    .type_source
                    .trim()
                    .strip_prefix(Self::SHARED_START)?
                    .strip_suffix(Self::SHARED_END)
                    .map(str::to_owned)
            })
            .filter(|client| TestClients::is_client(self.module, client))
    }

    fn required(assigned: &str, name: &str) -> String {
        let message = format!("Test.construct needs an assigned Fake named {name}.");
        format!("{assigned}.expect({message:?})")
    }

    fn defaulted(assigned: &str, client: &str) -> String {
        format!(
            "{assigned}.unwrap_or_else(|| std::sync::Arc::new({client}::with_context(self.behaviors.borrow().clone().expect({:?}))))",
            Self::NO_SETUP
        )
    }

    fn faked(kind: &str) -> &str {
        kind.trim()
            .strip_prefix(Self::FAKE_START)
            .and_then(|inner| inner.strip_suffix(Self::FAKE_END))
            .unwrap_or(kind.trim())
    }
}
