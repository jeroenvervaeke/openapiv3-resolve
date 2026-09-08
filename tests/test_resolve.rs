use indexmap::IndexMap;
use openapiv3::{
    ArrayType, Components, ObjectType, OpenAPI, ReferenceOr, Schema, SchemaData, SchemaKind, Type,
};
use openapiv3_resolve::ResolveWithOpenAPI;

/// Verifies that `ReferenceOr<Box<T>>` (e.g. `ArrayType::items`) can be
/// resolved against the document.
#[test]
fn test_resolve_boxed_array_items() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                (
                    "Pets".to_string(),
                    ReferenceOr::Item(Schema {
                        schema_data: SchemaData {
                            title: Some("Pets".to_string()),
                            ..Default::default()
                        },
                        schema_kind: SchemaKind::Type(Type::Array(ArrayType {
                            items: Some(ReferenceOr::Reference {
                                reference: "#/components/schemas/Pet".to_string(),
                            }),
                            min_items: None,
                            max_items: None,
                            unique_items: false,
                        })),
                    }),
                ),
                (
                    "Pet".to_string(),
                    ReferenceOr::Item(Schema {
                        schema_data: SchemaData {
                            title: Some("Pet".to_string()),
                            ..Default::default()
                        },
                        schema_kind: SchemaKind::Type(Type::Object(ObjectType {
                            properties: IndexMap::from([(
                                "name".to_string(),
                                ReferenceOr::boxed_item(Schema {
                                    schema_data: Default::default(),
                                    schema_kind: SchemaKind::Type(Type::String(Default::default())),
                                }),
                            )]),
                            required: vec!["name".to_string()],
                            ..Default::default()
                        })),
                    }),
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let pets = openapi
        .components
        .as_ref()
        .unwrap()
        .schemas
        .get("Pets")
        .expect("Pets schema present")
        .as_item()
        .expect("Pets is inline");

    let SchemaKind::Type(Type::Array(ArrayType { items, .. })) = &pets.schema_kind else {
        panic!("expected an array");
    };

    let pet = items
        .resolve(&openapi)
        .expect("should resolve array items via ReferenceOr<Box<Schema>>");

    assert_eq!(pet.schema_data.title.as_deref(), Some("Pet"));
    let SchemaKind::Type(Type::Object(ObjectType { properties, .. })) = &pet.schema_kind else {
        panic!("expected an object");
    };
    assert!(properties.contains_key("name"));
}
