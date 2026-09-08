use crate::ResolveError;
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

/// A section of an OpenAPI document that `$ref` pointers can name.
///
/// The string form is the *wire* spelling used in a document
/// (`components/requestBodies`), which is not the same as the Rust field name
/// on [`openapiv3::Components`] (`request_bodies`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Section {
    /// `#/components/callbacks`
    Callbacks,
    /// `#/components/examples`
    Examples,
    /// `#/components/headers`
    Headers,
    /// `#/components/links`
    Links,
    /// `#/components/parameters`
    Parameters,
    /// `#/components/requestBodies`
    RequestBodies,
    /// `#/components/responses`
    Responses,
    /// `#/components/schemas`
    Schemas,
    /// `#/components/securitySchemes`
    SecuritySchemes,
    /// `#/paths`
    Paths,
}

impl Section {
    /// The pointer body for this section, without the leading `#/`.
    pub const fn as_pointer(self) -> &'static str {
        match self {
            Self::Callbacks => "components/callbacks",
            Self::Examples => "components/examples",
            Self::Headers => "components/headers",
            Self::Links => "components/links",
            Self::Parameters => "components/parameters",
            Self::RequestBodies => "components/requestBodies",
            Self::Responses => "components/responses",
            Self::Schemas => "components/schemas",
            Self::SecuritySchemes => "components/securitySchemes",
            Self::Paths => "paths",
        }
    }

    const fn from_components_segment(segment: &str) -> Option<Self> {
        // `match` on &str is not const-friendly, so compare bytes.
        Some(match segment.as_bytes() {
            b"callbacks" => Self::Callbacks,
            b"examples" => Self::Examples,
            b"headers" => Self::Headers,
            b"links" => Self::Links,
            b"parameters" => Self::Parameters,
            b"requestBodies" => Self::RequestBodies,
            b"responses" => Self::Responses,
            b"schemas" => Self::Schemas,
            b"securitySchemes" => Self::SecuritySchemes,
            _ => return None,
        })
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_pointer())
    }
}

impl FromStr for Section {
    type Err = ResolveError;

    /// Parses the pointer body of a section, e.g. `components/schemas`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == Self::Paths.as_pointer() {
            return Ok(Self::Paths);
        }
        let Some(segment) = s.strip_prefix("components/") else {
            return Err(ResolveError::UnsupportedRootSection {
                section: s.to_owned(),
            });
        };
        Self::from_components_segment(segment).ok_or_else(|| ResolveError::UnknownSection {
            section: segment.to_owned(),
        })
    }
}

/// A parsed `$ref` pointer: the section it names and the (unescaped) name within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRef<'a> {
    /// The section the pointer names.
    pub section: Section,
    /// The component name, with JSON pointer escapes (`~0`, `~1`) resolved.
    pub name: Cow<'a, str>,
}

impl<'a> ComponentRef<'a> {
    /// Parses a `$ref` string such as `#/components/schemas/Pet`.
    ///
    /// Borrows the name from `reference` unless it contains an escape.
    pub fn parse(reference: &'a str) -> Result<Self, ResolveError> {
        let Some(pointer) = reference.strip_prefix("#/") else {
            return Err(match reference.split_once('#') {
                Some((document, _)) if !document.is_empty() => ResolveError::ExternalDocument {
                    document: document.to_owned(),
                },
                _ => ResolveError::NotALocalReference {
                    reference: reference.to_owned(),
                },
            });
        };

        let malformed = || ResolveError::MalformedPointer {
            reference: reference.to_owned(),
        };
        let (root, rest) = pointer.split_once('/').ok_or_else(malformed)?;

        let (section, name) = match root {
            "components" => {
                let (segment, name) = rest.split_once('/').ok_or_else(malformed)?;
                let section = Section::from_components_segment(segment).ok_or_else(|| {
                    ResolveError::UnknownSection {
                        section: segment.to_owned(),
                    }
                })?;
                (section, name)
            }
            "paths" => (Section::Paths, rest),
            other => {
                return Err(ResolveError::UnsupportedRootSection {
                    section: other.to_owned(),
                })
            }
        };

        if name.contains('/') {
            return Err(ResolveError::PointerTooDeep {
                reference: reference.to_owned(),
            });
        }

        Ok(Self {
            section,
            name: unescape(name),
        })
    }
}

