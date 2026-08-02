//! Canonical JSON Schema documents for Tachyon's versioned public contracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Product identity embedded in CLI and release-facing native artifacts.
pub const PRODUCT_VERSION: &str = env!("TACHYON_PRODUCT_VERSION");

/// The JSON Schema draft required by every canonical Tachyon contract.
pub const JSON_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// A canonical machine contract shipped by Tachyon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contract {
    /// Stable contract name.
    pub name: &'static str,
    /// Stable schema identifier.
    pub id: &'static str,
    /// Current major schema version.
    pub major_version: u16,
    /// Canonical JSON Schema document.
    pub schema: &'static str,
    /// Example that every compatible implementation must accept.
    pub valid_example: &'static str,
    /// Example that every compatible implementation must reject.
    pub invalid_example: &'static str,
}

/// All public contracts. Registry order is stable and alphabetical.
pub const CONTRACTS: [Contract; 8] = [
    Contract {
        name: "artifact-manifest",
        id: "https://tachyon.del.ma/schema/artifact-manifest/v1",
        major_version: 1,
        schema: include_str!("../../../api/artifact-manifest/v1/schema.json"),
        valid_example: include_str!("../../../api/artifact-manifest/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/artifact-manifest/v1/examples/invalid.json"),
    },
    Contract {
        name: "capability-manifest",
        id: "https://tachyon.del.ma/schema/capability-manifest/v1",
        major_version: 1,
        schema: include_str!("../../../api/capability-manifest/v1/schema.json"),
        valid_example: include_str!("../../../api/capability-manifest/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/capability-manifest/v1/examples/invalid.json"),
    },
    Contract {
        name: "diagnostics",
        id: "https://tachyon.del.ma/schema/diagnostics/v1",
        major_version: 1,
        schema: include_str!("../../../api/diagnostics/v1/schema.json"),
        valid_example: include_str!("../../../api/diagnostics/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/diagnostics/v1/examples/invalid.json"),
    },
    Contract {
        name: "handler-protocol",
        id: "https://tachyon.del.ma/schema/handler-protocol/v1",
        major_version: 1,
        schema: include_str!("../../../api/handler-protocol/v1/schema.json"),
        valid_example: include_str!("../../../api/handler-protocol/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/handler-protocol/v1/examples/invalid.json"),
    },
    Contract {
        name: "native-ui",
        id: "https://tachyon.del.ma/schema/native-ui/v1",
        major_version: 1,
        schema: include_str!("../../../api/native-ui/v1/schema.json"),
        valid_example: include_str!("../../../api/native-ui/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/native-ui/v1/examples/invalid.json"),
    },
    Contract {
        name: "route-manifest",
        id: "https://tachyon.del.ma/schema/route-manifest/v1",
        major_version: 1,
        schema: include_str!("../../../api/route-manifest/v1/schema.json"),
        valid_example: include_str!("../../../api/route-manifest/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/route-manifest/v1/examples/invalid.json"),
    },
    Contract {
        name: "view-ir",
        id: "https://tachyon.del.ma/schema/view-ir/v1",
        major_version: 1,
        schema: include_str!("../../../api/view-ir/v1/schema.json"),
        valid_example: include_str!("../../../api/view-ir/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/view-ir/v1/examples/invalid.json"),
    },
    Contract {
        name: "view-source-map",
        id: "https://tachyon.del.ma/schema/view-source-map/v1",
        major_version: 1,
        schema: include_str!("../../../api/view-source-map/v1/schema.json"),
        valid_example: include_str!("../../../api/view-source-map/v1/examples/valid.json"),
        invalid_example: include_str!("../../../api/view-source-map/v1/examples/invalid.json"),
    },
];

/// View IR v1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewIr {
    /// Contract major version.
    pub contract_version: u8,
    /// Portable project-relative source path.
    pub source: String,
    /// Root view node.
    pub root: ViewNode,
}

impl ViewIr {
    /// Creates View IR v1.
    #[must_use]
    pub const fn v1(source: String, root: ViewNode) -> Self {
        Self {
            contract_version: 1,
            source,
            root,
        }
    }
}

