use super::discriminator::resolve_discriminator;
use super::resolver::{Resolvable, Resolver};
use super::{NestedSchema, ResolvedAnySchema, ResolvedDiscriminator, ResolvedType};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{AdditionalProperties, ExternalDocumentation, Schema, SchemaData, SchemaKind};

/// [`Schema`] with every `$ref` replaced by the schema it named.
#[derive(Debug, PartialEq)]
pub struct ResolvedSchema {
    /// See [`Schema::schema_data`].
    pub schema_data: ResolvedSchemaData,
    /// See [`Schema::schema_kind`].
    pub schema_kind: ResolvedSchemaKind,
}

/// [`SchemaData`] with the discriminator's mapping targets resolved.
///
/// `Default` exists for the placeholder a schema's slot holds while that
/// schema is still being resolved; it has no meaning of its own.
#[derive(Debug, Default, PartialEq)]
pub struct ResolvedSchemaData {
    /// See [`SchemaData::nullable`].
    pub nullable: bool,
    /// See [`SchemaData::read_only`].
    ///
    /// Mirrors upstream as-is: the specification forbids a schema being both
    /// read-only and write-only, which neither `openapiv3` nor this crate
    /// enforces.
    pub read_only: bool,
    /// See [`SchemaData::write_only`], and the note on [`Self::read_only`].
    pub write_only: bool,
    /// See [`SchemaData::deprecated`].
    pub deprecated: bool,
    /// See [`SchemaData::external_docs`].
    pub external_docs: Option<ExternalDocumentation>,
    /// See [`SchemaData::example`].
    pub example: Option<serde_json::Value>,
    /// See [`SchemaData::title`].
    pub title: Option<String>,
    /// See [`SchemaData::description`].
    pub description: Option<String>,
    /// See [`SchemaData::discriminator`].
    pub discriminator: Option<ResolvedDiscriminator>,
    /// See [`SchemaData::default`].
    pub default: Option<serde_json::Value>,
    /// See [`SchemaData::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`SchemaKind`] with every `$ref` replaced by the schema it named.
#[derive(Debug, PartialEq)]
pub enum ResolvedSchemaKind {
    /// See [`SchemaKind::Type`].
    Type(ResolvedType),
    /// See [`SchemaKind::OneOf`].
    OneOf {
        /// The alternatives, exactly one of which must match.
        one_of: Vec<NestedSchema>,
    },
    /// See [`SchemaKind::AllOf`].
    AllOf {
        /// The schemas that must all match.
        all_of: Vec<NestedSchema>,
    },
    /// See [`SchemaKind::AnyOf`].
    AnyOf {
        /// The alternatives, at least one of which must match.
        any_of: Vec<NestedSchema>,
    },
    /// See [`SchemaKind::Not`].
    Not {
        /// The schema that must not match.
        not: NestedSchema,
    },
    /// See [`SchemaKind::Any`].
    Any(Box<ResolvedAnySchema>),
}

/// [`AdditionalProperties`] with a `$ref` replaced by the schema it named.
#[derive(Debug, PartialEq)]
pub enum ResolvedAdditionalProperties {
    /// See [`AdditionalProperties::Any`].
    Any(bool),
    /// See [`AdditionalProperties::Schema`].
    Schema(NestedSchema),
}

impl Resolvable for Schema {
    type Resolved = ResolvedSchema;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        // Kind first, and the order is load-bearing: a discriminator mapping
        // on a `oneOf`/`anyOf` reuses the alternatives' edges, so those have
        // to exist already. It also decides which edge of a cycle that runs
        // through both is the recursive one; see `NestedSchema`.
        let schema_kind = self.schema_kind.resolve_inline(cx)?;
        let discriminator = self
            .schema_data
            .discriminator
            .as_ref()
            .map(|discriminator| {
                resolve_discriminator(cx, discriminator, &self.schema_kind, &schema_kind)
            })
            .transpose()?;
        Ok(ResolvedSchema {
            schema_data: resolve_schema_data(&self.schema_data, discriminator),
            schema_kind,
        })
    }
}

impl Resolvable for SchemaKind {
    type Resolved = ResolvedSchemaKind;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(match self {
            Self::Type(kind) => ResolvedSchemaKind::Type(kind.resolve_inline(cx)?),
            Self::OneOf { one_of } => ResolvedSchemaKind::OneOf {
                one_of: cx.nested_vec(one_of)?,
            },
            Self::AllOf { all_of } => ResolvedSchemaKind::AllOf {
                all_of: cx.nested_vec(all_of)?,
            },
            Self::AnyOf { any_of } => ResolvedSchemaKind::AnyOf {
                any_of: cx.nested_vec(any_of)?,
            },
            Self::Not { not } => ResolvedSchemaKind::Not {
                not: cx.nested(not)?,
            },
            Self::Any(any) => ResolvedSchemaKind::Any(Box::new(any.resolve_inline(cx)?)),
        })
    }
}

impl Resolvable for AdditionalProperties {
    type Resolved = ResolvedAdditionalProperties;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(match self {
            Self::Any(allowed) => ResolvedAdditionalProperties::Any(*allowed),
            Self::Schema(schema) => ResolvedAdditionalProperties::Schema(cx.nested(schema)?),
        })
    }
}

/// Copies `data` over, with its discriminator already resolved.
fn resolve_schema_data(
    data: &SchemaData,
    discriminator: Option<ResolvedDiscriminator>,
) -> ResolvedSchemaData {
    // Destructured without `..` so that a field added upstream fails to
    // compile here instead of being quietly dropped from the mirror. The
    // original discriminator is bound so it counts as handled, the resolved
    // one is what goes in.
    let SchemaData {
        nullable,
        read_only,
        write_only,
        deprecated,
        external_docs,
        example,
        title,
        description,
        discriminator: _,
        default,
        extensions,
    } = data;
    ResolvedSchemaData {
        nullable: *nullable,
        read_only: *read_only,
        write_only: *write_only,
        deprecated: *deprecated,
        external_docs: external_docs.clone(),
        example: example.clone(),
        title: title.clone(),
        description: description.clone(),
        discriminator,
        default: default.clone(),
        extensions: extensions.clone(),
    }
}
