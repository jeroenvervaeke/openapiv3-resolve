//! End-to-end tests for [`openapiv3_resolve::resolve`].

use indexmap::IndexMap;
use openapiv3::*;
use openapiv3_resolve::{
    OpenAPIExt, ResolveError, ResolvedAdditionalProperties, ResolvedRefOr, ResolvedSchemaKind,
    ResolvedType,
};

fn schema_item(title: &str, kind: SchemaKind) -> ReferenceOr<Schema> {
    ReferenceOr::Item(Schema {
        schema_data: SchemaData {
            title: Some(title.to_string()),
            ..Default::default()
        },
        schema_kind: kind,
    })
}

fn schema_ref(name: &str) -> ReferenceOr<Schema> {
    ReferenceOr::Reference {
        reference: format!("#/components/schemas/{name}"),
    }
}

fn boxed_schema_ref(name: &str) -> ReferenceOr<Box<Schema>> {
    ReferenceOr::Reference {
        reference: format!("#/components/schemas/{name}"),
    }
}

fn string_kind() -> SchemaKind {
    SchemaKind::Type(Type::String(StringType::default()))
}

#[test]
fn resolves_simple_inline_and_reference() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("Pet".to_string(), schema_item("Pet", string_kind())),
                (
                    "AlsoPet".to_string(),
                    schema_ref("Pet"), // alias chain
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");

    let pet = resolved.components.schemas.get("Pet").unwrap();
    assert_eq!(pet.schema_data.title.as_deref(), Some("Pet"));

    let also = resolved.components.schemas.get("AlsoPet").unwrap();
    // Alias entries share the same allocation as their target.
    assert!(openapiv3_resolve::Resolved::ptr_eq(pet, also));
    assert_eq!(also.schema_data.title.as_deref(), Some("Pet"));
}