/// One structural node in View IR v1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewNode {
    /// A standards-based HTML or web-component element.
    Element {
        /// Lowercase element tag.
        tag: String,
        /// Canonically ordered static and dynamic attribute source values.
        attributes: BTreeMap<String, String>,
        /// Child nodes.
        children: Vec<Self>,
    },
    /// A text node which may contain escaped interpolation source.
    Text {
        /// Source text.
        value: String,
    },
    /// A compiler conditional.
    Conditional {
        /// Bounded Tachyon expression.
        condition: String,
        /// Nodes emitted when the expression is truthy.
        then: Vec<Self>,
        /// Nodes emitted otherwise.
        #[serde(rename = "else")]
        otherwise: Vec<Self>,
    },
    /// A compiler iteration.
    Iteration {
        /// Local binding name.
        binding: String,
        /// Iterable expression.
        iterable: String,
        /// Nodes emitted for every item.
        body: Vec<Self>,
        /// Nodes emitted when the iterable is empty.
        empty: Vec<Self>,
    },
    /// A registered Tac component invocation.
    Component {
        /// Canonical component name.
        name: String,
        /// Canonically ordered property source values.
        properties: BTreeMap<String, String>,
        /// Invocation children supplied to the component slot.
        children: Vec<Self>,
    },
}

/// View Source Map v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewSourceMap {
    /// Contract major version.
    pub contract_version: u8,
    /// Portable output HTML path.
    pub output: String,
    /// Canonically ordered source paths.
    pub sources: Vec<String>,
    /// Canonically ordered generated-to-source mappings.
    pub mappings: Vec<ViewSourceMapping>,
}

impl ViewSourceMap {
    /// Creates View Source Map v1.
    #[must_use]
    pub const fn v1(
        output: String,
        sources: Vec<String>,
        mappings: Vec<ViewSourceMapping>,
    ) -> Self {
        Self {
            contract_version: 1,
            output,
            sources,
            mappings,
        }
    }
}

/// One byte-range mapping in View Source Map v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewSourceMapping {
    /// First generated HTML byte.
    pub generated_start: u64,
    /// Exclusive generated HTML byte.
    pub generated_end: u64,
    /// Portable project-relative input source.
    pub source: String,
    /// First input byte.
    pub source_start: u64,
    /// Exclusive input byte.
    pub source_end: u64,
}

/// A native application target represented by Native UI v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeTarget {
    /// Linux desktop.
    Linux,
    /// macOS desktop.
    Macos,
    /// Windows desktop.
    Windows,
    /// Android.
    Android,
    /// iOS.
    Ios,
}

/// One resolved Native UI v1 route document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeUi {
    /// Contract major version.
    pub contract_version: u8,
    /// Native platform target.
    pub target: NativeTarget,
    /// Resolved root node.
    pub root: NativeNode,
}

impl NativeUi {
    /// Creates a Native UI v1 document.
    #[must_use]
    pub const fn v1(target: NativeTarget, root: NativeNode) -> Self {
        Self {
            contract_version: 1,
            target,
            root,
        }
    }
}

/// Accessibility semantics carried into a native platform adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAccessibility {
    /// Platform-neutral semantic role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Accessible name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The source category for one `WebSurface`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSurfaceSource {
    /// A compiler-generated document contained by the application bundle.
    LocalBundle,
    /// An explicitly declared remote HTTPS URL.
    RemoteUrl,
}

/// Native bridge policy for one `WebSurface`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSurfaceBridge {
    /// No bridge is installed.
    None,
    /// A deny-by-default local capability bridge is installed.
    LocalCapabilities,
}

