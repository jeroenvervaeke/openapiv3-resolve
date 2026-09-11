use super::resolver::{Resolvable, Resolver};
use super::{ResolvedAnySchema, ResolvedType};
use crate::ResolveError;
use openapiv3::{AdditionalProperties, Schema, SchemaData, SchemaKind};
use std::sync::Arc;

/// [`Schema`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSchema {
    /// See [`Schema::schema_data`].
    pub schema_data: SchemaData,
    /// See [`Schema::schema_kind`].
    pub schema_kind: ResolvedSchemaKind,
}

/// [`SchemaKind`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedSchemaKind {
    /// See [`SchemaKind::Type`].
    Type(ResolvedType),
    /// See [`SchemaKind::OneOf`].
    OneOf {
        /// The alternatives, exactly one of which must match.
        one_of: Vec<Arc<ResolvedSchema>>,
    },
    /// See [`SchemaKind::AllOf`].
    AllOf {
        /// The schemas that must all match.
        all_of: Vec<Arc<ResolvedSchema>>,
    },
    /// See [`SchemaKind::AnyOf`].
    AnyOf {
        /// The alternatives, at least one of which must match.
        any_of: Vec<Arc<ResolvedSchema>>,
    },
    /// See [`SchemaKind::Not`].
    Not {
        /// The schema that must not match.
        not: Arc<ResolvedSchema>,
    },
    /// See [`SchemaKind::Any`].
    Any(Box<ResolvedAnySchema>),
}

/// [`AdditionalProperties`] with a `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedAdditionalProperties {
    /// See [`AdditionalProperties::Any`].
    Any(bool),
    /// See [`AdditionalProperties::Schema`].
    Schema(Arc<ResolvedSchema>),
}

impl Resolvable for Schema {
    type Resolved = ResolvedSchema;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedSchema {
            schema_data: self.schema_data.clone(),
            schema_kind: self.schema_kind.resolve_inline(cx)?,
        })
    }
}

impl Resolvable for SchemaKind {
    type Resolved = ResolvedSchemaKind;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(match self {
            Self::Type(kind) => ResolvedSchemaKind::Type(kind.resolve_inline(cx)?),
            Self::OneOf { one_of } => ResolvedSchemaKind::OneOf {
                one_of: cx.ref_or_vec(one_of)?,
            },
            Self::AllOf { all_of } => ResolvedSchemaKind::AllOf {
                all_of: cx.ref_or_vec(all_of)?,
            },
            Self::AnyOf { any_of } => ResolvedSchemaKind::AnyOf {
                any_of: cx.ref_or_vec(any_of)?,
            },
            Self::Not { not } => ResolvedSchemaKind::Not {
                not: cx.ref_or(not)?,
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
            Self::Schema(schema) => ResolvedAdditionalProperties::Schema(cx.ref_or(schema)?),
        })
    }
}
