use super::cache::{Cached, Caches, Slot};
use super::discriminator::MappingTarget;
use super::{NestedSchema, Shared};
use crate::reference::escape;
use crate::{lookup_named, walk, ResolveError};
use indexmap::IndexMap;
use openapiv3::{OpenAPI, ReferenceOr, Schema};
use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Arc;

/// A value that can be rebuilt with every `$ref` inside it followed.
pub(super) trait Resolvable {
    /// The reference-free counterpart of `Self`.
    type Resolved;

    /// Rebuilds `self`, resolving every reference it contains through `cx`.
    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError>;
}

/// The state of one full resolution: the document plus everything resolved so far.
pub(super) struct Resolver<'a> {
    openapi: &'a OpenAPI,
    pub(super) caches: Caches,
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
    ) -> Result<Shared<T::Resolved>, ResolveError> {
        self.inline_arc(item).map(Shared::new)
    }

    fn inline_arc<T: Resolvable>(&mut self, item: &T) -> Result<Arc<T::Resolved>, ResolveError> {
        item.resolve_inline(self).map(Arc::new)
    }

    /// Resolves a `$ref`, sharing the result with every other `$ref` to the
    /// same component.
    pub(super) fn reference<T: Cached>(
        &mut self,
        reference: &str,
    ) -> Result<Shared<T::Resolved>, ResolveError> {
        let (name, item) = walk::<T>(self.openapi, reference)?;
        match self.slot::<T>(&name, item)? {
            Slot::Done(resolved) => Ok(Shared::new(resolved)),
            Slot::InProgress(_) => Err(ResolveError::CyclicReference {
                reference: reference.to_owned(),
            }),
        }
    }

    pub(super) fn ref_or<T: Cached>(
        &mut self,
        entry: &ReferenceOr<T>,
    ) -> Result<Shared<T::Resolved>, ResolveError> {
        match entry {
            ReferenceOr::Item(item) => self.inline(item),
            ReferenceOr::Reference { reference } => self.reference::<T>(reference),
        }
    }

    pub(super) fn ref_or_vec<T: Cached>(
        &mut self,
        entries: &[ReferenceOr<T>],
    ) -> Result<Vec<Shared<T::Resolved>>, ResolveError> {
        entries.iter().map(|entry| self.ref_or(entry)).collect()
    }

    pub(super) fn ref_or_map<K, T>(
        &mut self,
        entries: &IndexMap<K, ReferenceOr<T>>,
    ) -> Result<IndexMap<K, Shared<T::Resolved>>, ResolveError>
    where
        K: Clone + Hash + Eq,
        T: Cached,
    {
        entries
            .iter()
            .map(|(key, entry)| Ok((key.clone(), self.ref_or(entry)?)))
            .collect()
    }

    /// Resolves a schema nested inside another schema, which is the one place
    /// a `$ref` may point back at a schema still being built.
    pub(super) fn nested<S: Borrow<Schema>>(
        &mut self,
        entry: &ReferenceOr<S>,
    ) -> Result<NestedSchema, ResolveError> {
        match entry {
            ReferenceOr::Item(item) => self.inline_arc(item.borrow()).map(NestedSchema::schema),
            ReferenceOr::Reference { reference } => self.nested_reference(reference),
        }
    }

    /// Resolves a discriminator mapping value found inside a schema that has
    /// no alternatives to match it against; see [`Self::nested`].
    pub(super) fn nested_target(
        &mut self,
        target: MappingTarget<'_>,
    ) -> Result<NestedSchema, ResolveError> {
        match target {
            MappingTarget::Reference(reference) => self.nested_reference(reference),
            MappingTarget::Name(name) => match lookup_named::<Schema>(self.openapi, name)? {
                ReferenceOr::Item(item) => self.nested_slot(name, item),
                ReferenceOr::Reference { reference } => self.nested_reference(reference),
            },
        }
    }

    /// The name of the component a mapping value finally denotes, after any
    /// chain of `$ref`s, which is the name two ways of naming one schema
    /// agree on. A bare name is looked up as written, never parsed as a
    /// pointer.
    pub(super) fn schema_name(&self, target: MappingTarget<'_>) -> Result<String, ResolveError> {
        let reference = match target {
            MappingTarget::Reference(reference) => reference,
            MappingTarget::Name(name) => match lookup_named::<Schema>(self.openapi, name)? {
                ReferenceOr::Item(_) => return Ok(name.to_owned()),
                ReferenceOr::Reference { reference } => reference,
            },
        };
        walk::<Schema>(self.openapi, reference).map(|(name, _)| name.into_owned())
    }

    fn nested_reference(&mut self, reference: &str) -> Result<NestedSchema, ResolveError> {
        let (name, item) = walk::<Schema>(self.openapi, reference)?;
        self.nested_slot(&name, item)
    }

    fn nested_slot(&mut self, name: &str, item: &Schema) -> Result<NestedSchema, ResolveError> {
        Ok(match self.slot::<Schema>(name, item)? {
            Slot::Done(resolved) => NestedSchema::schema(resolved),
            Slot::InProgress(weak) => NestedSchema::recursive(weak),
        })
    }

    pub(super) fn nested_vec(
        &mut self,
        entries: &[ReferenceOr<Schema>],
    ) -> Result<Vec<NestedSchema>, ResolveError> {
        entries.iter().map(|entry| self.nested(entry)).collect()
    }

    pub(super) fn nested_map<S: Borrow<Schema>>(
        &mut self,
        entries: &IndexMap<String, ReferenceOr<S>>,
    ) -> Result<IndexMap<String, NestedSchema>, ResolveError> {
        entries
            .iter()
            .map(|(key, entry)| Ok((key.clone(), self.nested(entry)?)))
            .collect()
    }

    /// Resolves a whole section of the document, e.g. `#/components/schemas`
    /// or `#/paths`, so that its entries share their `Arc`s with the
    /// references that point at them.
    pub(super) fn section<T: Cached>(
        &mut self,
        entries: &IndexMap<String, ReferenceOr<T>>,
    ) -> Result<IndexMap<String, Shared<T::Resolved>>, ResolveError> {
        entries
            .iter()
            .map(|(name, entry)| {
                let resolved = match entry {
                    ReferenceOr::Item(item) => {
                        let pointer = format!("#/{}/{}", T::SECTION, escape(name));
                        self.reference_named::<T>(name, item, &pointer)?
                    }
                    ReferenceOr::Reference { reference } => self.reference::<T>(reference)?,
                };
                Ok((name.clone(), resolved))
            })
            .collect()
    }

    /// Like [`Self::reference`], for an item whose name is already known.
    fn reference_named<T: Cached>(
        &mut self,
        name: &str,
        item: &T,
        reference: &str,
    ) -> Result<Shared<T::Resolved>, ResolveError> {
        match self.slot::<T>(name, item)? {
            Slot::Done(resolved) => Ok(Shared::new(resolved)),
            Slot::InProgress(_) => Err(ResolveError::CyclicReference {
                reference: reference.to_owned(),
            }),
        }
    }

    /// The slot for the component stored under `name`, resolving it first if
    /// this is the first time it is reached.
    fn slot<T: Cached>(&mut self, name: &str, item: &T) -> Result<Slot<T::Resolved>, ResolveError> {
        if let Some(slot) = T::cache(&mut self.caches).0.get(name) {
            return Ok(slot.clone());
        }
        let resolved = T::resolve_shared(self, name, item)?;
        T::cache(&mut self.caches)
            .0
            .insert(name.to_owned(), Slot::Done(Arc::clone(&resolved)));
        Ok(Slot::Done(resolved))
    }
}