/// One resolved node in Native UI v1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeNode {
    /// A node handled by a fixed native platform adapter.
    NativeElement {
        /// Deterministic route-local element identity.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Stable adapter identifier.
        adapter: String,
        /// Canonically ordered primitive properties.
        properties: BTreeMap<String, Value>,
        /// Canonically ordered semantic event bindings.
        events: BTreeMap<String, String>,
        /// Optional accessibility semantics.
        #[serde(skip_serializing_if = "Option::is_none")]
        accessibility: Option<NativeAccessibility>,
        /// Resolved child nodes.
        children: Vec<Self>,
    },
    /// A native text value.
    Text {
        /// Resolved text.
        value: String,
    },
    /// An isolated web-rendered subtree.
    WebSurface {
        /// Deterministic route-local surface identity.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Local or remote content category.
        source: WebSurfaceSource,
        /// Bundle-relative path or HTTPS URL.
        location: String,
        /// Native bridge policy.
        bridge: WebSurfaceBridge,
        /// Inspectable fallback decision.
        reason: String,
        /// Optional fallback accessibility semantics.
        #[serde(skip_serializing_if = "Option::is_none")]
        accessibility: Option<NativeAccessibility>,
    },
}

/// Capability Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityManifest {
    /// Contract major version.
    pub contract_version: u8,
    /// Reverse-DNS application identifier.
    pub application_id: String,
    /// Deny-by-default policy.
    pub default_policy: String,
    /// Whether remote content may receive a bridge.
    pub remote_content_bridge: bool,
    /// Explicit capability grants.
    pub capabilities: Vec<CapabilityGrant>,
}

impl CapabilityManifest {
    /// Creates the empty deny-by-default Phase 4 manifest.
    #[must_use]
    pub fn deny_all(application_id: String) -> Self {
        Self {
            contract_version: 1,
            application_id,
            default_policy: String::from("deny"),
            remote_content_bridge: false,
            capabilities: Vec::new(),
        }
    }
}

/// One explicit native capability grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityGrant {
    /// Stable capability name.
    pub name: String,
    /// Bounded resource scopes.
    pub scope: Vec<String>,
    /// Human-readable justification.
    pub reason: String,
    /// Target names receiving the grant.
    pub targets: Vec<String>,
}

/// Artifact Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifest {
    /// Contract major version.
    pub contract_version: u8,
    /// Tachyon development or release version.
    pub release_version: String,
    /// Source revision.
    pub commit: String,
    /// Reproducible build epoch.
    pub source_date_epoch: u64,
    /// Target tuple.
    pub target: ArtifactTarget,
    /// External toolchain identities.
    pub toolchains: Vec<ArtifactToolchain>,
    /// Contract major versions consumed by the artifact.
    pub contracts: ArtifactContractVersions,
    /// Canonically ordered output digests.
    pub outputs: Vec<ArtifactOutput>,
}

/// Target tuple in Artifact Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactTarget {
    /// Operating-system name.
    pub os: String,
    /// Architecture name.
    pub architecture: String,
    /// ABI or platform-runtime identifier.
    pub abi: String,
}

/// One toolchain identity in Artifact Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactToolchain {
    /// Stable toolchain name.
    pub name: String,
    /// Bounded version string.
    pub version: String,
}

/// Canonical contract versions consumed by one artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactContractVersions {
    /// Artifact Manifest major version.
    pub artifact_manifest: u8,
    /// Capability Manifest major version.
    pub capability_manifest: u8,
    /// Diagnostics major version.
    pub diagnostics: u8,
    /// Handler Protocol major version.
    pub handler_protocol: u8,
    /// Native UI major version.
    pub native_ui: u8,
    /// Route Manifest major version.
    pub route_manifest: u8,
    /// View IR major version.
    pub view_ir: u8,
}

impl Default for ArtifactContractVersions {
    fn default() -> Self {
        Self {
            artifact_manifest: 1,
            capability_manifest: 1,
            diagnostics: 1,
            handler_protocol: 1,
            native_ui: 1,
            route_manifest: 1,
            view_ir: 1,
        }
    }
}

/// One hashed file in Artifact Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactOutput {
    /// Portable path relative to the artifact root.
    pub path: String,
    /// Lowercase SHA-256 digest.
    pub sha256: String,
    /// File length in bytes.
    pub size: u64,
}

/// Route Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteManifest {
    /// Contract major version.
    pub contract_version: u8,
    /// Canonically ordered application routes.
    pub routes: Vec<RouteEntry>,
}

impl RouteManifest {
    /// Creates a Route Manifest v1.
    #[must_use]
    pub const fn v1(routes: Vec<RouteEntry>) -> Self {
        Self {
            contract_version: 1,
            routes,
        }
    }
}

