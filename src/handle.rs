//! Reference-counted handles used by the resolved tree.
//!
//! Inline definitions, the entries stored in [`ResolvedComponents`], and
//! `$ref` back-edges all need a uniform smart-pointer story. We use a small
//! pair of wrappers around `Arc<OnceLock<T>>` / `Weak<OnceLock<T>>`:
//!
//! * [`Resolved<T>`] behaves like `Arc<T>` for the consumer: it is `Clone`,
//!   `Deref<Target = T>`, and dereferences to the resolved value.
//! * [`ResolvedWeak<T>`] is the `Weak` half. It is used for `$ref` edges so
//!   that cycles between named components do not leak — the components map
//!   holds the strong references that keep everything alive.
//!
//! Construction is two-phase: the resolver allocates an empty [`Resolved`]
//! for each named component, then fills it in once all entries exist. From a
//! consumer's perspective the value is always present; calling
//! [`Resolved::deref`] on a half-built value would panic, but the resolver
//! never hands out a [`ResolvedOpenAPI`] until every slot has been populated.
//!
//! [`ResolvedComponents`]: crate::ResolvedComponents
//! [`ResolvedOpenAPI`]: crate::ResolvedOpenAPI

use std::fmt;
use std::ops::Deref;
use std::sync::{Arc, OnceLock, Weak};

/// A handle to a resolved value, semantically equivalent to `Arc<T>`.
pub struct Resolved<T> {
    inner: Arc<OnceLock<T>>,
}

impl<T> Resolved<T> {
    /// Wraps `value` in a freshly allocated [`Resolved`] handle.
    pub fn new(value: T) -> Self {
        let cell = OnceLock::new();
        // Safe: the cell is brand new, so this cannot fail.
        let _ = cell.set(value);
        Self {
            inner: Arc::new(cell),
        }
    }

    /// Allocates an empty handle. Used by the resolver for the two-phase
    /// build of named components; the value is filled in via [`set`] before
    /// the resolved tree is returned to the caller.
    ///
    /// [`set`]: Self::set
    pub(crate) fn empty() -> Self {
        Self {
            inner: Arc::new(OnceLock::new()),
        }
    }

    /// Stores `value` into a previously [`empty`](Self::empty) handle.
    /// Returns `Err(value)` if the handle was already populated.
    pub(crate) fn set(&self, value: T) -> Result<(), T> {
        self.inner.set(value)
    }

    /// Returns whether this handle has been populated with a value.
    pub fn is_set(&self) -> bool {
        self.inner.get().is_some()
    }

    /// Borrows the underlying value, returning `None` if the handle has
    /// not been populated yet. End users will normally not need this —
    /// every handle reachable from a [`ResolvedOpenAPI`] is populated.
    ///
    /// [`ResolvedOpenAPI`]: crate::ResolvedOpenAPI
    pub fn try_get(&self) -> Option<&T> {
        self.inner.get()
    }

    /// Creates a non-owning [`ResolvedWeak`] handle to the same value.
    pub fn downgrade(&self) -> ResolvedWeak<T> {
        ResolvedWeak {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Returns true when the two handles point at the same allocation.
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        Arc::ptr_eq(&a.inner, &b.inner)
    }
}

impl<T> Clone for Resolved<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Deref for Resolved<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner
            .get()
            .expect("Resolved value accessed before initialization")
    }
}

impl<T> AsRef<T> for Resolved<T> {
    fn as_ref(&self) -> &T {
        self.deref()
    }
}

impl<T: fmt::Debug> fmt::Debug for Resolved<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.get() {
            Some(v) => f.debug_tuple("Resolved").field(v).finish(),
            None => f.debug_tuple("Resolved").field(&"<uninitialized>").finish(),
        }
    }
}

impl<T: PartialEq> PartialEq for Resolved<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner) || self.try_get() == other.try_get()
    }
}

/// A weak handle to a resolved value, semantically equivalent to `Weak<T>`.
///
/// Used for `$ref` back-edges so that cycles between named components do not
/// leak. The strong references live in [`ResolvedComponents`], so as long as
/// the resolved document is alive these weak handles upgrade successfully.
///
/// [`ResolvedComponents`]: crate::ResolvedComponents
pub struct ResolvedWeak<T> {
    inner: Weak<OnceLock<T>>,
}

impl<T> ResolvedWeak<T> {
    /// Attempts to upgrade to a strong [`Resolved`] handle.
    pub fn upgrade(&self) -> Option<Resolved<T>> {
        self.inner.upgrade().map(|inner| Resolved { inner })
    }
}

impl<T> Clone for ResolvedWeak<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Weak::clone(&self.inner),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for ResolvedWeak<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.upgrade() {
            Some(r) => f.debug_tuple("ResolvedWeak").field(&*r).finish(),
            None => f.debug_tuple("ResolvedWeak").field(&"<dropped>").finish(),
        }
    }
}

/// Either an inline resolved value or a `$ref` back-edge to a named component.
///
/// This is the resolved counterpart of [`openapiv3::ReferenceOr`]. Use
/// [`get`](Self::get) to obtain a strong handle to the underlying value
/// regardless of which arm is in use.
pub enum ResolvedRefOr<T> {
    /// An inline definition. Owns the value via a strong handle.
    Item(Resolved<T>),
    /// A `$ref` to a named component, stored as a weak handle so cycles do
    /// not leak. The strong reference is held by [`ResolvedComponents`].
    ///
    /// [`ResolvedComponents`]: crate::ResolvedComponents
    Reference(ResolvedWeak<T>),
}

impl<T> ResolvedRefOr<T> {
    /// Returns a strong [`Resolved`] handle to the underlying value, or
    /// `None` if this is a [`Reference`](Self::Reference) whose target has
    /// been dropped (which can only happen if the parent
    /// [`ResolvedOpenAPI`](crate::ResolvedOpenAPI) was dropped).
    pub fn get(&self) -> Option<Resolved<T>> {
        match self {
            Self::Item(r) => Some(r.clone()),
            Self::Reference(w) => w.upgrade(),
        }
    }

    /// Returns true if this is an inline item.
    pub fn is_item(&self) -> bool {
        matches!(self, Self::Item(_))
    }

    /// Returns true if this is a `$ref`.
    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference(_))
    }
}

impl<T> Clone for ResolvedRefOr<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Item(r) => Self::Item(r.clone()),
            Self::Reference(w) => Self::Reference(w.clone()),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for ResolvedRefOr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Item(r) => f.debug_tuple("Item").field(r).finish(),
            Self::Reference(w) => f.debug_tuple("Reference").field(w).finish(),
        }
    }
}