/// Resolves RFC 6901 escapes. `~1` becomes `/` and `~0` becomes `~`, in that
/// order, so `~01` round-trips to `~1` rather than to `/`.
fn unescape(segment: &str) -> Cow<'_, str> {
    if segment.contains('~') {
        Cow::Owned(segment.replace("~1", "/").replace("~0", "~"))
    } else {
        Cow::Borrowed(segment)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const ALL: [Section; 10] = [
        Section::Callbacks,
        Section::Examples,
        Section::Headers,
        Section::Links,
        Section::Parameters,
        Section::RequestBodies,
        Section::Responses,
        Section::Schemas,
        Section::SecuritySchemes,
        Section::Paths,
    ];

    #[test]
    fn every_section_round_trips_through_its_pointer() {
        for section in ALL {
            assert_eq!(Section::from_str(section.as_pointer()), Ok(section));
        }
    }

    #[test]
    fn section_pointers_use_wire_spelling_not_rust_field_names() {
        assert_eq!(
            Section::RequestBodies.as_pointer(),
            "components/requestBodies"
        );
        assert_eq!(
            Section::SecuritySchemes.as_pointer(),
            "components/securitySchemes"
        );
    }

    #[test]
    fn parses_a_pointer_into_every_section() {
        for section in ALL {
            let reference = format!("#/{}/Thing", section.as_pointer());
            let parsed = ComponentRef::parse(&reference);
            assert_eq!(
                parsed,
                Ok(ComponentRef {
                    section,
                    name: Cow::Borrowed("Thing")
                })
            );
        }
    }

    #[test]
    fn borrows_the_name_when_there_is_nothing_to_unescape() {
        let parsed = ComponentRef::parse("#/components/schemas/Pet").unwrap();
        assert!(matches!(parsed.name, Cow::Borrowed("Pet")));
    }

    #[test]
    fn unescapes_a_slash_in_a_path_name() {
        let parsed = ComponentRef::parse("#/paths/~1pets~1{id}").unwrap();
        assert_eq!(parsed.name, "/pets/{id}");
    }

    #[test]
    fn unescapes_tilde_after_slash_so_escaped_escapes_survive() {
        // `~01` must become `~1`, not `/`.
        let parsed = ComponentRef::parse("#/components/schemas/a~01b").unwrap();
        assert_eq!(parsed.name, "a~1b");
    }

    #[test]
    fn rejects_a_reference_without_the_local_prefix() {
        assert_eq!(
            ComponentRef::parse("Pet"),
            Err(ResolveError::NotALocalReference {
                reference: "Pet".to_owned()
            })
        );
    }

    #[test]
    fn rejects_a_reference_into_another_document() {
        assert_eq!(
            ComponentRef::parse("common.yaml#/components/schemas/Pet"),
            Err(ResolveError::ExternalDocument {
                document: "common.yaml".to_owned()
            })
        );
    }

    #[test]
    fn rejects_a_pointer_with_too_few_segments() {
        assert_eq!(
            ComponentRef::parse("#/components/schemas"),
            Err(ResolveError::MalformedPointer {
                reference: "#/components/schemas".to_owned()
            })
        );
    }

    #[test]
    fn rejects_a_root_section_that_cannot_hold_targets() {
        assert_eq!(
            ComponentRef::parse("#/info/title"),
            Err(ResolveError::UnsupportedRootSection {
                section: "info".to_owned()
            })
        );
    }

    #[test]
    fn rejects_a_rust_field_name_used_as_a_section() {
        assert_eq!(
            ComponentRef::parse("#/components/request_bodies/CreatePet"),
            Err(ResolveError::UnknownSection {
                section: "request_bodies".to_owned()
            })
        );
    }

    #[test]
    fn rejects_a_pointer_into_the_middle_of_a_component() {
        let reference = "#/components/schemas/Pet/properties/name";
        assert_eq!(
            ComponentRef::parse(reference),
            Err(ResolveError::PointerTooDeep {
                reference: reference.to_owned()
            })
        );
    }
}