/// One route in Route Manifest v1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteEntry {
    /// Canonical URL route. A dynamic segment keeps its `_name` form.
    pub route: String,
    /// Ordered dynamic segment names, empty for a fully static route.
    pub parameters: Vec<String>,
    /// Route behavior category.
    pub kind: RouteKind,
    /// Supported HTTP methods.
    pub methods: Vec<HttpMethod>,
    /// Project-relative view source.
    pub view: Option<String>,
    /// Handler contributors, empty before Phase 2.
    pub handlers: Vec<RouteHandler>,
    /// Route-context declaration.
    pub context: RouteContext,
}

/// Route behavior category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// A browser-visible page.
    Page,
    /// A non-visual API endpoint.
    Api,
}

/// HTTP methods representable in Route Manifest v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// DELETE.
    Delete,
    /// GET.
    Get,
    /// HEAD.
    Head,
    /// OPTIONS.
    Options,
    /// PATCH.
    Patch,
    /// POST.
    Post,
    /// PUT.
    Put,
}

/// A language handler contributing to a route.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteHandler {
    /// Project-relative source path.
    pub source: String,
    /// Handler language identifier.
    pub language: String,
    /// Runtime adapter identifier.
    pub runtime: String,
}

/// Route-context collision and export declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteContext {
    /// Duplicate export behavior.
    pub collision_policy: CollisionPolicy,
    /// Static handler exports.
    pub static_exports: Vec<String>,
    /// Method response exports.
    pub response_exports: Vec<String>,
}

impl Default for RouteContext {
    fn default() -> Self {
        Self {
            collision_policy: CollisionPolicy::Error,
            static_exports: Vec::new(),
            response_exports: Vec::new(),
        }
    }
}

/// Route-context collision behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    /// Reject duplicate route-context keys.
    Error,
}

/// Canonically ordered HTTP headers for Handler Protocol v1.
pub type HandlerHeaders = BTreeMap<String, Vec<String>>;

/// Body encoding supported by Handler Protocol v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandlerBodyEncoding {
    /// UTF-8 text.
    Utf8,
    /// RFC 4648 base64 text.
    Base64,
}

/// A bounded Handler Protocol v1 body.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerBody {
    /// Body encoding.
    pub encoding: HandlerBodyEncoding,
    /// Encoded body data.
    pub data: String,
}

/// The constant request envelope kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HandlerRequestKind {
    /// Handler invocation request.
    #[serde(rename = "request")]
    Request,
}

/// A Handler Protocol v1 invocation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerRequest {
    /// Protocol major version.
    pub protocol_version: u8,
    /// Envelope kind.
    pub kind: HandlerRequestKind,
    /// Caller-provided correlation identifier.
    pub request_id: String,
    /// Stable operation identifier.
    pub operation: String,
    /// Canonical route.
    pub route: String,
    /// Selected HTTP method.
    pub method: HttpMethod,
    /// Optional deadline in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    /// Canonically ordered request headers.
    pub headers: HandlerHeaders,
    /// Dynamic route parameters bound from the request path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
    /// Optional request body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<HandlerBody>,
}

impl HandlerRequest {
    /// Creates a Handler Protocol v1 route invocation.
    #[must_use]
    pub fn route(
        request_id: impl Into<String>,
        route: impl Into<String>,
        method: HttpMethod,
    ) -> Self {
        Self {
            protocol_version: 1,
            kind: HandlerRequestKind::Request,
            request_id: request_id.into(),
            operation: String::from("route.invoke"),
            route: route.into(),
            method,
            deadline_ms: None,
            headers: HandlerHeaders::new(),
            parameters: BTreeMap::new(),
            body: None,
        }
    }
}

/// The constant response envelope kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HandlerResponseKind {
    /// Handler invocation response.
    #[serde(rename = "response")]
    Response,
}

/// A bounded Handler Protocol v1 error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerProtocolError {
    /// Stable Tachyon diagnostic code.
    pub code: String,
    /// Bounded public error message.
    pub message: String,
    /// Whether a caller may safely retry.
    pub retryable: bool,
}

