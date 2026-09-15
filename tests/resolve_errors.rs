//! Each way a reference can fail gets its own error, not a bare "not found".

mod common;

use common::spec;
use openapiv3::{OpenAPI, Schema};
use openapiv3_resolve::{Resolve, ResolveError, Section};

fn spec_without_components() -> OpenAPI {
    serde_json::from_str(r#"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{}}"#)
        .expect("fixture spec parses")
}

fn schema_error(openapi: &OpenAPI, reference: &str) -> ResolveError {
    openapi
        .resolve_ref::<Schema>(reference)
        .expect_err("expected this reference to fail")
}

#[test]
fn reports_a_reference_that_is_not_a_pointer() {
    assert_eq!(
        schema_error(&spec(), "Pet"),
        ResolveError::NotALocalReference {
            reference: "Pet".to_owned()
        }
    );
}

#[test]
fn reports_a_reference_into_another_document() {
    assert_eq!(
        schema_error(&spec(), "common.yaml#/components/schemas/Pet"),
        ResolveError::ExternalDocument {
            document: "common.yaml".to_owned()
        }
    );
}

#[test]
fn reports_a_pointer_that_names_no_component() {
    assert_eq!(
        schema_error(&spec(), "#/components/schemas"),
        ResolveError::MalformedPointer {
            reference: "#/components/schemas".to_owned()
        }
    );
}

#[test]
fn reports_a_root_section_that_holds_no_targets() {
    assert_eq!(
        schema_error(&spec(), "#/info/title"),
        ResolveError::UnsupportedRootSection {
            section: "info".to_owned()
        }
    );
}

#[test]
fn reports_the_rust_field_name_as_an_unknown_section() {
    assert_eq!(
        schema_error(&spec(), "#/components/request_bodies/CreatePet"),
        ResolveError::UnknownSection {
            section: "request_bodies".to_owned()
        }
    );
}

#[test]
fn reports_a_document_without_components() {
    assert_eq!(
        schema_error(&spec_without_components(), "#/components/schemas/Pet"),
        ResolveError::ComponentsMissing {
            section: Section::Schemas
        }
    );
}

#[test]
fn reports_a_name_the_section_does_not_contain() {
    assert_eq!(
        schema_error(&spec(), "#/components/schemas/Missing"),
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Missing".to_owned()
        }
    );
}

#[test]
fn reports_a_reference_into_the_wrong_section() {
    // The target exists and is perfectly valid — it is just not a schema.
    assert_eq!(
        schema_error(&spec(), "#/components/responses/PetList"),
        ResolveError::SectionMismatch {
            expected: Section::Schemas,
            found: Section::Responses
        }
    );
}

#[test]
fn reports_a_pointer_into_the_middle_of_a_component() {
    let reference = "#/components/schemas/Pet/properties/name";
    assert_eq!(
        schema_error(&spec(), reference),
        ResolveError::PointerTooDeep {
            reference: reference.to_owned()
        }
    );
}

#[test]
fn every_error_renders_a_distinct_message_naming_its_cause() {
    let errors = [
        ResolveError::NotALocalReference {
            reference: "Pet".to_owned(),
        },
        ResolveError::ExternalDocument {
            document: "c.yaml".to_owned(),
        },
        ResolveError::MalformedPointer {
            reference: "#/x".to_owned(),
        },
        ResolveError::UnsupportedRootSection {
            section: "info".to_owned(),
        },
        ResolveError::UnknownSection {
            section: "request_bodies".to_owned(),
        },
        ResolveError::ComponentsMissing {
            section: Section::Schemas,
        },
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Pet".to_owned(),
        },
        ResolveError::SectionMismatch {
            expected: Section::Schemas,
            found: Section::Responses,
        },
        ResolveError::PointerTooDeep {
            reference: "#/a/b/c/d".to_owned(),
        },
        ResolveError::ReferenceChainTooLong {
            reference: "#/components/schemas/A".to_owned(),
            last: "#/components/schemas/B".to_owned(),
            max_hops: 100,
        },
        ResolveError::CyclicReference {
            reference: "#/components/schemas/Node".to_owned(),
        },
        ResolveError::DiscriminatorMappingMismatch {
            property_name: "kind".to_owned(),
            value: "cat".to_owned(),
            schema: "Cat".to_owned(),
        },
    ];

    let mut rendered: Vec<String> = Vec::new();
    for error in &errors {
        let message = error.to_string();
        let subject = match error {
            ResolveError::NotALocalReference { reference } => reference,
            ResolveError::ExternalDocument { document } => document,
            ResolveError::MalformedPointer { reference } => reference,
            ResolveError::UnsupportedRootSection { section } => section,
            ResolveError::UnknownSection { section } => section,
            ResolveError::ComponentsMissing { .. } => "components/schemas",
            ResolveError::NotFound { name, .. } => name,
            ResolveError::SectionMismatch { .. } => "components/responses",
            ResolveError::PointerTooDeep { reference } => reference,
            ResolveError::ReferenceChainTooLong { last, .. } => last,
            ResolveError::CyclicReference { reference } => reference,
            ResolveError::DiscriminatorMappingMismatch { schema, .. } => schema,
            _ => panic!("unhandled variant: {error:?}"),
        };
        assert!(
            message.contains(subject),
            "{message:?} does not name {subject:?}"
        );
        let _: &dyn std::error::Error = error;
        rendered.push(message);
    }

    let mut distinct = rendered.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        rendered.len(),
        "two variants render the same message"
    );
}
