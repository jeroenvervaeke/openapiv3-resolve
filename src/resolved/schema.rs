use super::resolver::{Resolvable, Resolver};
use super::{ResolvedAnySchema, ResolvedType};
use crate::ResolveError;
use openapiv3::{AdditionalProperties, Schema, SchemaData, SchemaKind};
use std::sync::{Arc, Weak};

/// [`Schema`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSchema {
    /// See [`Schema::schema_data`].
    pub schema_data: SchemaData,
    /// See [`Schema::schema_kind`].
    pub schema_kind: ResolvedSchemaKind,
}

/// A schema nested inside another schema.
///
/// Nearly always [`Schema`](Self::Schema). The exception is a `$ref` that
/// points back at a schema which contains it (a tree node whose children
/// are nodes, say): that edge is [`Recursive`](Self::Recursive) and holds a
/// [`Weak`], because a cycle of `Arc`s would never be freed. Which edge of a
/// cycle is the recursive one is decided by document order: it is the first
/// `$ref` met, walking `components` then `paths`, that closes the cycle.
///
/// A recursive edge upgrades for as long as the schema it points at is alive,
/// which is guaranteed while the [`ResolvedOpenAPI`](crate::ResolvedOpenAPI)
/// is, since `components` holds every schema a `$ref` can name.
///
/// Two recursive edges are equal when they point at the same allocation, so
/// comparing two independently resolved documents that contain a cycle
/// reports them as different.
#[derive(Debug, Clone)]
pub enum NestedSchema {
    /// The nested schema itself, inline or shared with other references to it.
    Schema(Arc<ResolvedSchema>),
    /// A schema that contains this edge, so it is still under construction.
    Recursive(Weak<ResolvedSchema>),
}

impl NestedSchema {
    /// The nested schema, or `None` if this is a recursive edge whose target
    /// has been dropped.
    pub fn upgrade(&self) -> Option<Arc<ResolvedSchema>> {
        match self {
            Self::Schema(schema) => Some(Arc::clone(schema)),
            Self::Recursive(schema) => schema.upgrade(),
        }
    }

    /// Whether this edge points back at a schema that contains it.
    pub fn is_recursive(&self) -> bool {
        matches!(self, Self::Recursive(_))
    }
}

impl PartialEq for NestedSchema {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Schema(left), Self::Schema(right)) => left == right,
            // Comparing contents here would never terminate.
            (Self::Recursive(left), Self::Recursive(right)) => Weak::ptr_eq(left, right),
            (Self::Schema(_), Self::Recursive(_)) | (Self::Recursive(_), Self::Schema(_)) => false,
        }
    }
}

/// [`SchemaKind`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedAdditionalProperties {
    /// See [`AdditionalProperties::Any`].
    Any(bool),
    /// See [`AdditionalProperties::Schema`].
    Schema(NestedSchema),
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
