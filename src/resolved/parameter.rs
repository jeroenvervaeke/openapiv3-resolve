use super::resolver::{Resolvable, Resolver};
use super::{ResolvedMediaType, ResolvedSchema, Shared};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{
    CookieStyle, Example, Header, HeaderStyle, Parameter, ParameterData, ParameterSchemaOrContent,
    PathStyle, QueryStyle,
};

/// [`Parameter`] with every `$ref` replaced by the item it named.
#[derive(Debug, PartialEq)]
pub enum ResolvedParameter {
    /// See [`Parameter::Query`].
    Query {
        /// The parameter itself.
        parameter_data: ResolvedParameterData,
        /// Whether reserved characters may appear unencoded.
        allow_reserved: bool,
        /// How the value is serialised.
        style: QueryStyle,
        /// Whether an empty value is allowed.
        allow_empty_value: Option<bool>,
    },
    /// See [`Parameter::Header`].
    Header {
        /// The parameter itself.
        parameter_data: ResolvedParameterData,
        /// How the value is serialised.
        style: HeaderStyle,
    },
    /// See [`Parameter::Path`].
    Path {
        /// The parameter itself.
        parameter_data: ResolvedParameterData,
        /// How the value is serialised.
        style: PathStyle,
    },
    /// See [`Parameter::Cookie`].
    Cookie {
        /// The parameter itself.
        parameter_data: ResolvedParameterData,
        /// How the value is serialised.
        style: CookieStyle,
    },
}

impl ResolvedParameter {
    /// The data shared by every kind of parameter.
    pub fn parameter_data(&self) -> &ResolvedParameterData {
        match self {
            Self::Query { parameter_data, .. }
            | Self::Header { parameter_data, .. }
            | Self::Path { parameter_data, .. }
            | Self::Cookie { parameter_data, .. } => parameter_data,
        }
    }
}

/// [`ParameterData`] with every `$ref` replaced by the item it named.
#[derive(Debug, PartialEq)]
pub struct ResolvedParameterData {
    /// See [`ParameterData::name`].
    pub name: String,
    /// See [`ParameterData::description`].
    pub description: Option<String>,
    /// See [`ParameterData::required`].
    pub required: bool,
    /// See [`ParameterData::deprecated`].
    pub deprecated: Option<bool>,
    /// See [`ParameterData::format`].
    pub format: ResolvedParameterSchemaOrContent,
    /// See [`ParameterData::example`].
    pub example: Option<serde_json::Value>,
    /// See [`ParameterData::examples`].
    pub examples: IndexMap<String, Shared<Example>>,
    /// See [`ParameterData::explode`].
    pub explode: Option<bool>,
    /// See [`ParameterData::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`ParameterSchemaOrContent`] with every `$ref` replaced by the item it named.
#[derive(Debug, PartialEq)]
pub enum ResolvedParameterSchemaOrContent {
    /// See [`ParameterSchemaOrContent::Schema`].
    Schema(Shared<ResolvedSchema>),
    /// See [`ParameterSchemaOrContent::Content`].
    Content(IndexMap<String, ResolvedMediaType>),
}

/// [`Header`] with every `$ref` replaced by the item it named.
#[derive(Debug, PartialEq)]
pub struct ResolvedHeader {
    /// See [`Header::description`].
    pub description: Option<String>,
    /// See [`Header::style`].
    pub style: HeaderStyle,
    /// See [`Header::required`].
    pub required: bool,
    /// See [`Header::deprecated`].
    pub deprecated: Option<bool>,
    /// See [`Header::format`].
    pub format: ResolvedParameterSchemaOrContent,
    /// See [`Header::example`].
    pub example: Option<serde_json::Value>,
    /// See [`Header::examples`].
    pub examples: IndexMap<String, Shared<Example>>,
    /// See [`Header::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

impl Resolvable for Parameter {
    type Resolved = ResolvedParameter;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(match self {
            Self::Query {
                parameter_data,
                allow_reserved,
                style,
                allow_empty_value,
            } => ResolvedParameter::Query {
                parameter_data: parameter_data.resolve_inline(cx)?,
                allow_reserved: *allow_reserved,
                style: style.clone(),
                allow_empty_value: *allow_empty_value,
            },
            Self::Header {
                parameter_data,
                style,
            } => ResolvedParameter::Header {
                parameter_data: parameter_data.resolve_inline(cx)?,
                style: style.clone(),
            },
            Self::Path {
                parameter_data,
                style,
            } => ResolvedParameter::Path {
                parameter_data: parameter_data.resolve_inline(cx)?,
                style: style.clone(),
            },
            Self::Cookie {
                parameter_data,
                style,
            } => ResolvedParameter::Cookie {
                parameter_data: parameter_data.resolve_inline(cx)?,
                style: style.clone(),
            },
        })
    }
}

impl Resolvable for ParameterData {
    type Resolved = ResolvedParameterData;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedParameterData {
            name: self.name.clone(),
            description: self.description.clone(),
            required: self.required,
            deprecated: self.deprecated,
            format: self.format.resolve_inline(cx)?,
            example: self.example.clone(),
            examples: cx.ref_or_map(&self.examples)?,
            explode: self.explode,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for ParameterSchemaOrContent {
    type Resolved = ResolvedParameterSchemaOrContent;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(match self {
            Self::Schema(schema) => ResolvedParameterSchemaOrContent::Schema(cx.ref_or(schema)?),
            Self::Content(content) => {
                ResolvedParameterSchemaOrContent::Content(super::resolve_map(content, cx)?)
            }
        })
    }
}

impl Resolvable for Header {
    type Resolved = ResolvedHeader;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedHeader {
            description: self.description.clone(),
            style: self.style.clone(),
            required: self.required,
            deprecated: self.deprecated,
            format: self.format.resolve_inline(cx)?,
            example: self.example.clone(),
            examples: cx.ref_or_map(&self.examples)?,
            extensions: self.extensions.clone(),
        })
    }
}
