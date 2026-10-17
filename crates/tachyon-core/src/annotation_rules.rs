use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::path::Path;

mod field_case;
mod field_declaration;
mod field_name;
mod injected_field;
mod nullable_field;
mod placed_marker;
mod scan;

pub(crate) use placed_marker::PlacedMarker;
pub(crate) use scan::Messages;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod injected_field_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    Type,
    Method,
    Field,
    Parameter,
}

impl Target {
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Type => "class",
            Self::Method => "method",
            Self::Field => "field",
            Self::Parameter => "parameter",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Arguments {
    None,
    Optional,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Realm {
    Tac,
    Yon,
}

impl Realm {
    pub(crate) fn of(path: &Path) -> Option<Self> {
        for part in path.components() {
            match part.as_os_str().to_str() {
                Some("client") => return Some(Self::Tac),
                Some("server") => return Some(Self::Yon),
                _ => {}
            }
        }
        None
    }

    pub(crate) fn named(environment: &str) -> Option<Self> {
        match environment {
            "tac" => Some(Self::Tac),
            "yon" => Some(Self::Yon),
            _ => None,
        }
    }

    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Tac => "Tac",
            Self::Yon => "Yon",
        }
    }

    pub(crate) fn languages(self) -> &'static [&'static str] {
        match self {
            Self::Tac => AnnotationRules::TAC,
            Self::Yon => AnnotationRules::YON,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rule {
    pub(crate) name: &'static str,
    pub(crate) meaning: &'static str,
    pub(crate) targets: &'static [Target],
    pub(crate) arguments: Arguments,
    pub(crate) tac: &'static [&'static str],
    pub(crate) yon: &'static [&'static str],
    pub(crate) instance: bool,
    pub(crate) with: &'static [&'static str],
    pub(crate) clashes: &'static [&'static str],
    pub(crate) class_needs: &'static [&'static str],
    pub(crate) one_per_method: bool,
    pub(crate) refuses_constructor: bool,
    pub(crate) refuses_nullable: bool,
    pub(crate) argument_advice: &'static str,
    pub(crate) replaced_by: &'static str,
    pub(crate) advice: &'static str,
    pub(crate) code: u16,
}

impl Rule {
    const fn new(name: &'static str, targets: &'static [Target], meaning: &'static str) -> Self {
        Self {
            name,
            meaning,
            targets,
            arguments: Arguments::Optional,
            tac: AnnotationRules::TAC_BEHAVIOR,
            yon: AnnotationRules::YON_BEHAVIOR,
            instance: false,
            with: &[],
            clashes: &[],
            class_needs: &[],
            one_per_method: false,
            refuses_constructor: false,
            refuses_nullable: false,
            argument_advice: "",
            replaced_by: "",
            advice: "",
            code: 1308,
        }
    }

    const fn retired(name: &'static str, replaced_by: &'static str, advice: &'static str) -> Self {
        Self {
            replaced_by,
            advice,
            targets: &[],
            tac: &[],
            yon: &[],
            ..Self::new(name, &[], "")
        }
    }

    const fn bare(self) -> Self {
        Self {
            arguments: Arguments::None,
            ..self
        }
    }

    const fn needs_arguments(self) -> Self {
        Self {
            arguments: Arguments::Required,
            ..self
        }
    }

    const fn reach(self, tac: &'static [&'static str], yon: &'static [&'static str]) -> Self {
        Self { tac, yon, ..self }
    }

    const fn instance(self) -> Self {
        Self {
            instance: true,
            ..self
        }
    }

    const fn with(self, with: &'static [&'static str]) -> Self {
        Self { with, ..self }
    }

    const fn clashes(self, clashes: &'static [&'static str]) -> Self {
        Self { clashes, ..self }
    }

    const fn class_needs(self, class_needs: &'static [&'static str]) -> Self {
        Self {
            class_needs,
            ..self
        }
    }

    const fn one_per_method(self) -> Self {
        Self {
            one_per_method: true,
            ..self
        }
    }

    const fn refuses_constructor(self) -> Self {
        Self {
            refuses_constructor: true,
            ..self
        }
    }

    const fn refuses_nullable(self) -> Self {
        Self {
            refuses_nullable: true,
            ..self
        }
    }

    const fn argument_advice(self, argument_advice: &'static str) -> Self {
        Self {
            argument_advice,
            ..self
        }
    }

    const fn advice(self, advice: &'static str) -> Self {
        Self { advice, ..self }
    }

    const fn code(self, code: u16) -> Self {
        Self { code, ..self }
    }

    pub(crate) fn python_annotated(&self) -> bool {
        self.targets.contains(&Target::Field) || self.targets.contains(&Target::Parameter)
    }

    pub(crate) fn is_retired(&self) -> bool {
        self.targets.is_empty()
    }

    pub(crate) fn languages(&self, realm: Realm) -> &'static [&'static str] {
        match realm {
            Realm::Tac => self.tac,
            Realm::Yon => self.yon,
        }
    }

    pub(crate) fn allows(&self, language: &str, realm: Realm) -> bool {
        self.languages(realm).contains(&language)
    }

    pub(crate) fn allowed_targets(&self) -> String {
        let words: Vec<String> = self
            .targets
            .iter()
            .map(|target| format!("a {}", target.word()))
            .collect();
        words.join(" or ")
    }

    pub(crate) fn explained(&self) -> String {
        format!("{} {}.", self.name, self.meaning)
    }
}

