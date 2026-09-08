//! `ReferenceOr<T>`, `ReferenceOr<Box<T>>` and their optional forms.
//!
//! The optional forms have their own method so that an absent field — which a
//! document is allowed to omit — cannot be mistaken for a broken reference.

use openapiv3::{MediaType, OpenAPI, ReferenceOr, Schema, SchemaKind, Type};
use openapiv3_resolve::{
    Resolve, ResolveError, ResolveOptionalWithOpenAPI, ResolveWithOpenAPI, Section,
};

fn spec() -> OpenAPI {
    serde_json::from_str(
        r##"{
          "openapi": "3.0.0",
          "info": { "title": "t", "version": "1" },
          "paths": {},
          "components": {
            "schemas": {
              "Pet": { "title": "Pet", "type": "string" },
              "Pets": { "type": "array", "items": { "$ref": "#/components/schemas/Pet" } }
            }
          }
        }"##,
    )
    .expect("fixture spec parses")
}

fn media_type(json: &str) -> MediaType {
    serde_json::from_str(json).expect("media type parses")
}

#[test]
fn resolves_a_boxed_reference_from_array_items() {
    let openapi = spec();
    let pets = openapi
        .resolve_ref::<Schema>("#/components/schemas/Pets")
        .expect("array schema resolves");
    let SchemaKind::Type(Type::Array(array)) = &pets.schema_kind else {
        panic!("expected an array schema");
    };
    let items = array.items.as_ref().expect("array has items");

    let resolved = items.resolve(&openapi).expect("boxed reference resolves");

    assert_eq!(resolved.schema_data.title.as_deref(), Some("Pet"));
}

#[test]
fn returns_an_inline_item_without_looking_anything_up() {
    // Resolved against a document with no `components` at all: a lookup would
    // fail with `ComponentsMissing`, so success proves none happened.
    let empty: OpenAPI = serde_json::from_str(
        r#"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{}}"#,
    )
    .expect("fixture spec parses");
    let inline: ReferenceOr<Schema> =
        serde_json::from_str(r#"{ "title": "Inline", "type": "string" }"#).expect("parses");

    let resolved = inline.resolve(&empty).expect("inline item resolves");

    assert_eq!(resolved.schema_data.title.as_deref(), Some("Inline"));
}

#[test]
fn resolves_a_plain_reference_that_is_neither_inline_nor_boxed() {
    let openapi = spec();
    let reference: ReferenceOr<Schema> =
        serde_json::from_str(r##"{ "$ref": "#/components/schemas/Pet" }"##).expect("parses");

    let resolved = reference.resolve(&openapi).expect("reference resolves");

    assert_eq!(resolved.schema_data.title.as_deref(), Some("Pet"));
}

#[test]
fn a_plain_reference_to_a_missing_name_is_an_error() {
    let openapi = spec();
    let reference: ReferenceOr<Schema> =
        serde_json::from_str(r##"{ "$ref": "#/components/schemas/Missing" }"##).expect("parses");

    assert_eq!(
        reference.resolve(&openapi),
        Err(ResolveError::NotFound {
            section: Section::Schemas,
            name: "Missing".to_owned()
        })
    );
}

#[test]
fn an_absent_optional_field_is_not_an_error() {
    let openapi = spec();
    let media = media_type("{}");

    assert_eq!(media.schema.resolve_optional(&openapi), Ok(None));
}

#[test]
fn a_present_optional_reference_resolves() {
    let openapi = spec();
    let media = media_type(r##"{ "schema": { "$ref": "#/components/schemas/Pet" } }"##);

    let resolved = media
        .schema
        .resolve_optional(&openapi)
        .expect("reference resolves")
        .expect("field is present");

    assert_eq!(resolved.schema_data.title.as_deref(), Some("Pet"));
}

#[test]
fn a_present_but_broken_optional_reference_is_an_error() {
    let openapi = spec();
    let media = media_type(r##"{ "schema": { "$ref": "#/components/schemas/Missing" } }"##);

    assert_eq!(
        media.schema.resolve_optional(&openapi),
        Err(ResolveError::NotFound {
            section: Section::Schemas,
            name: "Missing".to_owned()
        })
    );
}

#[test]
fn resolves_an_optional_boxed_reference() {
    let openapi = spec();
    let pets = openapi
        .resolve_ref::<Schema>("#/components/schemas/Pets")
        .expect("array schema resolves");
    let SchemaKind::Type(Type::Array(array)) = &pets.schema_kind else {
        panic!("expected an array schema");
    };

    let resolved = array
        .items
        .resolve_optional(&openapi)
        .expect("optional boxed reference resolves")
        .expect("items are present");

    assert_eq!(resolved.schema_data.title.as_deref(), Some("Pet"));
}

#[test]
fn a_present_but_broken_optional_boxed_reference_is_an_error() {
    let openapi = spec();
    let broken: Schema = serde_json::from_str(
        r##"{ "type": "array", "items": { "$ref": "#/components/schemas/Missing" } }"##,
    )
    .expect("schema parses");
    let SchemaKind::Type(Type::Array(array)) = &broken.schema_kind else {
        panic!("expected an array schema");
    };

    assert_eq!(
        array.items.resolve_optional(&openapi),
        Err(ResolveError::NotFound {
            section: Section::Schemas,
            name: "Missing".to_owned()
        })
    );
}
