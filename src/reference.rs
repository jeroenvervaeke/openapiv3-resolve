use crate::ResolveError;
use std::borrow::Cow;
use std::fmt;

/// Declares the sections in one place so the variant list, [`Section::ALL`]
/// and the wire spellings cannot drift apart.
macro_rules! sections {
    ($( $(#[$meta:meta])* $variant:ident => $pointer:literal ),+ $(,)?) => {
        /// A section of an OpenAPI document that `$ref` pointers can name.
        ///
        /// The string form is the *wire* spelling used in a document
        /// (`components/requestBodies`), which is not the same as the Rust
        /// field name on [`openapiv3::Components`] (`request_bodies`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Section {
            $( $(#[$meta])* $variant, )+
        }

        impl Section {
            /// Every section this crate can resolve into.
            pub const ALL: &'static [Section] = &[ $( Section::$variant, )+ ];

            /// The pointer body for this section, without the leading `#/`.
            pub const fn as_pointer(self) -> &'static str {
                match self { $( Self::$variant => $pointer, )+ }
            }
        }
    };
}

sections! {
    /// `#/components/callbacks`
    Callbacks => "components/callbacks",
    /// `#/components/examples`
    Examples => "components/examples",
    /// `#/components/headers`
    Headers => "components/headers",
    /// `#/components/links`
    Links => "components/links",
    /// `#/components/parameters`
    Parameters => "components/parameters",
    /// `#/components/requestBodies`
    RequestBodies => "components/requestBodies",
    /// `#/components/responses`
    Responses => "components/responses",
    /// `#/components/schemas`
    Schemas => "components/schemas",
    /// `#/components/securitySchemes`
    SecuritySchemes => "components/securitySchemes",
    /// `#/paths`
    Paths => "paths",
}

impl Section {
    fn from_components_segment(segment: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|section| section.as_pointer().strip_prefix("components/") == Some(segment))
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_pointer())
    }
}

/// A parsed `$ref` pointer: the section it names and the name within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComponentRef<'a> {
    pub section: Section,
    /// The component name, percent-decoded and JSON pointer unescaped.
    pub name: Cow<'a, str>,
}

impl<'a> ComponentRef<'a> {
    /// Parses a `$ref` string such as `#/components/schemas/Pet`.
    ///
    /// A `$ref` is a URI reference, so the fragment is percent-decoded before
    /// it is read as a JSON pointer: `#/paths/~1pets~1%7Bid%7D` names the path
    /// `/pets/{id}`. Borrows from `reference` when neither step has anything
    /// to do, which is every `#/components/...` pointer in practice.
    pub(crate) fn parse(reference: &'a str) -> Result<Self, ResolveError> {
        match percent_decode(reference) {
            Cow::Borrowed(decoded) => Self::parse_decoded(decoded, reference),
            Cow::Owned(decoded) => {
                let parsed = Self::parse_decoded(&decoded, reference)?;
                Ok(ComponentRef {
                    section: parsed.section,
                    name: Cow::Owned(parsed.name.into_owned()),
                })
            }
        }
    }

    fn parse_decoded<'s>(
        decoded: &'s str,
        reference: &str,
    ) -> Result<ComponentRef<'s>, ResolveError> {
        let Some(pointer) = decoded.strip_prefix("#/") else {
            return Err(match decoded.split_once('#') {
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
            root if root == Section::Paths.as_pointer() => (Section::Paths, rest),
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

        Ok(ComponentRef {
            section,
            name: unescape(name),
        })
    }
}

/// Resolves `%XX` escapes, leaving anything that is not a complete hex pair
/// alone. Returns the input untouched if decoding produces invalid UTF-8.
fn percent_decode(input: &str) -> Cow<'_, str> {
    if !input.contains('%') {
        return Cow::Borrowed(input);
    }

    let mut decoded = Vec::with_capacity(input.len());
    let mut bytes = input.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            decoded.push(byte);
            continue;
        }
        let mut probe = bytes.clone();
        match (
            probe.next().and_then(hex_digit),
            probe.next().and_then(hex_digit),
        ) {
            (Some(high), Some(low)) => {
                decoded.push(high * 16 + low);
                bytes = probe;
            }
            _ => decoded.push(b'%'),
        }
    }

    match String::from_utf8(decoded) {
        Ok(decoded) => Cow::Owned(decoded),
        Err(_) => Cow::Borrowed(input),
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

/// Resolves RFC 6901 escapes in one pass. `~1` becomes `/` and `~0` becomes
/// `~`; because it is a single pass, `~01` yields `~1` rather than `/`.
fn unescape(segment: &str) -> Cow<'_, str> {
    if !segment.contains('~') {
        return Cow::Borrowed(segment);
    }

    let mut unescaped = String::with_capacity(segment.len());
    let mut characters = segment.chars();
    while let Some(character) = characters.next() {
        if character != '~' {
            unescaped.push(character);
            continue;
        }
        match characters.next() {
            Some('0') => unescaped.push('~'),
            Some('1') => unescaped.push('/'),
            Some(other) => {
                unescaped.push('~');
                unescaped.push(other);
            }
            None => unescaped.push('~'),
        }
    }

    Cow::Owned(unescaped)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn every_section_parses_back_from_its_own_pointer() {
        // Drives `Section::ALL`, so a new variant that `parse` does not handle
        // fails here rather than becoming quietly unresolvable.
        for section in Section::ALL.iter().copied() {
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
    fn borrows_the_name_when_there_is_nothing_to_decode() {
        let parsed = ComponentRef::parse("#/components/schemas/Pet").unwrap();
        assert!(matches!(parsed.name, Cow::Borrowed("Pet")));
    }

    #[test]
    fn unescapes_a_slash_in_a_path_name() {
        let parsed = ComponentRef::parse("#/paths/~1pets").unwrap();
        assert_eq!(parsed.name, "/pets");
    }

    #[test]
    fn unescapes_tilde_after_slash_so_escaped_escapes_survive() {
        // `~01` must become `~1`, not `/`.
        let parsed = ComponentRef::parse("#/components/schemas/a~01b").unwrap();
        assert_eq!(parsed.name, "a~1b");
    }

    #[test]
    fn keeps_a_trailing_tilde_that_escapes_nothing() {
        let parsed = ComponentRef::parse("#/components/schemas/a~").unwrap();
        assert_eq!(parsed.name, "a~");
    }

    #[test]
    fn percent_decodes_the_fragment_before_reading_it_as_a_pointer() {
        // `{` and `}` are not legal fragment characters, so this is the
        // conformant spelling of a reference to the path `/pets/{id}`.
        let parsed = ComponentRef::parse("#/paths/~1pets~1%7Bid%7D").unwrap();
        assert_eq!(parsed.name, "/pets/{id}");
    }

    #[test]
    fn percent_decodes_multi_byte_utf8() {
        let parsed = ComponentRef::parse("#/components/schemas/caf%C3%A9").unwrap();
        assert_eq!(parsed.name, "café");
    }

    #[test]
    fn leaves_an_incomplete_percent_escape_alone() {
        let parsed = ComponentRef::parse("#/components/schemas/100%25%zz").unwrap();
        assert_eq!(parsed.name, "100%%zz");
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
