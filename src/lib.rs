#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

pub use indexmap;
pub use openapiv3;

mod component;
mod error;
mod reference;
mod resolved;

pub use component::Component;
pub use error::ResolveError;
pub use reference::Section;
pub use resolved::*;

use openapiv3::{OpenAPI, ReferenceOr};
use reference::ComponentRef;
use std::borrow::Cow;

/// How many `$ref` hops a single resolution may follow.
///
/// A cyclic document has no inline item at the end of the chain, so without a
/// bound the walk never terminates. Chains this long do not occur in practice.
pub const MAX_REFERENCE_HOPS: usize = 100;

/// Resolves a `$ref` pointer against a whole document.
pub trait Resolve {
    /// Resolves `reference` to a component, following chains of `$ref`s.
    ///
    /// The type argument decides which section is searched, so it usually has
    /// to be named: `openapi.resolve_ref::<Schema>("#/components/schemas/Pet")`.
    fn resolve_ref<'a, T: Component>(&'a self, reference: &str) -> Result<&'a T, ResolveError>;
}

/// Resolves a `ReferenceOr<T>` (or its boxed variant) to the item it denotes.
pub trait ResolveWithOpenAPI<T> {
    /// Returns the inline item, or the item the `$ref` points at.
    ///
    /// The result borrows from whichever of the two arguments it came from, so
    /// its lifetime is the shorter of them: resolving out of a temporary
    /// `ReferenceOr` yields a borrow that cannot outlive that temporary, even
    /// when the value in fact came from `openapi`.
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Result<&'a T, ResolveError>;
}

/// Resolves an optional `ReferenceOr<T>` field.
///
/// Kept separate from [`ResolveWithOpenAPI`] so that an absent field — which
/// is valid — stays distinguishable from a broken reference.
pub trait ResolveOptionalWithOpenAPI<T> {
    /// Returns `Ok(None)` if the field is absent, and an error only if a
    /// reference that *is* present cannot be resolved.
    fn resolve_optional<'a>(&'a self, openapi: &'a OpenAPI) -> Result<Option<&'a T>, ResolveError>;
}

impl Resolve for OpenAPI {
    fn resolve_ref<'a, T: Component>(&'a self, reference: &str) -> Result<&'a T, ResolveError> {
        walk(self, reference).map(|(_, item)| item)
    }
}

impl<T: Component> ResolveWithOpenAPI<T> for ReferenceOr<T> {
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Result<&'a T, ResolveError> {
        match self {
            ReferenceOr::Item(item) => Ok(item),
            ReferenceOr::Reference { reference } => openapi.resolve_ref(reference),
        }
    }
}

impl<T: Component> ResolveWithOpenAPI<T> for ReferenceOr<Box<T>> {
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Result<&'a T, ResolveError> {
        match self {
            ReferenceOr::Item(item) => Ok(item),
            ReferenceOr::Reference { reference } => openapi.resolve_ref(reference),
        }
    }
}

impl<T, R> ResolveOptionalWithOpenAPI<T> for Option<R>
where
    R: ResolveWithOpenAPI<T>,
{
    fn resolve_optional<'a>(&'a self, openapi: &'a OpenAPI) -> Result<Option<&'a T>, ResolveError> {
        match self {
            Some(value) => value.resolve(openapi).map(Some),
            None => Ok(None),
        }
    }
}

/// Follows `reference` to the inline item at the end of its chain, returning
/// the name that item is stored under alongside it.
///
/// The name is what distinguishes this from [`Resolve::resolve_ref`]: a full
/// resolution has to share the result between every reference to the same
/// component, and the final name is the key to share it under.
pub(crate) fn walk<'a, 'p, T: Component>(
    openapi: &'a OpenAPI,
    reference: &'p str,
) -> Result<(Cow<'p, str>, &'a T), ResolveError>
where
    'a: 'p,
{
    // Iterative on purpose: recursing here let a cyclic document overflow
    // the stack, which aborts the process instead of returning an error.
    let mut pointer: &'p str = reference;
    let mut hops: usize = 0;

    loop {
        let (parsed, entry) = lookup::<T>(openapi, pointer)?;
        match entry {
            ReferenceOr::Item(item) => return Ok((parsed.name, item)),
            ReferenceOr::Reference { reference: next } => {
                hops += 1;
                if hops > MAX_REFERENCE_HOPS {
                    return Err(ResolveError::ReferenceChainTooLong {
                        reference: reference.to_owned(),
                        last: next.clone(),
                        max_hops: MAX_REFERENCE_HOPS,
                    });
                }
                pointer = next.as_str();
            }
        }
    }
}

/// Looks up one hop: parses the pointer and reads the entry, without following it.
fn lookup<'a, 'p, T: Component>(
    openapi: &'a OpenAPI,
    pointer: &'p str,
) -> Result<(ComponentRef<'p>, &'a ReferenceOr<T>), ResolveError> {
    let parsed = ComponentRef::parse(pointer)?;

    if parsed.section != T::SECTION {
        return Err(ResolveError::SectionMismatch {
            expected: T::SECTION,
            found: parsed.section,
        });
    }

    let section = T::section(openapi).ok_or(ResolveError::ComponentsMissing {
        section: T::SECTION,
    })?;

    match section.get(parsed.name.as_ref()) {
        Some(entry) => Ok((parsed, entry)),
        None => Err(ResolveError::NotFound {
            section: T::SECTION,
            name: parsed.name.into_owned(),
        }),
    }
}
