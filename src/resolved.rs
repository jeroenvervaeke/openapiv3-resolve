//! Resolved counterparts of the [`openapiv3`] types.
//!
//! Each type that originally contained `ReferenceOr<T>` is mirrored here with
//! a `Resolved*` variant whose reference fields use [`ResolvedRefOr<T>`]. Types
//! that do not contain references (e.g. [`Info`], [`Server`], [`SchemaData`])
//! are reused unchanged.

use indexmap::IndexMap;
use openapiv3::*;

use crate::handle::{Resolved, ResolvedRefOr};

/// Fully-resolved counterpart of [`openapiv3::OpenAPI`].
#[derive(Debug, Clone)]
pub struct ResolvedOpenAPI {
    pub openapi: String,
    pub info: Info,
    pub servers: Vec<Server>,
    pub paths: ResolvedPaths,
    pub components: ResolvedComponents,
    pub security: Option<Vec<SecurityRequirement>>,
    pub tags: Vec<Tag>,
    pub external_docs: Option<ExternalDocumentation>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Components`]. Each inner map holds
/// strong [`Resolved`] handles which keep every reachable component alive
/// for the lifetime of the document.
#[derive(Debug, Clone, Default)]
pub struct ResolvedComponents {
    pub schemas: IndexMap<String, Resolved<ResolvedSchema>>,
    pub responses: IndexMap<String, Resolved<ResolvedResponse>>,
    pub parameters: IndexMap<String, Resolved<ResolvedParameter>>,
    pub examples: IndexMap<String, Resolved<Example>>,
    pub request_bodies: IndexMap<String, Resolved<ResolvedRequestBody>>,
    pub headers: IndexMap<String, Resolved<ResolvedHeader>>,
    pub security_schemes: IndexMap<String, Resolved<SecurityScheme>>,
    pub links: IndexMap<String, Resolved<Link>>,
    pub callbacks: IndexMap<String, Resolved<ResolvedCallback>>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Paths`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedPaths {
    /// Path items keyed by their templated path. Path items cannot be stored
    /// in `components` in OpenAPI 3.0, so any `$ref` here would be an
    /// external reference; the resolver rejects those.
    pub paths: IndexMap<String, Resolved<ResolvedPathItem>>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::PathItem`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedPathItem {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub get: Option<ResolvedOperation>,
    pub put: Option<ResolvedOperation>,
    pub post: Option<ResolvedOperation>,
    pub delete: Option<ResolvedOperation>,
    pub options: Option<ResolvedOperation>,
    pub head: Option<ResolvedOperation>,
    pub patch: Option<ResolvedOperation>,
    pub trace: Option<ResolvedOperation>,
    pub servers: Vec<Server>,
    pub parameters: Vec<ResolvedRefOr<ResolvedParameter>>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Operation`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedOperation {
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub external_docs: Option<ExternalDocumentation>,
    pub operation_id: Option<String>,
    pub parameters: Vec<ResolvedRefOr<ResolvedParameter>>,
    pub request_body: Option<ResolvedRefOr<ResolvedRequestBody>>,
    pub responses: ResolvedResponses,
    /// Note: `Operation::callbacks` in [`openapiv3`] uses an inline-only
    /// [`Callback`] (no `$ref`), so this field stores resolved callbacks
    /// directly.
    pub callbacks: IndexMap<String, ResolvedCallback>,
    pub deprecated: bool,
    pub security: Option<Vec<SecurityRequirement>>,
    pub servers: Vec<Server>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Responses`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedResponses {
    pub default: Option<ResolvedRefOr<ResolvedResponse>>,
    pub responses: IndexMap<StatusCode, ResolvedRefOr<ResolvedResponse>>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Response`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedResponse {
    pub description: String,
    pub headers: IndexMap<String, ResolvedRefOr<ResolvedHeader>>,
    pub content: IndexMap<String, ResolvedMediaType>,
    pub links: IndexMap<String, ResolvedRefOr<Link>>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::RequestBody`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedRequestBody {
    pub description: Option<String>,
    pub content: IndexMap<String, ResolvedMediaType>,
    pub required: bool,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::MediaType`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedMediaType {
    pub schema: Option<ResolvedRefOr<ResolvedSchema>>,
    pub example: Option<serde_json::Value>,
    pub examples: IndexMap<String, ResolvedRefOr<Example>>,
    pub encoding: IndexMap<String, ResolvedEncoding>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Encoding`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedEncoding {
    pub content_type: Option<String>,
    pub headers: IndexMap<String, ResolvedRefOr<ResolvedHeader>>,
    pub style: Option<QueryStyle>,
    pub explode: bool,
    pub allow_reserved: bool,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Header`].
#[derive(Debug, Clone)]
pub struct ResolvedHeader {
    pub description: Option<String>,
    pub style: HeaderStyle,
    pub required: bool,
    pub deprecated: Option<bool>,
    pub format: ResolvedParameterSchemaOrContent,
    pub example: Option<serde_json::Value>,
    pub examples: IndexMap<String, ResolvedRefOr<Example>>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::Parameter`].
#[derive(Debug, Clone)]
pub enum ResolvedParameter {
    Query {
        parameter_data: ResolvedParameterData,
        allow_reserved: bool,
        style: QueryStyle,
        allow_empty_value: Option<bool>,
    },
    Header {
        parameter_data: ResolvedParameterData,
        style: HeaderStyle,
    },
    Path {
        parameter_data: ResolvedParameterData,
        style: PathStyle,
    },
    Cookie {
        parameter_data: ResolvedParameterData,
        style: CookieStyle,
    },
}

impl ResolvedParameter {
    /// Borrows the [`ResolvedParameterData`] for any variant.
    pub fn parameter_data(&self) -> &ResolvedParameterData {
        match self {
            Self::Query { parameter_data, .. }
            | Self::Header { parameter_data, .. }
            | Self::Path { parameter_data, .. }
            | Self::Cookie { parameter_data, .. } => parameter_data,
        }
    }
}

/// Resolved counterpart of [`openapiv3::ParameterData`].
#[derive(Debug, Clone)]
pub struct ResolvedParameterData {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    pub deprecated: Option<bool>,
    pub format: ResolvedParameterSchemaOrContent,
    pub example: Option<serde_json::Value>,
    pub examples: IndexMap<String, ResolvedRefOr<Example>>,
    pub explode: Option<bool>,
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolved counterpart of [`openapiv3::ParameterSchemaOrContent`].
#[derive(Debug, Clone)]
pub enum ResolvedParameterSchemaOrContent {
    Schema(ResolvedRefOr<ResolvedSchema>),
    Content(IndexMap<String, ResolvedMediaType>),
}

/// Resolved counterpart of [`openapiv3::Callback`]. A callback is just a map
/// of expressions to path items.
pub type ResolvedCallback = IndexMap<String, ResolvedPathItem>;

/// Resolved counterpart of [`openapiv3::Schema`].
#[derive(Debug, Clone)]
pub struct ResolvedSchema {
    pub schema_data: SchemaData,
    pub schema_kind: ResolvedSchemaKind,
}

/// Resolved counterpart of [`openapiv3::SchemaKind`].
// `Any` carries the long inline-schema struct; the original openapiv3
// SchemaKind has the same shape, so we mirror it rather than introducing a
// gratuitous Box that diverges from the source enum.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ResolvedSchemaKind {
    Type(ResolvedType),
    OneOf {
        one_of: Vec<ResolvedRefOr<ResolvedSchema>>,
    },
    AllOf {
        all_of: Vec<ResolvedRefOr<ResolvedSchema>>,
    },
    AnyOf {
        any_of: Vec<ResolvedRefOr<ResolvedSchema>>,
    },
    Not {
        not: ResolvedRefOr<ResolvedSchema>,
    },
    Any(ResolvedAnySchema),
}

/// Resolved counterpart of [`openapiv3::Type`].
#[derive(Debug, Clone)]
pub enum ResolvedType {
    String(StringType),
    Number(NumberType),
    Integer(IntegerType),
    Object(ResolvedObjectType),
    Array(ResolvedArrayType),
    Boolean(BooleanType),
}

/// Resolved counterpart of [`openapiv3::ObjectType`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedObjectType {
    pub properties: IndexMap<String, ResolvedRefOr<ResolvedSchema>>,
    pub required: Vec<String>,
    pub additional_properties: Option<ResolvedAdditionalProperties>,
    pub min_properties: Option<usize>,
    pub max_properties: Option<usize>,
}

/// Resolved counterpart of [`openapiv3::ArrayType`].
#[derive(Debug, Clone)]
pub struct ResolvedArrayType {
    pub items: Option<ResolvedRefOr<ResolvedSchema>>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub unique_items: bool,
}

/// Resolved counterpart of [`openapiv3::AdditionalProperties`].
#[derive(Debug, Clone)]
pub enum ResolvedAdditionalProperties {
    Any(bool),
    Schema(ResolvedRefOr<ResolvedSchema>),
}

/// Resolved counterpart of [`openapiv3::AnySchema`].
#[derive(Debug, Clone, Default)]
pub struct ResolvedAnySchema {
    pub typ: Option<String>,
    pub pattern: Option<String>,
    pub multiple_of: Option<f64>,
    pub exclusive_minimum: Option<bool>,
    pub exclusive_maximum: Option<bool>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub properties: IndexMap<String, ResolvedRefOr<ResolvedSchema>>,
    pub required: Vec<String>,
    pub additional_properties: Option<ResolvedAdditionalProperties>,
    pub min_properties: Option<usize>,
    pub max_properties: Option<usize>,
    pub items: Option<ResolvedRefOr<ResolvedSchema>>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub unique_items: Option<bool>,
    pub enumeration: Vec<serde_json::Value>,
    pub format: Option<String>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub one_of: Vec<ResolvedRefOr<ResolvedSchema>>,
    pub all_of: Vec<ResolvedRefOr<ResolvedSchema>>,
    pub any_of: Vec<ResolvedRefOr<ResolvedSchema>>,
    pub not: Option<ResolvedRefOr<ResolvedSchema>>,
}
