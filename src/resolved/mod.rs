//! A whole document with every `$ref` followed, for callers that would rather
//! walk a plain tree than resolve at each site.

mod cache;
mod document;
mod operation;
mod parameter;
mod resolver;
mod response;
mod schema;
mod schema_type;

pub use document::{
    ResolvedCallback, ResolvedComponents, ResolvedOpenAPI, ResolvedPathItem, ResolvedPaths,
};
pub use operation::{ResolvedOperation, ResolvedResponses};
pub use parameter::{
    ResolvedHeader, ResolvedParameter, ResolvedParameterData, ResolvedParameterSchemaOrContent,
};
pub use response::{ResolvedEncoding, ResolvedMediaType, ResolvedRequestBody, ResolvedResponse};
pub use schema::{NestedSchema, ResolvedAdditionalProperties, ResolvedSchema, ResolvedSchemaKind};
pub use schema_type::{ResolvedAnySchema, ResolvedArrayType, ResolvedObjectType, ResolvedType};

use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{Example, Link, OpenAPI, SecurityScheme};
use resolver::{Resolvable, Resolver};

impl TryFrom<&OpenAPI> for ResolvedOpenAPI {
    type Error = ResolveError;

    /// Resolves every `$ref` in the document.
    ///
    /// Fails with the first reference that does not resolve. A schema that
    /// contains a reference back to itself is fine, and comes out with a
    /// [`NestedSchema::Recursive`] edge; any other component that contains
    /// itself has no finite tree form and fails with
    /// [`ResolveError::CyclicReference`].
    fn try_from(openapi: &OpenAPI) -> Result<Self, Self::Error> {
        openapi.resolve_inline(&mut Resolver::new(openapi))
    }
}

impl TryFrom<OpenAPI> for ResolvedOpenAPI {
    type Error = ResolveError;

    fn try_from(openapi: OpenAPI) -> Result<Self, Self::Error> {
        Self::try_from(&openapi)
    }
}

/// Resolves every value of a map of inline items, keeping keys and order.
fn resolve_map<T: Resolvable>(
    map: &IndexMap<String, T>,
    cx: &mut Resolver<'_>,
) -> Result<IndexMap<String, T::Resolved>, ResolveError> {
    map.iter()
        .map(|(key, value)| Ok((key.clone(), value.resolve_inline(cx)?)))
        .collect()
}

macro_rules! reference_free {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Resolvable for $ty {
                type Resolved = Self;

                fn resolve_inline(&self, _: &mut Resolver<'_>) -> Result<Self, ResolveError> {
                    Ok(self.clone())
                }
            }
        )+
    };
}

// These hold no `$ref`, so upstream's type already is the resolved form.
reference_free!(Example, Link, SecurityScheme);
