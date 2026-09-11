use super::resolver::{Resolvable, Resolver};
use super::{ResolvedSchema, ResolvedSchemaKind, ResolvedType};
use crate::{Component, ResolveError};
use openapiv3::{
    BooleanType, Callback, Example, Header, Link, Parameter, PathItem, RequestBody, Response,
    Schema, SchemaData, SecurityScheme,
};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

/// A [`Component`] whose resolved form is shared between all its references.
pub(super) trait Cached: Resolvable + Component {
    /// The cache holding this type's resolved components.
    fn cache(caches: &mut Caches) -> &mut Cache<Self::Resolved>;

    /// Resolves `item`, which is stored under `name`, into its shared `Arc`.
    ///
    /// Marks the slot in progress before descending, so that a `$ref` back to
    /// this component from inside it is seen as a cycle instead of recursing
    /// forever. Only schemas can hand out a usable back-edge, so the default
    /// marker never upgrades.
    fn resolve_shared(
        cx: &mut Resolver<'_>,
        name: &str,
        item: &Self,
    ) -> Result<Arc<Self::Resolved>, ResolveError> {
        Self::cache(&mut cx.caches)
            .0
            .insert(name.to_owned(), Slot::InProgress(Weak::new()));
        item.resolve_inline(cx).map(Arc::new)
    }
}

/// Resolved components by name, one per type so that same-named components
/// of different kinds never collide.
pub(super) struct Cache<R>(pub(super) HashMap<String, Slot<R>>);

impl<R> Default for Cache<R> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

pub(super) enum Slot<R> {
    /// Resolution has started and not finished: reaching this slot again
    /// means the component contains itself. The `Weak` is the allocation
    /// under construction, for the one type that can represent that.
    InProgress(Weak<R>),
    Done(Arc<R>),
}

impl<R> Clone for Slot<R> {
    fn clone(&self) -> Self {
        match self {
            Self::InProgress(weak) => Self::InProgress(Weak::clone(weak)),
            Self::Done(arc) => Self::Done(Arc::clone(arc)),
        }
    }
}

#[derive(Default)]
pub(super) struct Caches {
    callbacks: Cache<<Callback as Resolvable>::Resolved>,
    examples: Cache<Example>,
    headers: Cache<<Header as Resolvable>::Resolved>,
    links: Cache<Link>,
    parameters: Cache<<Parameter as Resolvable>::Resolved>,
    request_bodies: Cache<<RequestBody as Resolvable>::Resolved>,
    responses: Cache<<Response as Resolvable>::Resolved>,
    schemas: Cache<ResolvedSchema>,
    security_schemes: Cache<SecurityScheme>,
    paths: Cache<<PathItem as Resolvable>::Resolved>,
}

macro_rules! cached {
    ($( $field:ident : $ty:ty ),+ $(,)?) => {
        $(
            impl Cached for $ty {
                fn cache(caches: &mut Caches) -> &mut Cache<Self::Resolved> {
                    &mut caches.$field
                }
            }
        )+
    };
}

cached! {
    callbacks: Callback,
    examples: Example,
    headers: Header,
    links: Link,
    parameters: Parameter,
    request_bodies: RequestBody,
    responses: Response,
    security_schemes: SecurityScheme,
    paths: PathItem,
}

impl Cached for Schema {
    fn cache(caches: &mut Caches) -> &mut Cache<Self::Resolved> {
        &mut caches.schemas
    }

    /// Schemas may contain themselves, so the allocation is created first and
    /// its `Weak` published before the contents are resolved; a `$ref` back to
    /// this schema then becomes a recursive `NestedSchema` edge to it.
    fn resolve_shared(
        cx: &mut Resolver<'_>,
        name: &str,
        item: &Self,
    ) -> Result<Arc<ResolvedSchema>, ResolveError> {
        let mut failure = None;
        let resolved = Arc::new_cyclic(|weak| {
            Self::cache(&mut cx.caches)
                .0
                .insert(name.to_owned(), Slot::InProgress(Weak::clone(weak)));
            match item.resolve_inline(cx) {
                Ok(resolved) => resolved,
                Err(error) => {
                    // `new_cyclic` has to be handed a value; this one is
                    // dropped together with the `Arc` as soon as we return.
                    failure = Some(error);
                    ResolvedSchema {
                        schema_data: SchemaData::default(),
                        schema_kind: ResolvedSchemaKind::Type(ResolvedType::Boolean(
                            BooleanType::default(),
                        )),
                    }
                }
            }
        });
        match failure {
            Some(error) => Err(error),
            None => Ok(resolved),
        }
    }
}