/// A Handler Protocol v1 response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerResponse {
    /// Protocol major version.
    pub protocol_version: u8,
    /// Envelope kind.
    pub kind: HandlerResponseKind,
    /// Matching request identifier.
    pub request_id: String,
    /// HTTP-compatible response status.
    pub status: u16,
    /// Canonically ordered response headers.
    pub headers: HandlerHeaders,
    /// Successful response body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<HandlerBody>,
    /// Failed response details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<HandlerProtocolError>,
}

/// JSON contribution returned by the private `view.context` build operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerContextContribution {
    /// Public own data fields declared on the handler class.
    ///
    /// Absent means none, so a handler contributing only response values does
    /// not have to write an empty object.
    #[serde(default)]
    pub static_values: BTreeMap<String, Value>,
    /// Object entries returned from static `GET()`.
    ///
    /// Absent means none, for the same reason.
    #[serde(default)]
    pub response_values: BTreeMap<String, Value>,
}

impl HandlerResponse {
    /// Creates a successful Handler Protocol v1 response.
    #[must_use]
    pub fn success(
        request_id: impl Into<String>,
        status: u16,
        headers: HandlerHeaders,
        body: HandlerBody,
    ) -> Self {
        Self {
            protocol_version: 1,
            kind: HandlerResponseKind::Response,
            request_id: request_id.into(),
            status,
            headers,
            body: Some(body),
            error: None,
        }
    }

    /// Creates a failed Handler Protocol v1 response.
    #[must_use]
    pub fn error(request_id: impl Into<String>, status: u16, error: HandlerProtocolError) -> Self {
        Self {
            protocol_version: 1,
            kind: HandlerResponseKind::Response,
            request_id: request_id.into(),
            status,
            headers: HandlerHeaders::new(),
            body: None,
            error: Some(error),
        }
    }
}

/// The constant cancellation envelope kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HandlerCancelKind {
    /// Handler invocation cancellation.
    #[serde(rename = "cancel")]
    Cancel,
}

/// A Handler Protocol v1 cancellation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerCancel {
    /// Protocol major version.
    pub protocol_version: u8,
    /// Envelope kind.
    pub kind: HandlerCancelKind,
    /// Request identifier to cancel.
    pub request_id: String,
}

impl HandlerCancel {
    /// Creates a Handler Protocol v1 cancellation.
    #[must_use]
    pub fn v1(request_id: impl Into<String>) -> Self {
        Self {
            protocol_version: 1,
            kind: HandlerCancelKind::Cancel,
            request_id: request_id.into(),
        }
    }
}

/// Any Handler Protocol v1 envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HandlerEnvelope {
    /// Invocation request.
    Request(HandlerRequest),
    /// Invocation response.
    Response(HandlerResponse),
    /// Cancellation request.
    Cancel(HandlerCancel),
}

/// Finds a canonical contract by stable name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Contract> {
    CONTRACTS.iter().find(|contract| contract.name == name)
}

