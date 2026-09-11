use crate::Section;
use std::fmt;

/// Everything that can go wrong while resolving a `$ref`.
///
/// The variants distinguish a broken document (a dangling name, a `$ref` to
/// the wrong kind of component) from a reference this crate structurally
/// cannot follow (another file, a pointer into the middle of a component), so
/// a caller can decide which ones are worth aborting on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// The reference is not a JSON pointer into the current document.
    NotALocalReference {
        /// The reference as it appeared in the document.
        reference: String,
    },
    /// The reference points into another document, which this crate does not load.
    ExternalDocument {
        /// The part of the reference before the `#`, e.g. `common.yaml`.
        document: String,
    },
    /// The reference is a local pointer but has too few segments to name a component.
    MalformedPointer {
        /// The reference as it appeared in the document.
        reference: String,
    },
    /// The pointer starts at a section of the document that cannot hold `$ref` targets.
    UnsupportedRootSection {
        /// The first pointer segment, e.g. `info`.
        section: String,
    },
    /// The pointer names a `#/components` sub-section that does not exist.
    UnknownSection {
        /// The offending segment, e.g. `request_bodies` instead of `requestBodies`.
        section: String,
    },
    /// The pointer names a component but the document has no `components` object.
    ComponentsMissing {
        /// The section the pointer asked for.
        section: Section,
    },
    /// The pointer is well formed but names something the section does not contain.
    NotFound {
        /// The section that was searched.
        section: Section,
        /// The component name, after JSON pointer unescaping.
        name: String,
    },
    /// The pointer names a valid component of a different kind than the caller asked for.
    SectionMismatch {
        /// The section implied by the requested type.
        expected: Section,
        /// The section the pointer actually names.
        found: Section,
    },
    /// The pointer continues past a component, into its contents.
    PointerTooDeep {
        /// The reference as it appeared in the document.
        reference: String,
    },
    /// The chain of references never reached an inline item; the document is
    /// most likely cyclic.
    ReferenceChainTooLong {
        /// The reference the walk started from.
        reference: String,
        /// The reference the walk gave up on, which is where the cycle runs.
        last: String,
        /// The hop limit that was hit, [`crate::MAX_REFERENCE_HOPS`].
        max_hops: usize,
    },
    /// A component other than a schema contains, directly or through other
    /// components, a `$ref` back to itself, so it has no finite fully resolved
    /// form. A header whose content encoding names that same header is the
    /// one way a document can do this.
    ///
    /// Schemas are allowed to contain themselves; see
    /// [`NestedSchema`](crate::NestedSchema). Only
    /// [`ResolvedOpenAPI`](crate::ResolvedOpenAPI) reports this; borrowing
    /// resolution stops at the first item and never notices the cycle.
    CyclicReference {
        /// The `$ref` that closed the cycle, as it appeared in the document.
        reference: String,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALocalReference { reference } => {
                write!(
                    f,
                    "`{reference}` is not a local JSON pointer (expected `#/...`)"
                )
            }
            Self::ExternalDocument { document } => {
                write!(
                    f,
                    "reference points into `{document}`, which is a separate document"
                )
            }
            Self::MalformedPointer { reference } => {
                write!(f, "`{reference}` does not name a component")
            }
            Self::UnsupportedRootSection { section } => {
                write!(f, "cannot resolve references into `{section}`")
            }
            Self::UnknownSection { section } => {
                write!(f, "`{section}` is not a known section of `#/components`")
            }
            Self::ComponentsMissing { section } => {
                write!(
                    f,
                    "document has no `components` object to look up `{section}` in"
                )
            }
            Self::NotFound { section, name } => {
                write!(f, "`{section}` does not contain `{name}`")
            }
            Self::SectionMismatch { expected, found } => {
                write!(
                    f,
                    "expected a reference into `{expected}` but got one into `{found}`"
                )
            }
            Self::PointerTooDeep { reference } => {
                write!(
                    f,
                    "`{reference}` points inside a component; only whole components resolve"
                )
            }
            Self::ReferenceChainTooLong {
                reference,
                last,
                max_hops,
            } => {
                write!(
                    f,
                    "`{reference}` did not reach an item within {max_hops} hops, \
                     still at `{last}` (cyclic?)"
                )
            }
            Self::CyclicReference { reference } => {
                write!(
                    f,
                    "`{reference}` refers back to a component that contains it; \
                     a cyclic document cannot be fully resolved"
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}
