//! Errors produced by [`resolve`](crate::resolve).

use thiserror::Error;

/// Failure modes for the resolver.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// A `$ref` did not begin with `#/components/...`. External refs are not
    /// supported.
    #[error(
        "unsupported $ref `{reference}`: only internal refs starting with `#/components/` are supported"
    )]
    UnsupportedReference {
        /// The reference string that failed to parse.
        reference: String,
    },

    /// A `$ref` pointed at a component kind that does not match the expected
    /// type at this position (e.g. a Schema position pointed at
    /// `#/components/responses/...`).
    #[error("$ref `{reference}` points at `{found}` but the position requires `{expected}`")]
    WrongReferenceKind {
        /// The reference string.
        reference: String,
        /// The component kind expected at this position
        /// (e.g. `"schemas"`).
        expected: &'static str,
        /// The component kind found in the reference (e.g. `"responses"`).
        found: String,
    },

    /// A `$ref` pointed at a name that does not exist in the corresponding
    /// components map.
    #[error("$ref `{reference}` does not resolve to any component")]
    UnresolvedReference {
        /// The reference string.
        reference: String,
    },

    /// A chain of `$ref`s in the components map (e.g.
    /// `Foo -> Bar -> Foo`) never reaches an inline definition.
    #[error(
        "$ref chain in components/{kind} forms a cycle without an inline definition: {}",
        chain.join(" -> ")
    )]
    ReferenceCycle {
        /// The names visited while following the chain, in order.
        chain: Vec<String>,
        /// The component kind being chased (e.g. `"schemas"`).
        kind: &'static str,
    },

    /// A `$ref` to a [`PathItem`](openapiv3::PathItem) was encountered. Path
    /// items are not stored in `components` in OpenAPI 3.0, so refs to them
    /// cannot be resolved internally.
    #[error(
        "$ref `{reference}` to a PathItem cannot be resolved: path items are not stored in components"
    )]
    PathItemReference {
        /// The reference string.
        reference: String,
    },
}