pub(crate) struct AnnotationRules;

impl AnnotationRules {
    pub(crate) const LANGUAGES: [&'static str; 16] = [
        "js", "ts", "py", "php", "java", "groovy", "kt", "cs", "rs", "dart", "go", "swift", "rb",
        "lua", "fs", "hpp",
    ];
    pub(crate) const YON: &'static [&'static str] = &[
        "js", "ts", "py", "php", "java", "groovy", "kt", "cs", "fs", "rs", "dart", "go", "rb",
        "lua",
    ];
    const YON_BEHAVIOR: &'static [&'static str] = &[
        "js", "ts", "py", "php", "java", "groovy", "kt", "cs", "fs", "rs", "dart", "go", "rb",
        "lua",
    ];
    pub(crate) const YON_RESOURCE: &'static [&'static str] = &[
        "js", "ts", "py", "php", "java", "groovy", "kt", "cs", "fs", "rs", "dart", "go", "rb",
        "lua",
    ];
    const TAC: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "lua", "py", "hpp",
    ];
    const TAC_EXCEPT_CPP: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "lua", "py",
    ];
    const TAC_ROLE: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "lua", "py", "java", "fs", "hpp",
    ];
    pub(crate) const TAC_RESOURCE: &'static [&'static str] =
        &["js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb"];
    const TAC_REMOTE: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "hpp",
    ];
    const TAC_BEHAVIOR: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "py",
    ];
    const TAC_FAKE: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "py", "hpp",
    ];
    const TAC_HELPER: &'static [&'static str] =
        &["js", "ts", "dart", "rs", "py", "cs", "fs", "go", "rb"];
    const TAC_NEEDS_ALL: &'static [&'static str] = &[
        "js", "ts", "rs", "cs", "dart", "go", "swift", "kt", "rb", "java", "lua", "py",
    ];
    pub(crate) const YON_BACKGROUND: &'static [&'static str] =
        &["js", "ts", "py", "kt", "dart", "cs", "fs", "lua", "rb"];
    const YON_STREAM: &'static [&'static str] = &[
        "js", "ts", "py", "php", "kt", "cs", "fs", "dart", "rb", "lua", "go",
    ];
    const TAC_STREAM: &'static [&'static str] = &["js", "ts", "go", "rb", "rs"];
    const TAC_VALUE_SOURCE: &'static [&'static str] = &[
        "js", "ts", "rs", "kt", "cs", "go", "rb", "dart", "swift", "lua", "hpp", "py",
    ];
    const YON_VALUE_SOURCE: &'static [&'static str] = &[
        "js", "ts", "py", "rs", "java", "groovy", "kt", "cs", "fs", "go", "rb", "php", "dart",
        "lua",
    ];
    const NOWHERE: &'static [&'static str] = &[];
    const TYPE: &'static [Target] = &[Target::Type];
    const METHOD: &'static [Target] = &[Target::Method];
    const FIELD: &'static [Target] = &[Target::Field];
    const PARAMETER: &'static [Target] = &[Target::Parameter];
    const FIELD_OR_TYPE: &'static [Target] = &[Target::Field, Target::Type];
    const TYPE_OR_METHOD: &'static [Target] = &[Target::Type, Target::Method];
    const ACCESS: &'static [&'static str] = &["Public", "SignedIn", "Allow"];
    const CONTROLLERS: &'static [&'static str] = &["RequestRoute", "SocketRoute"];
    const ACCESS_ADVICE: &'static str = "Every route says who may call it (ADR 0068). Write @Public, @SignedIn or @Allow(Role) on the controller class or on a route method; the method wins";
    const TEST_CLASSES: &'static [&'static str] = &["Test", "TestSetup"];
    const SETUP_CLASSES: &'static [&'static str] = &["Setup", "TestSetup"];
    const ANSWERS: &'static [&'static str] = &[
        "GetRoute",
        "PostRoute",
        "PutRoute",
        "PatchRoute",
        "DeleteRoute",
        "StreamRoute",
        "HandlesError",
    ];
    const HTTP_METHODS: &'static [&'static str] = &[
        "GetRoute",
        "PostRoute",
        "PutRoute",
        "PatchRoute",
        "DeleteRoute",
        "StreamRoute",
        "GetRequest",
        "PostRequest",
        "PutRequest",
        "PatchRequest",
        "DeleteRequest",
        "StreamRequest",
        "SocketRequest",
    ];
    const SOCKET_EVENTS: &'static [&'static str] = &["OnOpen", "OnMessage", "OnClose", "OnError"];
    const SOCKET_ADVICE: &'static str = "A socket class is a Yon controller in server/controllers/, written in any Yon language. In the browser, receive its messages with @SocketRequest on an @HTTPClient method";
    const ROUTE_SOURCES: &'static [&'static str] = &[
        "FromPath",
        "FromQuery",
        "FromBody",
        "FromHeader",
        "FromCookie",
    ];
    const REQUEST_PARTS: &'static [&'static str] = &[
        "PathVariable",
        "RequestParam",
        "RequestBody",
        "RequestHeader",
    ];
    const CACHE_CHANGES: &'static [&'static str] = &["Cached", "ClearCache", "UpdateCache"];
    const LIFECYCLE: &'static [&'static str] = &["OnStart", "OnStop", "OnEvent"];
    const HELPER_ADVICE: &'static str = "Tac Helper supports JavaScript, TypeScript, Dart, compiled Rust, the synchronous Python ownership adapter, Go Helpers owned by the service that needs or creates them, Ruby Helpers that their service creates on first use, and caller-owned C#/F# composition. A Ruby Helper has no OnStart, OnStop or OnEvent. C# and F# Helpers require explicit construction and do not support managed Helper injection or lifecycle hooks. F# Helpers require an ordinary primary constructor and do not support authored Needs properties. Other compiled languages need a qualified same-language Helper constructor and ownership adapter; use a supported role rather than replacing Service with Helper";
    const RUBY_NEEDS_ADVICE: &'static str = "A Ruby NeedsAll class lists each field as `attr_reader :order_store` and writes no `initialize`; a Ruby class that needs default values writes a plain `initialize` and no NeedsAll";

    pub(crate) const ALL: &'static [Rule] = &[
        Rule::new(
            "Test",
            Self::TYPE,
            "marks a test class and names the TestSetup class it runs with",
        )
        .needs_arguments()
        .reach(Self::TAC, Self::YON),
        Rule::new(
            "TestSetup",
            Self::TYPE,
            "marks the class that prepares the objects a test uses",
        )
        .bare()
        .reach(Self::TAC, Self::YON),
        Rule::new(
            "Fake",
            Self::FIELD,
            "puts a stand-in object into this field while a test runs",
        )
        .bare()
        .class_needs(Self::TEST_CLASSES)
        .reach(Self::TAC_FAKE, Self::YON),
        Rule::new(
            "ValueSource",
            Self::METHOD,
            "runs this test case once for each input in its list",
        )
        .needs_arguments()
        .instance()
        .reach(Self::TAC_VALUE_SOURCE, Self::YON_VALUE_SOURCE)
        .class_needs(Self::TEST_CLASSES),
        Rule::new(
            "UseProfiles",
            Self::TYPE,
            "turns on the named profiles while this test runs",
        )
        .needs_arguments()
        .reach(Self::TAC_EXCEPT_CPP, Self::YON)
        .class_needs(Self::TEST_CLASSES),
        Rule::new(
            "TestSettings",
            Self::TYPE,
            "sets setting values while this test runs",
        )
        .needs_arguments()
        .class_needs(Self::TEST_CLASSES),
        Rule::new(
            "FreshStart",
            Self::TYPE,
            "builds new objects for every test instead of sharing them",
        )
        .bare()
        .class_needs(Self::TEST_CLASSES),
        Rule::retired(
            "Client",
            "HTTPClient",
            "HTTPClient marks a class that sends HTTP requests and gives the path they start with",
        ),
        Rule::new(
            "Id",
            Self::FIELD,
            "marks the field that identifies each saved record",
        )
        .bare()
        .class_needs(&["Entity"]),
        Rule::new(
            "Service",
            Self::TYPE,
            "marks a class that other classes can use",
        )
        .bare()
        .reach(Self::TAC_ROLE, Self::YON),
        Rule::new(
            "Lifetime",
            Self::TYPE,
            "says how long one object of this class lives",
        )
        .needs_arguments()
        .reach(Self::TAC_EXCEPT_CPP, Self::YON),
        Rule::new(
            "Helper",
            Self::TYPE,
            "marks a class that services use for part of their work",
        )
        .bare()
        .reach(Self::TAC_HELPER, Self::YON)
        .advice(Self::HELPER_ADVICE),
        Rule::new(
            "Converter",
            Self::TYPE,
            "marks a class whose methods copy one model into another",
        )
        .bare()
        .reach(Self::TAC_EXCEPT_CPP, Self::YON_BEHAVIOR),
        Rule::new(
            "Convert",
            Self::METHOD,
            "copies the fields of one model into a new model",
        )
        .reach(Self::TAC_EXCEPT_CPP, Self::YON_BEHAVIOR),
        Rule::new(
            "Needs",
            Self::FIELD_OR_TYPE,
            "asks Tachyon to supply an object that this class uses",
        )
        .reach(Self::TAC, Self::YON)
        .refuses_nullable(),
        Rule::new(
            "Named",
            Self::FIELD,
            "chooses which object fills a Needs field when there are several",
        )
        .needs_arguments()
        .reach(Self::TAC_EXCEPT_CPP, Self::YON)
        .with(&["Needs"]),
        Rule::new(
            "Setup",
            Self::TYPE,
            "marks a class that builds and configures shared objects",
        )
        .bare(),
        Rule::new(
            "Description",
            Self::TYPE,
            "gives the class a short description for tools and documents",
        )
        .needs_arguments(),
        Rule::new(
            "Schedule",
            Self::TYPE,
            "runs this worker again and again after a fixed delay",
        )
        .needs_arguments()
        .reach(Self::NOWHERE, Self::YON)
        .code(1502),
        Rule::new(
            "NeedsAll",
            Self::TYPE,
            "requires every field that has no initial value: collaborators are injected and the other fields come from the component's props",
        )
        .bare()
        .reach(Self::TAC_NEEDS_ALL, Self::YON)
        .advice(Self::RUBY_NEEDS_ADVICE)
        .refuses_constructor()
        .refuses_nullable()
        .argument_advice("NeedsAll takes no field list; every field can be filled"),
        Rule::new(
            "LocalState",
            Self::FIELD,
            "keeps this field in the browser's localStorage, so its value comes back after a reload and in later visits on this device",
        )
        .bare()
        .reach(Self::TAC_BEHAVIOR, Self::NOWHERE)
        .clashes(&["SessionState", "Needs"]),
        Rule::new(
            "SessionState",
            Self::FIELD,
            "keeps this field in the browser's sessionStorage, so its value comes back after a reload in the same tab",
        )
        .bare()
        .reach(Self::TAC_BEHAVIOR, Self::NOWHERE)
        .clashes(&["LocalState", "Needs"]),
        Rule::new(
            "CacheSettings",
            Self::TYPE,
            "names the cache that this class's Cached methods use, and sets its limits",
        )
        .needs_arguments()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR),
        Rule::new(
            "Cached",
            Self::METHOD,
            "keeps a method's answer and returns it again for the same arguments",
        )
        .bare()
        .instance()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR)
        .clashes(Self::CACHE_CHANGES)
        .class_needs(&["CacheSettings"]),
        Rule::new(
            "CacheKey",
            Self::PARAMETER,
            "makes this argument part of the key of a cached answer",
        )
        .bare()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR),
        Rule::new(
            "ClearCache",
            Self::METHOD,
            "removes the saved answers of a Cached method after this method runs",
        )
        .needs_arguments()
        .instance()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR)
        .clashes(Self::CACHE_CHANGES),
        Rule::new(
            "UpdateCache",
            Self::METHOD,
            "saves this method's answer as the new answer of a Cached method",
        )
        .needs_arguments()
        .instance()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR)
        .clashes(Self::CACHE_CHANGES),
        Rule::new(
            "PageDefaults",
            Self::PARAMETER,
            "gives the page size and sort order that a page argument uses when the caller leaves them out",
        )
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR),
        Rule::new("Retry", Self::METHOD, "runs the method again when it fails")
            .instance()
            .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR),
        Rule::new(
            "MaxAtOnce",
            Self::METHOD,
            "limits how many calls of this method run at the same time",
        )
        .instance()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR),
        Rule::new(
            "AllOrNothing",
            Self::METHOD,
            "saves all of the method's changes, or none of them when it fails",
        )
        .instance()
        .reach(Self::TAC_BEHAVIOR, Self::YON_BEHAVIOR),
        Rule::new(
            "Entity",
            Self::TYPE,
            "marks a model that is saved as a record",
        ),
        Rule::new(
            "Topic",
            Self::TYPE,
            "sends each published event of this model to the listeners in every Yon language and in open pages",
        )
        .bare()
        .reach(Self::TAC_EXCEPT_CPP, Self::YON),
        Rule::new(
            "Table",
            Self::TYPE,
            "names the table that stores this Entity",
        )
        .needs_arguments()
        .class_needs(&["Entity"]),
        Rule::retired(
            "WebCall",
            "GetRequest",
            "write @GetRequest, @PostRequest, @PutRequest, @PatchRequest or @DeleteRequest with the path inside: calling the method then sends that HTTP request",
        ),
        Rule::new(
            "Gateway",
            Self::METHOD,
            "calls a method of a class that is written in another language",
        )
        .needs_arguments()
        .instance()
        .reach(Self::NOWHERE, Self::YON_RESOURCE),
        Rule::new(
            "Store",
            Self::TYPE,
            "marks a class that saves and finds records",
        )
        .reach(Self::TAC_RESOURCE, Self::YON),
        Rule::new(
            "Query",
            Self::METHOD,
            "turns an empty method into a query of the Store's records",
        )
        .reach(Self::TAC_RESOURCE, Self::YON_BEHAVIOR)
        .instance()
        .class_needs(&["Store"]),
        Rule::new(
            "Settings",
            Self::TYPE,
            "fills the class's fields from the settings whose names share a prefix",
        )
        .needs_arguments(),
        Rule::new(
            "WhenProfile",
            Self::TYPE,
            "uses this Setup only when one of the named profiles is on",
        )
        .needs_arguments()
        .class_needs(Self::SETUP_CLASSES),
        Rule::new(
            "WhenSetting",
            Self::TYPE,
            "uses this Setup only when a setting has the given value",
        )
        .needs_arguments()
        .class_needs(Self::SETUP_CLASSES),
        Rule::new(
            "Setting",
            Self::FIELD,
            "fills this field with one setting value",
        )
        .needs_arguments(),
        Rule::new(
            "Checked",
            Self::TYPE,
            "marks a model whose fields are checked before a method uses it",
        )
        .bare(),
        Rule::new(
            "Check",
            Self::PARAMETER,
            "checks the fields of this argument before the method runs",
        ),
        Rule::new(
            "Required",
            Self::FIELD,
            "refuses a value when this field is missing",
        )
        .bare(),
        Rule::new(
            "NotEmpty",
            Self::FIELD,
            "refuses a value when this field is empty",
        )
        .bare(),
        Rule::new(
            "Length",
            Self::FIELD,
            "limits how long the text or list in this field may be",
        )
        .needs_arguments(),
        Rule::new(
            "Min",
            Self::FIELD,
            "refuses a number in this field that is below the given value",
        )
        .needs_arguments(),
        Rule::new(
            "Max",
            Self::FIELD,
            "refuses a number in this field that is above the given value",
        )
        .needs_arguments(),
        Rule::new(
            "OnEvent",
            Self::METHOD,
            "runs the method when an event of its parameter's type is published",
        )
        .instance()
        .reach(Self::TAC, Self::YON)
        .clashes(Self::LIFECYCLE),
        Rule::new(
            "OnStart",
            Self::METHOD,
            "runs the method once when the object starts",
        )
        .bare()
        .instance()
        .reach(Self::TAC, Self::YON)
        .clashes(Self::LIFECYCLE),
        Rule::new(
            "OnStop",
            Self::METHOD,
            "runs the method once when the object stops",
        )
        .bare()
        .instance()
        .reach(Self::TAC, Self::YON)
        .clashes(Self::LIFECYCLE),
        Rule::retired(
            "Resource",
            "Needs",
            "write @Needs on a field whose name says which object it receives (ADR 0038)",
        ),
        Rule::new(
            "Order",
            Self::METHOD,
            "sets when this listener runs compared with the other listeners of the event",
        )
        .needs_arguments()
        .reach(Self::TAC, Self::YON)
        .with(&["OnEvent"]),
        Rule::new(
            "InBackground",
            Self::METHOD,
            "runs this listener without making the publisher wait",
        )
        .bare()
        .reach(Self::TAC, Self::YON_BACKGROUND)
        .with(&["OnEvent"])
        .advice("InBackground listeners require a Yon language that defers them."),
        Rule::retired(
            "Behaviors",
            "",
            "Behaviors is a name that Tachyon generates for its own code; it is not an annotation",
        ),
        Rule::new(
            "RequestRoute",
            Self::TYPE,
            "marks a controller and gives the path that its routes start with",
        )
        .reach(Self::NOWHERE, Self::YON)
        .code(2015),
        Rule::new(
            "ErrorHandler",
            Self::TYPE,
            "marks a class whose methods answer the errors of controllers",
        )
        .reach(Self::NOWHERE, Self::YON)
        .code(2015),
        Rule::new(
            "HandlesError",
            Self::METHOD,
            "answers one kind of error with an HTTP response",
        )
        .bare()
        .reach(Self::NOWHERE, Self::YON)
        .class_needs(&["ErrorHandler"])
        .code(2015),
        Rule::new(
            "GetRoute",
            Self::METHOD,
            "means this method answers GET requests",
        )
        .route(),
        Rule::new(
            "PostRoute",
            Self::METHOD,
            "means this method answers POST requests",
        )
        .route(),
        Rule::new(
            "PutRoute",
            Self::METHOD,
            "means this method answers PUT requests",
        )
        .route(),
        Rule::new(
            "PatchRoute",
            Self::METHOD,
            "means this method answers PATCH requests",
        )
        .route(),
        Rule::new(
            "DeleteRoute",
            Self::METHOD,
            "means this method answers DELETE requests",
        )
        .route(),
        Rule::new(
            "ResponseStatus",
            Self::METHOD,
            "sets the HTTP status of a normal answer",
        )
        .needs_arguments()
        .reach(Self::NOWHERE, Self::YON)
        .with(Self::ANSWERS)
        .code(2015),
        Rule::new(
            "FromPath",
            Self::PARAMETER,
            "fills this argument from a path variable, such as {id}",
        )
        .route_source(),
        Rule::new(
            "FromQuery",
            Self::PARAMETER,
            "fills this argument from the query string, after ?",
        )
        .route_source(),
        Rule::new(
            "FromBody",
            Self::PARAMETER,
            "fills this argument from the request body, read as JSON",
        )
        .route_source()
        .one_per_method(),
        Rule::new(
            "FromHeader",
            Self::PARAMETER,
            "fills this argument from a request header",
        )
        .route_source(),
        Rule::new(
            "FromCookie",
            Self::PARAMETER,
            "fills this argument from a cookie",
        )
        .route_source(),
        Rule::new(
            "HTTPClient",
            Self::TYPE,
            "marks a class that sends HTTP requests and gives the path they start with",
        )
        .reach(Self::TAC_REMOTE, Self::YON_RESOURCE),
        Rule::new(
            "GetRequest",
            Self::METHOD,
            "means calling this method sends a GET request",
        )
        .request(),
        Rule::new(
            "PostRequest",
            Self::METHOD,
            "means calling this method sends a POST request",
        )
        .request(),
        Rule::new(
            "PutRequest",
            Self::METHOD,
            "means calling this method sends a PUT request",
        )
        .request(),
        Rule::new(
            "PatchRequest",
            Self::METHOD,
            "means calling this method sends a PATCH request",
        )
        .request(),
        Rule::new(
            "DeleteRequest",
            Self::METHOD,
            "means calling this method sends a DELETE request",
        )
        .request(),
        Rule::new(
            "PathVariable",
            Self::PARAMETER,
            "puts this argument into the request path",
        )
        .request_part(),
        Rule::new(
            "RequestParam",
            Self::PARAMETER,
            "adds this argument to the query string of the request",
        )
        .request_part(),
        Rule::new(
            "RequestBody",
            Self::PARAMETER,
            "sends this argument as the request body, written as JSON",
        )
        .request_part()
        .one_per_method(),
        Rule::new(
            "RequestHeader",
            Self::PARAMETER,
            "sends this argument as a request header",
        )
        .request_part(),
        Rule::new(
            "StreamRequest",
            Self::METHOD,
            "means Tachyon keeps a stream of server-sent events open and passes each message to this method",
        )
        .request()
        .instance()
        .reach(Self::TAC_STREAM, Self::NOWHERE)
        .advice("The browser's JavaScript holds the stream, so write the @StreamRequest client in JavaScript, TypeScript, Go, Ruby or Rust. Each message is also published as a typed event, so a service in any Tac language receives it with @OnEvent on a method whose parameter is the message model"),
        Rule::new(
            "StreamRoute",
            Self::METHOD,
            "means this method answers GET requests with a stream of server-sent events",
        )
        .route()
        .reach(Self::NOWHERE, Self::YON_STREAM),
        Rule::new(
            "SocketRequest",
            Self::METHOD,
            "means Tachyon keeps a WebSocket open and passes each incoming message to this method",
        )
        .request()
        .instance()
        .reach(Self::TAC_STREAM, Self::NOWHERE)
        .advice("The browser's JavaScript holds the socket, so write the @SocketRequest client in JavaScript, TypeScript, Go, Ruby or Rust. Each message is also published as a typed event, so a service in any Tac language receives it with @OnEvent on a method whose parameter is the message model"),
        Rule::new(
            "SocketRoute",
            Self::TYPE,
            "marks a controller that keeps a WebSocket connection open and gives its path",
        )
        .reach(Self::NOWHERE, Self::YON)
        .clashes(&["RequestRoute"])
        .advice(Self::SOCKET_ADVICE)
        .code(2015),
        Rule::new(
            "OnOpen",
            Self::METHOD,
            "runs the method when a browser opens the socket",
        )
        .socket_event(),
        Rule::new(
            "OnMessage",
            Self::METHOD,
            "runs the method for each message that the browser sends on the socket",
        )
        .socket_event(),
        Rule::new("OnClose", Self::METHOD, "runs the method when the socket closes")
            .socket_event(),
        Rule::new(
            "OnError",
            Self::METHOD,
            "runs the method when opening the socket or handling a message fails",
        )
        .socket_event(),
        Rule::retired(
            "Stream",
            "StreamRoute",
            "write @StreamRoute alone instead of @GetRoute and @Stream, and start the method name with 'stream'. `ty migrate fix --write` converts it",
        ),
    ];

    const ACCESS_RULES: &'static [Rule] = &[
        Rule::new(
            "Public",
            Self::TYPE_OR_METHOD,
            "means anyone may call this route, also a caller who is not signed in",
        )
        .access(),
        Rule::new(
            "SignedIn",
            Self::TYPE_OR_METHOD,
            "means any signed-in user may call this route",
        )
        .access(),
        Rule::new(
            "Allow",
            Self::TYPE_OR_METHOD,
            "means only a signed-in user with one of these roles may call this route",
        )
        .access()
        .needs_arguments()
        .argument_advice("@Allow takes one or more role types from server/roles/, for example @Allow(Admin)"),
        Rule::new(
            "IdentityProvider",
            Self::TYPE,
            "marks the one class that reads the request and says who is signed in and which roles they have",
        )
        .bare()
        .reach(Self::NOWHERE, Self::YON)
        .clashes(&["RequestRoute", "SocketRoute", "ErrorHandler"])
        .advice("Put the one @IdentityProvider class in server/controllers/<Name>IdentityProvider.<ext>, with one method identify(request)")
        .code(2015),
    ];

    pub(crate) fn rules() -> impl Iterator<Item = &'static Rule> {
        Self::ALL.iter().chain(Self::ACCESS_RULES)
    }

    pub(crate) fn check(path: &Path, source: &str) -> Result<(), Failure> {
        crate::tac_cpp::CppMarkerCheck::either(path, source, |path, source| {
            crate::fsharp_source::FsharpView::run(path, source, Self::check_fsharp)
                .unwrap_or_else(|| Self::check_direct(path, source))
        })
    }

    fn check_fsharp(path: &Path, source: &str) -> Result<(), Failure> {
        Self::check_direct(path, source)?;
        let in_yon = Realm::of(path) == Some(Realm::Yon);
        let found = scan::Scan::run_as("cs", "cs", source);
        let outside = found.outside_reach("fs", Realm::Yon).filter(|_| in_yon);
        outside.map_or(Ok(()), |(at, rule)| {
            let message = scan::Messages::unavailable(rule, "cs", Realm::Yon);
            Err(Self::failure_at(path, source, at, rule.code, &message))
        })
    }

    pub(crate) fn check_lua_shadow(path: &Path, source: &str) -> Result<(), Failure> {
        Self::check_as(path, source, crate::lua_source::LuaSource::EXTENSION)
    }

    fn check_direct(path: &Path, source: &str) -> Result<(), Failure> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        crate::lua_source::LuaView::of(path, source).map_or_else(
            || Self::check_as(path, source, extension),
            |view| {
                Self::check_as(
                    view.path(),
                    view.text(),
                    crate::lua_source::LuaSource::EXTENSION,
                )
                .map_err(|failure| view.failure(&failure))
            },
        )
    }

    fn check_as(path: &Path, source: &str, spelling: &str) -> Result<(), Failure> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !Self::LANGUAGES.contains(&extension) {
            return Ok(());
        }
        let found = scan::Scan::run_as(extension, spelling, source);
        let realm = Realm::of(path);
        let problem = found
            .problem(realm)
            .map(|(at, rule, message)| (at, rule.code, message))
            .or_else(|| found.nullable_problem(source, realm))
            .or_else(|| found.injected_problem(source, realm));
        let Some((at, code, message)) = problem else {
            return Ok(());
        };
        Err(Self::failure_at(path, source, at, code, &message))
    }

    fn failure_at(path: &Path, source: &str, at: usize, code: u16, message: &str) -> Failure {
        let portable = path.to_string_lossy().replace('\\', "/");
        let line = source[..at.min(source.len())].matches('\n').count() + 1;
        Failure::one(diagnostic(
            code,
            format!("{} ({portable}:{line})", message.trim_end_matches('.')),
            Some(String::from(
                "Each annotation checks the class, method, field or parameter it is written on (ADR 0056). docs/BEHAVIOR_ANNOTATIONS.md lists where each one goes.",
            )),
            source_span(&portable, at, (at + 1).min(source.len())),
        ))
    }

    pub(crate) fn injected_fields(path: &Path, source: &str) -> Vec<injected_field::InjectedField> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        scan::Scan::run(extension, source).injected_fields(source, Realm::of(path))
    }

    pub(crate) fn nullable_fields(path: &Path, source: &str) -> Vec<nullable_field::NullableField> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        scan::Scan::run(extension, source).nullable_fields(source, Realm::of(path))
    }

    pub(crate) fn framework_members(path: &Path, source: &str) -> Vec<usize> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        scan::Scan::run(extension, source).framework_members(source, Realm::of(path))
    }

    pub(crate) fn find(written: &str) -> Option<&'static Rule> {
        let plain = written.rsplit("::").next().unwrap_or(written);
        if let Some(rule) = Self::rules().find(|rule| rule.name == plain) {
            return Some(rule);
        }
        let normalized = plain.replace('_', "").to_ascii_lowercase();
        Self::rules().find(|rule| rule.name.to_ascii_lowercase() == normalized)
    }

    pub(crate) fn python_parameter(name: &str) -> String {
        match Self::find(name) {
            Some(rule) => scan::Messages::python_annotated(rule, Target::Parameter),
            None => format!("{name} goes inside the parameter's type in Python"),
        }
    }

    pub(crate) fn needs_all_constructor(language: &str) -> String {
        Self::find("NeedsAll").map_or_else(String::new, |rule| {
            scan::Messages::constructor(rule, language)
        })
    }

    pub(crate) fn no_arguments(name: &str) -> String {
        match Self::find(name) {
            Some(rule) if !rule.argument_advice.is_empty() => {
                format!("{}. {}", rule.argument_advice, rule.explained())
            }
            Some(rule) => format!(
                "{name} accepts no arguments: write it alone. {}",
                rule.explained()
            ),
            None => format!("{name} accepts no arguments"),
        }
    }

    pub(crate) fn belongs(name: &str) -> String {
        match Self::find(name) {
            Some(rule) if !rule.is_retired() => format!(
                "{name} belongs on {}. {}",
                rule.allowed_targets(),
                rule.explained()
            ),
            _ => format!("{name} is not written here"),
        }
    }

    pub(crate) fn checked_refusal(name: &str, language: &str, realm: Realm) -> Option<String> {
        let rule = Self::find(name)?;
        let checked = realm.languages().contains(&language);
        (checked && !rule.allows(language, realm))
            .then(|| scan::Messages::unavailable(rule, language, realm))
    }

    pub(crate) fn unavailable(name: &str, language: &str, environment: &str) -> Option<String> {
        let rule = Self::find(name)?;
        let realm = Realm::named(environment)?;
        if rule.allows(language, realm) {
            return None;
        }
        Some(scan::Messages::unavailable(rule, language, realm))
    }
}

