use crate::reference::escape;
use crate::{walk, Component, ResolveError};
use indexmap::IndexMap;
use openapiv3::{OpenAPI, ReferenceOr};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

/// A value that can be rebuilt with every `$ref` inside it followed.
pub(super) trait Resolvable {
    /// The reference-free counterpart of `Self`.
    type Resolved;

    /// Rebuilds `self`, resolving every reference it contains through `cx`.
    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError>;
}

/// A [`Component`] whose resolved form is shared between all its references.
pub(super) trait Cached: Resolvable + Component {
    /// The cache holding this type's resolved components.
    fn cache(caches: &mut Caches) -> &mut Cache<Self::Resolved>;
}

/// The state of one full resolution: the document plus everything resolved so far.
pub(super) struct Resolver<'a> {
    openapi: &'a OpenAPI,
    caches: Caches,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(openapi: &'a OpenAPI) -> Self {
        Self {
            openapi,
            caches: Caches::default(),
        }
    }

    /// Resolves an inline item; the result is owned by its one reference site.
    pub(super) fn inline<T: Resolvable>(
        &mut self,
        item: &T,
    ) -> Result<Arc<T::Resolved>, ResolveError> {
        item.resolve_inline(self).map(Arc::new)
    }

    /// Resolves a `$ref`, sharing the result with every other `$ref` to the
    /// same component.
    pub(super) fn reference<T: Cached>(
        &mut self,
        reference: &str,
    ) -> Result<Arc<T::Resolved>, ResolveError> {
        let (name, item) = walk::<T>(self.openapi, reference)?;
        self.named::<T>(&name, item, reference)
    }

    pub(super) fn ref_or<T: Cached>(
        &mut self,
        entry: &ReferenceOr<T>,
    ) -> Result<Arc<T::Resolved>, ResolveError> {
        match entry {
            ReferenceOr::Item(item) => self.inline(item),
            ReferenceOr::Reference { reference } => self.reference::<T>(reference),
        }
    }

    pub(super) fn ref_or_boxed<T: Cached>(
        &mut self,
        entry: &ReferenceOr<Box<T>>,
    ) -> Result<Arc<T::Resolved>, ResolveError> {
        match entry {
            ReferenceOr::Item(item) => self.inline(item.as_ref()),
            ReferenceOr::Reference { reference } => self.reference::<T>(reference),
        }
    }

    pub(super) fn ref_or_vec<T: Cached>(
        &mut self,
        entries: &[ReferenceOr<T>],
    ) -> Result<Vec<Arc<T::Resolved>>, ResolveError> {
        entries.iter().map(|entry| self.ref_or(entry)).collect()
    }

    pub(super) fn ref_or_map<K, T>(
        &mut self,
        entries: &IndexMap<K, ReferenceOr<T>>,
    ) -> Result<IndexMap<K, Arc<T::Resolved>>, ResolveError>
    where
        K: Clone + Hash + Eq,
        T: Cached,
    {
        entries
            .iter()
            .map(|(key, entry)| Ok((key.clone(), self.ref_or(entry)?)))
            .collect()
    }

    pub(super) fn ref_or_boxed_map<T: Cached>(
        &mut self,
        entries: &IndexMap<String, ReferenceOr<Box<T>>>,
    ) -> Result<IndexMap<String, Arc<T::Resolved>>, ResolveError> {
        entries
            .iter()
            .map(|(key, entry)| Ok((key.clone(), self.ref_or_boxed(entry)?)))
            .collect()
    }

    /// Resolves a whole section of the document, e.g. `#/components/schemas`
    /// or `#/paths`, so that its entries share their `Arc`s with the
    /// references that point at them.
    pub(super) fn section<T: Cached>(
        &mut self,
        entries: &IndexMap<String, ReferenceOr<T>>,
    ) -> Result<IndexMap<String, Arc<T::Resolved>>, ResolveError> {
        entries
            .iter()
            .map(|(name, entry)| {
                let resolved = match entry {
                    ReferenceOr::Item(item) => {
                        let pointer = format!("#/{}/{}", T::SECTION, escape(name));
                        self.named::<T>(name, item, &pointer)?
                    }
                    ReferenceOr::Reference { reference } => self.reference::<T>(reference)?,
                };
                Ok((name.clone(), resolved))
            })
            .collect()
    }

    /// Resolves the component stored under `name`, or returns the `Arc` it
    /// already resolved to. `reference` is only for the error message.
    fn named<T: Cached>(
        &mut self,
        name: &str,
        item: &T,
        reference: &str,
    ) -> Result<Arc<T::Resolved>, ResolveError> {
        match T::cache(&mut self.caches).0.get(name) {
            Some(Slot::Done(resolved)) => return Ok(Arc::clone(resolved)),
            Some(Slot::InProgress) => {
                return Err(ResolveError::CyclicReference {
                    reference: reference.to_owned(),
                })
            }
            None => {}
        }

        // Marked before descending, so that a `$ref` back to this component
        // from inside it is caught as a cycle instead of recursing forever.
        T::cache(&mut self.caches)
            .0
            .insert(name.to_owned(), Slot::InProgress);
        let resolved = self.inline(item)?;
        T::cache(&mut self.caches)
            .0
            .insert(name.to_owned(), Slot::Done(Arc::clone(&resolved)));
        Ok(resolved)
    }
}

/// Resolved components by name, one per type so that same-named components
/// of different kinds never collide.
pub(super) struct Cache<R>(HashMap<String, Slot<R>>);

impl<R> Default for Cache<R> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

enum Slot<R> {
    /// Resolution has started and not finished: reaching this slot again
    /// means the component contains itself.
    InProgress,
    Done(Arc<R>),
}

macro_rules! caches {
    ($( $field:ident : $ty:ty ),+ $(,)?) => {
        #[derive(Default)]
        pub(super) struct Caches {
            $( $field: Cache<<$ty as Resolvable>::Resolved>, )+
        }

        $(
            impl Cached for $ty {
                fn cache(caches: &mut Caches) -> &mut Cache<Self::Resolved> {
                    &mut caches.$field
                }
            }
        )+
    };
}

caches! {
    callbacks: openapiv3::Callback,
    examples: openapiv3::Example,
    headers: openapiv3::Header,
    links: openapiv3::Link,
    parameters: openapiv3::Parameter,
    request_bodies: openapiv3::RequestBody,
    responses: openapiv3::Response,
    schemas: openapiv3::Schema,
    security_schemes: openapiv3::SecurityScheme,
    paths: openapiv3::PathItem,
}