#[test]
fn resolves_self_cycle_via_weak_ref() {
    // Tree { children: [Tree] }
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([(
                "Tree".to_string(),
                schema_item(
                    "Tree",
                    SchemaKind::Type(Type::Object(ObjectType {
                        properties: IndexMap::from([(
                            "children".to_string(),
                            ReferenceOr::Item(Box::new(Schema {
                                schema_data: SchemaData::default(),
                                schema_kind: SchemaKind::Type(Type::Array(ArrayType {
                                    items: Some(boxed_schema_ref("Tree")),
                                    min_items: None,
                                    max_items: None,
                                    unique_items: false,
                                })),
                            })),
                        )]),
                        ..Default::default()
                    })),
                ),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let tree = resolved.components.schemas.get("Tree").unwrap();

    let ResolvedSchemaKind::Type(ResolvedType::Object(obj)) = &tree.schema_kind else {
        panic!("expected object");
    };
    let children = obj.properties.get("children").unwrap();
    let ResolvedRefOr::Item(children_inline) = children else {
        panic!("children defined inline");
    };
    let ResolvedSchemaKind::Type(ResolvedType::Array(arr)) = &children_inline.schema_kind else {
        panic!("expected array");
    };
    let items_ref = arr.items.as_ref().expect("items present");
    let ResolvedRefOr::Reference(weak) = items_ref else {
        panic!("items should be a $ref-style weak handle");
    };
    let upgraded = weak.upgrade().expect("Tree is alive via components map");
    assert!(openapiv3_resolve::Resolved::ptr_eq(tree, &upgraded));
}

#[test]
fn resolves_mutual_cycle_between_two_components() {
    // A.next -> B, B.next -> A
    let object_with_next = |next_to: &str| {
        ReferenceOr::Item(Schema {
            schema_data: SchemaData::default(),
            schema_kind: SchemaKind::Type(Type::Object(ObjectType {
                properties: IndexMap::from([("next".to_string(), boxed_schema_ref(next_to))]),
                ..Default::default()
            })),
        })
    };

    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("A".to_string(), object_with_next("B")),
                ("B".to_string(), object_with_next("A")),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");

    let a = resolved.components.schemas.get("A").unwrap();
    let b = resolved.components.schemas.get("B").unwrap();

    let next_of = |s: &openapiv3_resolve::ResolvedSchema| {
        let ResolvedSchemaKind::Type(ResolvedType::Object(o)) = &s.schema_kind else {
            panic!("expected object");
        };
        match o.properties.get("next").unwrap() {
            ResolvedRefOr::Reference(w) => w.upgrade().unwrap(),
            _ => panic!("expected $ref"),
        }
    };

    let a_next = next_of(a);
    let b_next = next_of(b);
    assert!(openapiv3_resolve::Resolved::ptr_eq(&a_next, b));
    assert!(openapiv3_resolve::Resolved::ptr_eq(&b_next, a));
}

#[test]
fn unresolved_reference_returns_error() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([(
                "User".to_string(),
                schema_item(
                    "User",
                    SchemaKind::Type(Type::Object(ObjectType {
                        properties: IndexMap::from([(
                            "missing".to_string(),
                            boxed_schema_ref("DoesNotExist"),
                        )]),
                        ..Default::default()
                    })),
                ),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let err = openapi.resolve_all().expect_err("should error");
    match err {
        ResolveError::UnresolvedReference { reference } => {
            assert_eq!(reference, "#/components/schemas/DoesNotExist");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn ref_chain_cycle_in_components_returns_error() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("Foo".to_string(), schema_ref("Bar")),
                ("Bar".to_string(), schema_ref("Foo")),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let err = openapi.resolve_all().expect_err("should error");
    assert!(
        matches!(err, ResolveError::ReferenceCycle { .. }),
        "expected ReferenceCycle, got {err:?}"
    );
}

#[test]
fn external_ref_is_unsupported() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([(
                "Outer".to_string(),
                schema_item(
                    "Outer",
                    SchemaKind::Type(Type::Object(ObjectType {
                        properties: IndexMap::from([(
                            "external".to_string(),
                            ReferenceOr::Reference {
                                reference: "external.yaml#/components/schemas/X".to_string(),
                            },
                        )]),
                        ..Default::default()
                    })),
                ),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let err = openapi.resolve_all().expect_err("should error");
    assert!(
        matches!(err, ResolveError::UnsupportedReference { .. }),
        "expected UnsupportedReference, got {err:?}"
    );
}

#[test]
fn wrong_kind_ref_is_rejected() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([(
                "Outer".to_string(),
                schema_item(
                    "Outer",
                    SchemaKind::Type(Type::Object(ObjectType {
                        properties: IndexMap::from([(
                            "wrong".to_string(),
                            ReferenceOr::Reference {
                                reference: "#/components/responses/Foo".to_string(),
                            },
                        )]),
                        ..Default::default()
                    })),
                ),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let err = openapi.resolve_all().expect_err("should error");
    match err {
        ResolveError::WrongReferenceKind {
            expected, found, ..
        } => {
            assert_eq!(expected, "schemas");
            assert_eq!(found, "responses");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn resolves_response_with_referenced_header_and_schema() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/pets".to_string(),
                ReferenceOr::Item(PathItem {
                    get: Some(Operation {
                        responses: Responses {
                            responses: IndexMap::from([(
                                StatusCode::Code(200),
                                ReferenceOr::Reference {
                                    reference: "#/components/responses/PetList".to_string(),
                                },
                            )]),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        components: Some(Components {
            schemas: IndexMap::from([("Pet".to_string(), schema_item("Pet", string_kind()))]),
            headers: IndexMap::from([(
                "X-Total".to_string(),
                ReferenceOr::Item(Header {
                    description: Some("total count".to_string()),
                    style: HeaderStyle::Simple,
                    required: false,
                    deprecated: None,
                    format: ParameterSchemaOrContent::Schema(ReferenceOr::Item(Schema {
                        schema_data: SchemaData::default(),
                        schema_kind: SchemaKind::Type(Type::Integer(IntegerType::default())),
                    })),
                    example: None,
                    examples: IndexMap::new(),
                    extensions: IndexMap::new(),
                }),
            )]),
            responses: IndexMap::from([(
                "PetList".to_string(),
                ReferenceOr::Item(Response {
                    description: "a list of pets".to_string(),
                    headers: IndexMap::from([(
                        "X-Total".to_string(),
                        ReferenceOr::Reference {
                            reference: "#/components/headers/X-Total".to_string(),
                        },
                    )]),
                    content: IndexMap::from([(
                        "application/json".to_string(),
                        MediaType {
                            schema: Some(schema_ref("Pet")),
                            ..Default::default()
                        },
                    )]),
                    ..Default::default()
                }),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let path = resolved.paths.paths.get("/pets").unwrap();
    let op = path.get.as_ref().unwrap();
    let resp_ref = op.responses.responses.get(&StatusCode::Code(200)).unwrap();
    let resp = match resp_ref {
        ResolvedRefOr::Reference(w) => w.upgrade().unwrap(),
        _ => panic!("expected $ref"),
    };
    assert_eq!(resp.description, "a list of pets");

    let header_ref = resp.headers.get("X-Total").unwrap();
    let ResolvedRefOr::Reference(weak_header) = header_ref else {
        panic!("expected $ref header");
    };
    let header = weak_header.upgrade().unwrap();
    assert_eq!(header.description.as_deref(), Some("total count"));

    let schema_ref = resp
        .content
        .get("application/json")
        .unwrap()
        .schema
        .as_ref()
        .unwrap();
    let ResolvedRefOr::Reference(weak_schema) = schema_ref else {
        panic!("expected $ref schema");
    };
    let pet = weak_schema.upgrade().unwrap();
    assert_eq!(pet.schema_data.title.as_deref(), Some("Pet"));
}

#[test]
fn resolves_additional_properties_schema_box() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("Value".to_string(), schema_item("Value", string_kind())),
                (
                    "Bag".to_string(),
                    schema_item(
                        "Bag",
                        SchemaKind::Type(Type::Object(ObjectType {
                            additional_properties: Some(AdditionalProperties::Schema(Box::new(
                                schema_ref("Value"),
                            ))),
                            ..Default::default()
                        })),
                    ),
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let bag = resolved.components.schemas.get("Bag").unwrap();
    let ResolvedSchemaKind::Type(ResolvedType::Object(obj)) = &bag.schema_kind else {
        panic!("expected object");
    };
    let ap = obj.additional_properties.as_ref().expect("present");
    let ResolvedAdditionalProperties::Schema(ResolvedRefOr::Reference(weak)) = ap else {
        panic!("expected schema $ref");
    };
    let value = weak.upgrade().unwrap();
    assert_eq!(value.schema_data.title.as_deref(), Some("Value"));
}

#[test]
fn path_item_reference_is_rejected() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/pets".to_string(),
                ReferenceOr::Reference {
                    reference: "external.yaml#/paths/~1pets".to_string(),
                },
            )]),
            extensions: IndexMap::new(),
        },
        ..Default::default()
    };

    let err = openapi.resolve_all().expect_err("should error");
    assert!(matches!(err, ResolveError::PathItemReference { .. }));
}