impl Rule {
    const fn route(self) -> Self {
        Self {
            tac: AnnotationRules::NOWHERE,
            yon: AnnotationRules::YON,
            instance: true,
            clashes: AnnotationRules::HTTP_METHODS,
            class_needs: &["RequestRoute"],
            code: 2015,
            ..self
        }
    }

    const fn access(self) -> Self {
        Self {
            arguments: Arguments::None,
            tac: AnnotationRules::NOWHERE,
            yon: AnnotationRules::YON,
            clashes: AnnotationRules::ACCESS,
            class_needs: AnnotationRules::CONTROLLERS,
            advice: AnnotationRules::ACCESS_ADVICE,
            code: 2015,
            ..self
        }
    }

    const fn socket_event(self) -> Self {
        Self {
            arguments: Arguments::None,
            tac: AnnotationRules::NOWHERE,
            yon: AnnotationRules::YON,
            instance: true,
            clashes: AnnotationRules::SOCKET_EVENTS,
            class_needs: &["SocketRoute"],
            advice: AnnotationRules::SOCKET_ADVICE,
            code: 2015,
            ..self
        }
    }

    const fn request(self) -> Self {
        Self {
            tac: AnnotationRules::TAC_REMOTE,
            yon: AnnotationRules::YON_RESOURCE,
            clashes: AnnotationRules::HTTP_METHODS,
            class_needs: &["HTTPClient"],
            ..self
        }
    }

    const fn route_source(self) -> Self {
        Self {
            tac: AnnotationRules::NOWHERE,
            yon: AnnotationRules::YON,
            clashes: AnnotationRules::ROUTE_SOURCES,
            class_needs: &["RequestRoute", "IdentityProvider"],
            code: 2015,
            ..self
        }
    }

    const fn request_part(self) -> Self {
        Self {
            tac: AnnotationRules::TAC_REMOTE,
            yon: AnnotationRules::YON_RESOURCE,
            clashes: AnnotationRules::REQUEST_PARTS,
            class_needs: &["HTTPClient"],
            ..self
        }
    }
}
