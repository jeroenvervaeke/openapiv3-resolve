use crate::Section;
use indexmap::IndexMap;
use openapiv3::{
    Callback, Example, Header, Link, OpenAPI, Parameter, PathItem, ReferenceOr, RequestBody,
    Response, Schema, SecurityScheme,
};

/// A type that `$ref` pointers can name.
///
/// Implemented for the nine `#/components` types and for [`PathItem`]. The
/// type is what decides which section is searched, so a pointer into a
/// different section is reported as
/// [`ResolveError::SectionMismatch`](crate::ResolveError::SectionMismatch)
/// rather than silently missing.
///
/// Note that [`Callback`] is a transparent alias for
/// `IndexMap<String, PathItem>` rather than a distinct type, so *any* value of
/// that shape resolves as a callback. That is upstream's design, not a choice
/// this crate can undo.
///
/// This trait is sealed: it cannot be implemented outside this crate. That
/// keeps [`Self::SECTION`] and [`Self::section`] in agreement, and it is what
/// makes the blanket impls on `ReferenceOr<T>` and `ReferenceOr<Box<T>>`
/// non-overlapping for good.
pub trait Component: sealed::Sealed + Sized {
    /// The section of the document holding values of this type.
    const SECTION: Section;

    /// Borrows that section, or `None` if the document omits it entirely.
    fn section(openapi: &OpenAPI) -> Option<&IndexMap<String, ReferenceOr<Self>>>;
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! component {
    ($ty:ty, $section:ident, $field:ident) => {
        impl sealed::Sealed for $ty {}

        impl Component for $ty {
            const SECTION: Section = Section::$section;

            fn section(openapi: &OpenAPI) -> Option<&IndexMap<String, ReferenceOr<Self>>> {
                Some(&openapi.components.as_ref()?.$field)
            }
        }
    };
}

// The section name is the wire spelling, which `Components` renames from its
// Rust field name; deriving one from the other is what made `requestBodies`
// and `securitySchemes` unresolvable.
component!(Callback, Callbacks, callbacks);
component!(Example, Examples, examples);
component!(Header, Headers, headers);
component!(Link, Links, links);
component!(Parameter, Parameters, parameters);
component!(RequestBody, RequestBodies, request_bodies);
component!(Response, Responses, responses);
component!(Schema, Schemas, schemas);
component!(SecurityScheme, SecuritySchemes, security_schemes);

impl sealed::Sealed for PathItem {}

impl Component for PathItem {
    const SECTION: Section = Section::Paths;

    fn section(openapi: &OpenAPI) -> Option<&IndexMap<String, ReferenceOr<Self>>> {
        Some(&openapi.paths.paths)
    }
}