/// Parses a contract's canonical schema.
///
/// The repository gate guarantees every registered schema parses successfully;
/// this function remains fallible so callers never need to trust embedded input.
///
/// # Errors
///
/// Returns a JSON parsing error if the embedded canonical schema is malformed.
pub fn parse_schema(contract: &Contract) -> Result<Value, serde_json::Error> {
    serde_json::from_str(contract.schema)
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{
        ArtifactContractVersions, ArtifactManifest, ArtifactOutput, ArtifactTarget,
        ArtifactToolchain, CONTRACTS, CapabilityManifest, CollisionPolicy, HandlerBody,
        HandlerBodyEncoding, HandlerCancel, HandlerEnvelope, HandlerHeaders, HandlerProtocolError,
        HandlerRequest, HandlerResponse, HttpMethod, JSON_SCHEMA_DRAFT, NativeAccessibility,
        NativeNode, NativeTarget, NativeUi, RouteContext, RouteEntry, RouteHandler, RouteKind,
        RouteManifest, WebSurfaceBridge, WebSurfaceSource, find, parse_schema,
    };
    use std::collections::BTreeMap;

    #[test]
    fn every_schema_is_canonical_and_examples_prove_both_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        for contract in &CONTRACTS {
            let schema = parse_schema(contract)?;
            jsonschema::meta::validate(&schema).map_err(|error| {
                io::Error::other(format!(
                    "{} does not satisfy its JSON meta-schema: {error}",
                    contract.name
                ))
            })?;

            assert_eq!(schema["$schema"], JSON_SCHEMA_DRAFT, "{}", contract.name);
            assert_eq!(schema["$id"], contract.id, "{}", contract.name);

            let validator = jsonschema::validator_for(&schema).map_err(|error| {
                io::Error::other(format!("{} could not compile: {error}", contract.name))
            })?;
            let valid = serde_json::from_str(contract.valid_example)?;
            let invalid = serde_json::from_str(contract.invalid_example)?;

            assert!(validator.is_valid(&valid), "{}", contract.name);
            assert!(!validator.is_valid(&invalid), "{}", contract.name);
        }

        Ok(())
    }

    #[test]
    fn registry_names_and_ids_are_unique_and_ordered() {
        let mut names = CONTRACTS
            .iter()
            .map(|contract| contract.name)
            .collect::<Vec<_>>();
        let original_names = names.clone();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names, original_names);

        let mut ids = CONTRACTS
            .iter()
            .map(|contract| contract.id)
            .collect::<Vec<_>>();
        let original_len = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), original_len);
    }

    #[test]
    fn contracts_are_discoverable_by_stable_name() {
        assert_eq!(
            find("view-ir").map(|contract| contract.major_version),
            Some(1)
        );
        assert!(find("unknown").is_none());
    }

    #[test]
    fn route_manifest_types_have_the_canonical_wire_shape() -> Result<(), serde_json::Error> {
        let default_context = RouteContext::default();
        assert_eq!(default_context.collision_policy, CollisionPolicy::Error);
        assert!(default_context.static_exports.is_empty());

        let methods = vec![
            HttpMethod::Delete,
            HttpMethod::Get,
            HttpMethod::Head,
            HttpMethod::Options,
            HttpMethod::Patch,
            HttpMethod::Post,
            HttpMethod::Put,
        ];
        let manifest = RouteManifest::v1(vec![RouteEntry {
            route: String::from("/api"),
            parameters: Vec::new(),
            kind: RouteKind::Api,
            methods,
            view: None,
            handlers: vec![RouteHandler {
                source: String::from("server/routes/api/yon.js"),
                language: String::from("javascript"),
                runtime: String::from("bun"),
            }],
            context: RouteContext {
                collision_policy: CollisionPolicy::Error,
                static_exports: vec![String::from("title")],
                response_exports: vec![String::from("products")],
            },
        }]);
        let value = serde_json::to_value(manifest)?;
        assert_eq!(value["contract_version"], 1);
        assert_eq!(value["routes"][0]["kind"], "api");
        assert_eq!(value["routes"][0]["methods"][0], "DELETE");
        assert_eq!(value["routes"][0]["methods"][6], "PUT");
        assert_eq!(value["routes"][0]["context"]["collision_policy"], "error");

        let page = serde_json::to_value(RouteManifest::v1(vec![RouteEntry {
            route: String::from("/"),
            parameters: Vec::new(),
            kind: RouteKind::Page,
            methods: vec![HttpMethod::Get],
            view: Some(String::from("client/pages/tac.html")),
            handlers: Vec::new(),
            context: default_context,
        }]))?;
        assert_eq!(page["routes"][0]["kind"], "page");
        Ok(())
    }

    #[test]
    fn native_and_packaging_types_have_canonical_wire_shapes() -> Result<(), serde_json::Error> {
        let native = NativeUi::v1(
            NativeTarget::Macos,
            NativeNode::NativeElement {
                id: Some(String::from("n_root")),
                adapter: String::from("layout.column"),
                properties: BTreeMap::new(),
                events: BTreeMap::new(),
                accessibility: Some(NativeAccessibility {
                    role: Some(String::from("main")),
                    label: Some(String::from("Catalog")),
                }),
                children: vec![NativeNode::WebSurface {
                    id: Some(String::from("n_chart")),
                    source: WebSurfaceSource::LocalBundle,
                    location: String::from("WebSurfaces/n_chart/index.html"),
                    bridge: WebSurfaceBridge::None,
                    reason: String::from("No native chart adapter."),
                    accessibility: Some(NativeAccessibility {
                        role: Some(String::from("group")),
                        label: Some(String::from("Chart")),
                    }),
                }],
            },
        );
        let native_value = serde_json::to_value(native)?;
        assert_eq!(native_value["target"], "macos");
        assert_eq!(native_value["root"]["kind"], "native_element");
        assert_eq!(native_value["root"]["children"][0]["bridge"], "none");

        let capability = CapabilityManifest::deny_all(String::from("dev.tachyon.catalog"));
        let capability_value = serde_json::to_value(capability)?;
        assert_eq!(capability_value["default_policy"], "deny");
        assert_eq!(capability_value["remote_content_bridge"], false);

        let artifact = ArtifactManifest {
            contract_version: 1,
            release_version: String::from("0.0.0-phase4"),
            commit: "0".repeat(40),
            source_date_epoch: 0,
            target: ArtifactTarget {
                os: String::from("macos"),
                architecture: String::from("aarch64"),
                abi: String::from("swiftui"),
            },
            toolchains: vec![ArtifactToolchain {
                name: String::from("swift"),
                version: String::from("6.3.3"),
            }],
            contracts: ArtifactContractVersions::default(),
            outputs: vec![ArtifactOutput {
                path: String::from("PhaseFour.app/Contents/MacOS/PhaseFour"),
                sha256: "0".repeat(64),
                size: 1,
            }],
        };
        let artifact_value = serde_json::to_value(artifact)?;
        assert_eq!(artifact_value["contracts"]["native_ui"], 1);
        assert_eq!(artifact_value["target"]["os"], "macos");
        Ok(())
    }

    #[test]
    fn handler_protocol_types_cover_every_canonical_envelope() -> Result<(), serde_json::Error> {
        let mut request = HandlerRequest::route("req_01", "/products", HttpMethod::Post);
        request.deadline_ms = Some(2_000);
        request.headers.insert(
            String::from("content-type"),
            vec![String::from("application/json")],
        );
        request.body = Some(HandlerBody {
            encoding: HandlerBodyEncoding::Utf8,
            data: String::from(r#"{"name":"Ada"}"#),
        });
        let request_value = serde_json::to_value(HandlerEnvelope::Request(request.clone()))?;
        assert_eq!(request_value["kind"], "request");
        assert_eq!(request_value["method"], "POST");
        assert_eq!(
            serde_json::from_value::<HandlerEnvelope>(request_value)?,
            HandlerEnvelope::Request(request)
        );

        let mut headers = HandlerHeaders::new();
        headers.insert(
            String::from("content-type"),
            vec![String::from("application/json; charset=utf-8")],
        );
        let response = HandlerResponse::success(
            "req_01",
            200,
            headers,
            HandlerBody {
                encoding: HandlerBodyEncoding::Utf8,
                data: String::from(r#"{"ok":true}"#),
            },
        );
        let response_value = serde_json::to_value(HandlerEnvelope::Response(response.clone()))?;
        assert_eq!(response_value["kind"], "response");
        assert!(response_value.get("error").is_none());
        assert_eq!(
            serde_json::from_value::<HandlerEnvelope>(response_value)?,
            HandlerEnvelope::Response(response)
        );

        let failure = HandlerResponse::error(
            "req_02",
            500,
            HandlerProtocolError {
                code: String::from("TY2201"),
                message: String::from("Handler failed."),
                retryable: false,
            },
        );
        let failure_value = serde_json::to_value(failure)?;
        assert!(failure_value.get("body").is_none());
        assert_eq!(failure_value["error"]["code"], "TY2201");

        let cancel = HandlerCancel::v1("req_01");
        let cancel_value = serde_json::to_value(HandlerEnvelope::Cancel(cancel.clone()))?;
        assert_eq!(cancel_value["kind"], "cancel");
        assert_eq!(
            serde_json::from_value::<HandlerEnvelope>(cancel_value)?,
            HandlerEnvelope::Cancel(cancel)
        );
        Ok(())
    }
}
