use super::resolver::Resolver;
use super::{NestedSchema, ResolvedSchemaKind};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{Discriminator, ReferenceOr, Schema, SchemaKind};

/// [`Discriminator`] with every mapping value replaced by the schema it named.
///
/// A mapping value is either a `$ref` or a bare schema name. A value
/// containing `#` is followed as a `$ref` (`#/components/schemas/Cat`, or a
/// reference into another document, which fails exactly as the equivalent
/// `$ref` would); any other value is the name of a schema under
/// `#/components/schemas`, looked up as written.
///
/// On a `oneOf` or `anyOf` schema every mapping value must name one of the
/// alternatives, and the entry is that alternative's very edge; a value that
/// names any other schema fails with
/// [`ResolveError::DiscriminatorMappingMismatch`]. Inline alternatives cannot
/// be named, as the specification says. A schema that is not a `oneOf` or
/// `anyOf` (the `allOf` inheritance pattern, where the mapping lives on the
/// parent) has no alternatives to check against, so its mapping values are
/// resolved like any other `$ref`. Entries are resolved in document order,
/// and the first one that fails is the error reported.
#[derive(Debug, PartialEq)]
pub struct ResolvedDiscriminator {
    /// See [`Discriminator::property_name`].
    pub property_name: String,
    /// See [`Discriminator::mapping`].
    pub mapping: IndexMap<String, NestedSchema>,
    /// See [`Discriminator::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// Resolves `discriminator`, which sits on a schema whose kind has already
/// been resolved to `resolved` from `kind`.
pub(super) fn resolve_discriminator(
    cx: &mut Resolver<'_>,
    discriminator: &Discriminator,
    kind: &SchemaKind,
    resolved: &ResolvedSchemaKind,
) -> Result<ResolvedDiscriminator, ResolveError> {
    // Destructured without `..` so that a field added upstream fails to
    // compile here instead of being quietly dropped from the mirror.
    let Discriminator {
        property_name,
        mapping,
        extensions,
    } = discriminator;
    let alternatives = alternatives(cx, kind, resolved)?;
    let mapping = mapping
        .iter()
        .map(|(value, target)| {
            let target = MappingTarget::classify(target);
            let edge = match &alternatives {
                Some(alternatives) => {
                    let schema = cx.schema_name(target)?;
                    alternatives
                        .iter()
                        .find(|(name, _)| *name == schema)
                        .map(|(_, edge)| edge.duplicate())
                        .ok_or_else(|| ResolveError::DiscriminatorMappingMismatch {
                            property_name: property_name.clone(),
                            value: value.clone(),
                            schema,
                        })?
                }
                None => cx.nested_target(target)?,
            };
            Ok((value.clone(), edge))
        })
        .collect::<Result<_, _>>()?;
    Ok(ResolvedDiscriminator {
        property_name: property_name.clone(),
        mapping,
        extensions: extensions.clone(),
    })
}

/// How one discriminator mapping value names its schema.
///
/// The specification allows either a schema name or a reference and does not
/// say how to tell them apart. A component name can never contain `#`, so a
/// `#` marks a reference; everything else is taken as a name, which keeps
/// names outside the specification's grammar (`a/b`) reachable all the same.
#[derive(Clone, Copy)]
pub(super) enum MappingTarget<'a> {
    /// A `$ref`, local or into another document.
    Reference(&'a str),
    /// The name of a schema under `#/components/schemas`.
    Name(&'a str),
}

impl<'a> MappingTarget<'a> {
    fn classify(target: &'a str) -> Self {
        if target.contains('#') {
            Self::Reference(target)
        } else {
            Self::Name(target)
        }
    }
}

/// The alternatives a mapping value may name, as the name each `$ref` finally
/// resolves to paired with its resolved edge; `None` when the schema is not a
/// `oneOf` or `anyOf` and so has no alternatives at all.
fn alternatives<'a>(
    cx: &Resolver<'_>,
    kind: &'a SchemaKind,
    resolved: &'a ResolvedSchemaKind,
) -> Result<Option<Vec<(String, &'a NestedSchema)>>, ResolveError> {
    let (Some(entries), Some(edges)) = (entries_of(kind), edges_of(resolved)) else {
        return Ok(None);
    };
    entries
        .into_iter()
        .zip(edges)
        // Inline alternatives have no name a mapping value could use.
        .filter_map(|(entry, edge)| match entry {
            ReferenceOr::Item(_) => None,
            ReferenceOr::Reference { reference } => Some((reference, edge)),
        })
        .map(|(reference, edge)| Ok((cx.schema_name(MappingTarget::Reference(reference))?, edge)))
        .collect::<Result<_, _>>()
        .map(Some)
}

fn entries_of(kind: &SchemaKind) -> Option<Vec<&ReferenceOr<Schema>>> {
    match kind {
        SchemaKind::OneOf { one_of } => Some(one_of.iter().collect()),
        SchemaKind::AnyOf { any_of } => Some(any_of.iter().collect()),
        SchemaKind::Any(any) if !(any.one_of.is_empty() && any.any_of.is_empty()) => {
            Some(any.one_of.iter().chain(&any.any_of).collect())
        }
        SchemaKind::Any(_)
        | SchemaKind::Type(_)
        | SchemaKind::AllOf { .. }
        | SchemaKind::Not { .. } => None,
    }
}

fn edges_of(resolved: &ResolvedSchemaKind) -> Option<Vec<&NestedSchema>> {
    match resolved {
        ResolvedSchemaKind::OneOf { one_of } => Some(one_of.iter().collect()),
        ResolvedSchemaKind::AnyOf { any_of } => Some(any_of.iter().collect()),
        ResolvedSchemaKind::Any(any) if !(any.one_of.is_empty() && any.any_of.is_empty()) => {
            Some(any.one_of.iter().chain(&any.any_of).collect())
        }
        ResolvedSchemaKind::Any(_)
        | ResolvedSchemaKind::Type(_)
        | ResolvedSchemaKind::AllOf { .. }
        | ResolvedSchemaKind::Not { .. } => None,
    }
}
