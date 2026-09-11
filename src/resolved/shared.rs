use super::ResolvedSchema;
use std::fmt;
use std::ops::Deref;
use std::sync::{Arc, Weak};

/// A resolved item, shared by every `$ref` that named it.
///
/// Dereferences to the item. It cannot be cloned, and the document it belongs
/// to hands out borrows only, so a `Shared` value can never outlive its
/// document: that is what lets a [`NestedSchema`] be dereferenced without a
/// check, however the schemas refer to each other.
///
/// ```compile_fail
/// use openapiv3_resolve::{ResolvedSchema, Shared};
///
/// fn detach(shared: &Shared<ResolvedSchema>) -> Shared<ResolvedSchema> {
///     shared.clone()
/// }
/// ```
pub struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    pub(super) fn new(item: Arc<T>) -> Self {
        Self(item)
    }

    /// The address of the item, for telling whether two `Shared` values name
    /// the same component: `std::ptr::eq(Shared::as_ptr(a), Shared::as_ptr(b))`.
    ///
    /// An associated function, like [`Arc::as_ptr`], so it cannot shadow a
    /// method of `T`.
    pub fn as_ptr(this: &Self) -> *const T {
        Arc::as_ptr(&this.0)
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// A schema nested inside another schema.
///
/// [`get`](Self::get) borrows the nested schema. Nearly always that schema is
/// owned or shared like any other; the exception is a `$ref` that points back
/// at a schema which contains it (a tree node whose children are nodes, say).
/// Such an edge is [`recursive`](Self::is_recursive) and holds only a weak
/// pointer, because a cycle of owning pointers would never be freed. Which
/// edge of a cycle is the recursive one is decided by document order: it is
/// the first `$ref`, walking `components` then `paths`, that closes the cycle.
///
/// The weak pointer always upgrades: every schema a `$ref` can name lives in
/// the document's `components`, and this edge can only be reached by
/// borrowing from that document.
///
/// Two recursive edges are equal when they point at the same allocation, so
/// comparing two independently resolved documents that contain a cycle
/// reports them as different.
pub struct NestedSchema(Edge);

enum Edge {
    Schema(Arc<ResolvedSchema>),
    Recursive(Weak<ResolvedSchema>),
}

impl NestedSchema {
    pub(super) fn schema(schema: Arc<ResolvedSchema>) -> Self {
        Self(Edge::Schema(schema))
    }

    pub(super) fn recursive(schema: Weak<ResolvedSchema>) -> Self {
        Self(Edge::Recursive(schema))
    }

    /// Borrows the nested schema.
    pub fn get(&self) -> SchemaGuard<'_> {
        SchemaGuard(match &self.0 {
            Edge::Schema(schema) => Guard::Borrowed(schema),
            // Infallible: the target is owned by the document's `components`,
            // and `self` can only be reached by borrowing from that document
            // (`Shared` is not `Clone`, and the document hands out borrows only).
            #[allow(clippy::expect_used)]
            Edge::Recursive(schema) => Guard::Upgraded(
                schema
                    .upgrade()
                    .expect("recursive schema edge outlived its document"),
            ),
        })
    }

    /// Whether this edge points back at a schema that contains it.
    pub fn is_recursive(&self) -> bool {
        matches!(self.0, Edge::Recursive(_))
    }
}

impl fmt::Debug for NestedSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Edge::Schema(schema) => schema.fmt(f),
            // Printing the target would never terminate.
            Edge::Recursive(_) => f.write_str("Recursive(..)"),
        }
    }
}

impl PartialEq for NestedSchema {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Edge::Schema(left), Edge::Schema(right)) => left == right,
            // Comparing contents here would never terminate.
            (Edge::Recursive(left), Edge::Recursive(right)) => Weak::ptr_eq(left, right),
            (Edge::Schema(_), Edge::Recursive(_)) | (Edge::Recursive(_), Edge::Schema(_)) => false,
        }
    }
}

/// A borrow of a [`NestedSchema`]'s target; dereferences to the schema.
pub struct SchemaGuard<'a>(Guard<'a>);

enum Guard<'a> {
    Borrowed(&'a ResolvedSchema),
    Upgraded(Arc<ResolvedSchema>),
}

impl SchemaGuard<'_> {
    /// The address of the schema, comparable with [`Shared::as_ptr`].
    pub fn as_ptr(this: &Self) -> *const ResolvedSchema {
        match &this.0 {
            Guard::Borrowed(schema) => *schema,
            Guard::Upgraded(schema) => Arc::as_ptr(schema),
        }
    }
}

impl Deref for SchemaGuard<'_> {
    type Target = ResolvedSchema;

    fn deref(&self) -> &ResolvedSchema {
        match &self.0 {
            Guard::Borrowed(schema) => schema,
            Guard::Upgraded(schema) => schema,
        }
    }
}

impl fmt::Debug for SchemaGuard<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}
