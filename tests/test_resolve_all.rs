//! End-to-end tests for [`openapiv3_resolve::resolve`].

use indexmap::IndexMap;
use openapiv3::*;
use openapiv3_resolve::{
    OpenAPIExt, ResolveError, Resolved, ResolvedAdditionalProperties, ResolvedParameter,
    ResolvedParameterSchemaOrContent, ResolvedRefOr, ResolvedSchemaKind, ResolvedType,
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

// ---------------------------------------------------------------------------
// Component-kind coverage: parameters / request bodies / examples / links /
// security schemes / callbacks / encoding-with-header
// ---------------------------------------------------------------------------

fn integer_schema() -> ReferenceOr<Schema> {
    ReferenceOr::Item(Schema {
        schema_data: SchemaData::default(),
        schema_kind: SchemaKind::Type(Type::Integer(IntegerType::default())),
    })
}

fn query_param(name: &str) -> Parameter {
    Parameter::Query {
        parameter_data: ParameterData {
            name: name.to_string(),
            description: None,
            required: false,
            deprecated: None,
            format: ParameterSchemaOrContent::Schema(integer_schema()),
            example: None,
            examples: IndexMap::new(),
            explode: None,
            extensions: IndexMap::new(),
        },
        allow_reserved: false,
        style: QueryStyle::Form,
        allow_empty_value: None,
    }
}

#[test]
fn resolves_query_parameter_reference() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/foo".to_string(),
                ReferenceOr::Item(PathItem {
                    get: Some(Operation {
                        parameters: vec![ReferenceOr::Reference {
                            reference: "#/components/parameters/Limit".to_string(),
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        components: Some(Components {
            parameters: IndexMap::from([(
                "Limit".to_string(),
                ReferenceOr::Item(query_param("limit")),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let op = resolved
        .paths
        .paths
        .get("/foo")
        .unwrap()
        .get
        .as_ref()
        .unwrap();
    let ResolvedRefOr::Reference(w) = &op.parameters[0] else {
        panic!("expected $ref");
    };
    let p = w.upgrade().unwrap();
    let ResolvedParameter::Query { parameter_data, .. } = &*p else {
        panic!("expected query");
    };
    assert_eq!(parameter_data.name, "limit");

    // The inner schema inside the parameter is inline; check it survived.
    let ResolvedParameterSchemaOrContent::Schema(ResolvedRefOr::Item(schema)) =
        &parameter_data.format
    else {
        panic!("expected inline schema");
    };
    assert!(matches!(
        schema.schema_kind,
        ResolvedSchemaKind::Type(ResolvedType::Integer(_))
    ));
}

#[test]
fn resolves_request_body_reference() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/foo".to_string(),
                ReferenceOr::Item(PathItem {
                    post: Some(Operation {
                        request_body: Some(ReferenceOr::Reference {
                            reference: "#/components/requestBodies/Body".to_string(),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        components: Some(Components {
            request_bodies: IndexMap::from([(
                "Body".to_string(),
                ReferenceOr::Item(RequestBody {
                    description: Some("the body".to_string()),
                    required: true,
                    content: IndexMap::new(),
                    extensions: IndexMap::new(),
                }),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let op = resolved
        .paths
        .paths
        .get("/foo")
        .unwrap()
        .post
        .as_ref()
        .unwrap();
    let ResolvedRefOr::Reference(w) = op.request_body.as_ref().unwrap() else {
        panic!("expected $ref");
    };
    let body = w.upgrade().unwrap();
    assert_eq!(body.description.as_deref(), Some("the body"));
    assert!(body.required);
}

#[test]
fn resolves_example_reference_in_media_type() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/foo".to_string(),
                ReferenceOr::Item(PathItem {
                    get: Some(Operation {
                        responses: Responses {
                            responses: IndexMap::from([(
                                StatusCode::Code(200),
                                ReferenceOr::Item(Response {
                                    description: "ok".to_string(),
                                    content: IndexMap::from([(
                                        "application/json".to_string(),
                                        MediaType {
                                            examples: IndexMap::from([(
                                                "default".to_string(),
                                                ReferenceOr::Reference {
                                                    reference: "#/components/examples/Hello"
                                                        .to_string(),
                                                },
                                            )]),
                                            ..Default::default()
                                        },
                                    )]),
                                    ..Default::default()
                                }),
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
            examples: IndexMap::from([(
                "Hello".to_string(),
                ReferenceOr::Item(Example {
                    summary: Some("hi".to_string()),
                    description: None,
                    value: Some(serde_json::json!("hello")),
                    external_value: None,
                    extensions: IndexMap::new(),
                }),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let op = resolved
        .paths
        .paths
        .get("/foo")
        .unwrap()
        .get
        .as_ref()
        .unwrap();
    let ResolvedRefOr::Item(resp) = op.responses.responses.get(&StatusCode::Code(200)).unwrap()
    else {
        panic!()
    };
    let mt = resp.content.get("application/json").unwrap();
    let ResolvedRefOr::Reference(w) = mt.examples.get("default").unwrap() else {
        panic!("expected $ref");
    };
    let ex = w.upgrade().unwrap();
    assert_eq!(ex.summary.as_deref(), Some("hi"));
    assert_eq!(ex.value.as_ref().unwrap(), &serde_json::json!("hello"));
}

#[test]
fn resolves_link_reference_in_response() {
    let openapi = OpenAPI {
        components: Some(Components {
            responses: IndexMap::from([(
                "WithLinks".to_string(),
                ReferenceOr::Item(Response {
                    description: "rsp".to_string(),
                    links: IndexMap::from([(
                        "self".to_string(),
                        ReferenceOr::Reference {
                            reference: "#/components/links/Self".to_string(),
                        },
                    )]),
                    ..Default::default()
                }),
            )]),
            links: IndexMap::from([(
                "Self".to_string(),
                ReferenceOr::Item(Link {
                    description: Some("the link".to_string()),
                    operation: LinkOperation::OperationId("getThing".to_string()),
                    request_body: None,
                    parameters: IndexMap::new(),
                    server: None,
                    extensions: IndexMap::new(),
                }),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let resp = resolved.components.responses.get("WithLinks").unwrap();
    let ResolvedRefOr::Reference(w) = resp.links.get("self").unwrap() else {
        panic!("expected $ref");
    };
    let l = w.upgrade().unwrap();
    assert_eq!(l.description.as_deref(), Some("the link"));
}

#[test]
fn security_scheme_alias_chain_resolves() {
    let openapi = OpenAPI {
        components: Some(Components {
            security_schemes: IndexMap::from([
                (
                    "Primary".to_string(),
                    ReferenceOr::Item(SecurityScheme::APIKey {
                        location: APIKeyLocation::Header,
                        name: "X-API-Key".to_string(),
                        description: Some("primary".to_string()),
                        extensions: IndexMap::new(),
                    }),
                ),
                (
                    "Alias".to_string(),
                    ReferenceOr::Reference {
                        reference: "#/components/securitySchemes/Primary".to_string(),
                    },
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let primary = resolved.components.security_schemes.get("Primary").unwrap();
    let alias = resolved.components.security_schemes.get("Alias").unwrap();
    assert!(Resolved::ptr_eq(primary, alias));
    assert!(matches!(&**primary, SecurityScheme::APIKey { .. }));
}

#[test]
fn callback_alias_chain_resolves() {
    let mut cb: Callback = IndexMap::new();
    cb.insert("{$req}".to_string(), PathItem::default());

    let openapi = OpenAPI {
        components: Some(Components {
            callbacks: IndexMap::from([
                ("Original".to_string(), ReferenceOr::Item(cb)),
                (
                    "Alias".to_string(),
                    ReferenceOr::Reference {
                        reference: "#/components/callbacks/Original".to_string(),
                    },
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let orig = resolved.components.callbacks.get("Original").unwrap();
    let alias = resolved.components.callbacks.get("Alias").unwrap();
    assert!(Resolved::ptr_eq(orig, alias));
    assert!(orig.contains_key("{$req}"));
}

#[test]
fn resolves_encoding_with_referenced_header() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/upload".to_string(),
                ReferenceOr::Item(PathItem {
                    post: Some(Operation {
                        request_body: Some(ReferenceOr::Item(RequestBody {
                            content: IndexMap::from([(
                                "multipart/form-data".to_string(),
                                MediaType {
                                    encoding: IndexMap::from([(
                                        "file".to_string(),
                                        Encoding {
                                            headers: IndexMap::from([(
                                                "X-Trace".to_string(),
                                                ReferenceOr::Reference {
                                                    reference: "#/components/headers/Trace"
                                                        .to_string(),
                                                },
                                            )]),
                                            ..Default::default()
                                        },
                                    )]),
                                    ..Default::default()
                                },
                            )]),
                            ..Default::default()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        components: Some(Components {
            headers: IndexMap::from([(
                "Trace".to_string(),
                ReferenceOr::Item(Header {
                    description: Some("trace id".to_string()),
                    style: HeaderStyle::Simple,
                    required: false,
                    deprecated: None,
                    format: ParameterSchemaOrContent::Schema(ReferenceOr::Item(Schema {
                        schema_data: SchemaData::default(),
                        schema_kind: SchemaKind::Type(Type::String(StringType::default())),
                    })),
                    example: None,
                    examples: IndexMap::new(),
                    extensions: IndexMap::new(),
                }),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let op = resolved
        .paths
        .paths
        .get("/upload")
        .unwrap()
        .post
        .as_ref()
        .unwrap();
    let ResolvedRefOr::Item(rb) = op.request_body.as_ref().unwrap() else {
        panic!("expected inline body");
    };
    let mt = rb.content.get("multipart/form-data").unwrap();
    let enc = mt.encoding.get("file").unwrap();
    let ResolvedRefOr::Reference(w) = enc.headers.get("X-Trace").unwrap() else {
        panic!("expected $ref header");
    };
    let h = w.upgrade().unwrap();
    assert_eq!(h.description.as_deref(), Some("trace id"));
}

// ---------------------------------------------------------------------------
// Schema variant coverage: OneOf/AllOf/AnyOf, Not, AnySchema,
// AdditionalProperties::Any
// ---------------------------------------------------------------------------

#[test]
fn resolves_one_of_all_of_any_of_mixed_inline_and_ref() {
    let make = |kind: SchemaKind| {
        ReferenceOr::Item(Schema {
            schema_data: SchemaData::default(),
            schema_kind: kind,
        })
    };

    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("Cat".to_string(), schema_item("Cat", string_kind())),
                (
                    "OneOfAnimal".to_string(),
                    make(SchemaKind::OneOf {
                        one_of: vec![schema_item("InlineDog", string_kind()), schema_ref("Cat")],
                    }),
                ),
                (
                    "AllOfAnimal".to_string(),
                    make(SchemaKind::AllOf {
                        all_of: vec![schema_ref("Cat"), schema_item("InlineMixin", string_kind())],
                    }),
                ),
                (
                    "AnyOfAnimal".to_string(),
                    make(SchemaKind::AnyOf {
                        any_of: vec![schema_ref("Cat")],
                    }),
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");

    let one = resolved.components.schemas.get("OneOfAnimal").unwrap();
    let ResolvedSchemaKind::OneOf { one_of } = &one.schema_kind else {
        panic!()
    };
    assert!(matches!(one_of[0], ResolvedRefOr::Item(_)));
    assert!(matches!(one_of[1], ResolvedRefOr::Reference(_)));

    let all = resolved.components.schemas.get("AllOfAnimal").unwrap();
    let ResolvedSchemaKind::AllOf { all_of } = &all.schema_kind else {
        panic!()
    };
    assert!(matches!(all_of[0], ResolvedRefOr::Reference(_)));
    assert!(matches!(all_of[1], ResolvedRefOr::Item(_)));

    let any = resolved.components.schemas.get("AnyOfAnimal").unwrap();
    let ResolvedSchemaKind::AnyOf { any_of } = &any.schema_kind else {
        panic!()
    };
    assert_eq!(any_of.len(), 1);
    assert!(matches!(any_of[0], ResolvedRefOr::Reference(_)));
}

#[test]
fn resolves_schema_not() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                (
                    "Forbidden".to_string(),
                    schema_item("Forbidden", string_kind()),
                ),
                (
                    "NotForbidden".to_string(),
                    ReferenceOr::Item(Schema {
                        schema_data: SchemaData::default(),
                        schema_kind: SchemaKind::Not {
                            not: Box::new(ReferenceOr::Reference {
                                reference: "#/components/schemas/Forbidden".to_string(),
                            }),
                        },
                    }),
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let nf = resolved.components.schemas.get("NotForbidden").unwrap();
    let ResolvedSchemaKind::Not { not } = &nf.schema_kind else {
        panic!()
    };
    let ResolvedRefOr::Reference(w) = not else {
        panic!("expected $ref")
    };
    let forbidden = resolved.components.schemas.get("Forbidden").unwrap();
    assert!(Resolved::ptr_eq(&w.upgrade().unwrap(), forbidden));
}

#[test]
fn resolves_any_schema_with_internal_ref() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("Pet".to_string(), schema_item("Pet", string_kind())),
                (
                    "Anything".to_string(),
                    ReferenceOr::Item(Schema {
                        schema_data: SchemaData::default(),
                        schema_kind: SchemaKind::Any(AnySchema {
                            properties: IndexMap::from([(
                                "pet".to_string(),
                                boxed_schema_ref("Pet"),
                            )]),
                            one_of: vec![schema_ref("Pet")],
                            ..Default::default()
                        }),
                    }),
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let any = resolved.components.schemas.get("Anything").unwrap();
    let ResolvedSchemaKind::Any(a) = &any.schema_kind else {
        panic!()
    };
    let ResolvedRefOr::Reference(w) = a.properties.get("pet").unwrap() else {
        panic!()
    };
    let pet = resolved.components.schemas.get("Pet").unwrap();
    assert!(Resolved::ptr_eq(&w.upgrade().unwrap(), pet));
    assert_eq!(a.one_of.len(), 1);
}

#[test]
fn resolves_additional_properties_any_variant() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                (
                    "OpenTrue".to_string(),
                    schema_item(
                        "OpenTrue",
                        SchemaKind::Type(Type::Object(ObjectType {
                            additional_properties: Some(AdditionalProperties::Any(true)),
                            ..Default::default()
                        })),
                    ),
                ),
                (
                    "OpenFalse".to_string(),
                    schema_item(
                        "OpenFalse",
                        SchemaKind::Type(Type::Object(ObjectType {
                            additional_properties: Some(AdditionalProperties::Any(false)),
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
    let assert_kind = |name: &str, expected: bool| {
        let s = resolved.components.schemas.get(name).unwrap();
        let ResolvedSchemaKind::Type(ResolvedType::Object(o)) = &s.schema_kind else {
            panic!()
        };
        assert!(matches!(
            o.additional_properties,
            Some(ResolvedAdditionalProperties::Any(b)) if b == expected
        ));
    };
    assert_kind("OpenTrue", true);
    assert_kind("OpenFalse", false);
}

// ---------------------------------------------------------------------------
// Multi-hop alias chain & shared component identity across many sites
// ---------------------------------------------------------------------------

#[test]
fn multi_hop_alias_chain_resolves() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("A".to_string(), schema_ref("B")),
                ("B".to_string(), schema_ref("C")),
                ("C".to_string(), schema_item("C", string_kind())),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let a = resolved.components.schemas.get("A").unwrap();
    let b = resolved.components.schemas.get("B").unwrap();
    let c = resolved.components.schemas.get("C").unwrap();
    assert!(Resolved::ptr_eq(a, c));
    assert!(Resolved::ptr_eq(b, c));
    assert_eq!(c.schema_data.title.as_deref(), Some("C"));
}

#[test]
fn shared_component_yields_one_allocation_for_many_sites() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/pets".to_string(),
                ReferenceOr::Item(PathItem {
                    get: Some(Operation {
                        parameters: vec![ReferenceOr::Item(Parameter::Query {
                            parameter_data: ParameterData {
                                name: "filter".to_string(),
                                description: None,
                                required: false,
                                deprecated: None,
                                format: ParameterSchemaOrContent::Schema(schema_ref("Pet")),
                                example: None,
                                examples: IndexMap::new(),
                                explode: None,
                                extensions: IndexMap::new(),
                            },
                            allow_reserved: false,
                            style: QueryStyle::Form,
                            allow_empty_value: None,
                        })],
                        responses: Responses {
                            responses: IndexMap::from([(
                                StatusCode::Code(200),
                                ReferenceOr::Item(Response {
                                    description: "ok".to_string(),
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
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        components: Some(Components {
            schemas: IndexMap::from([
                ("Pet".to_string(), schema_item("Pet", string_kind())),
                (
                    "Wrapper".to_string(),
                    schema_item(
                        "Wrapper",
                        SchemaKind::Type(Type::Object(ObjectType {
                            properties: IndexMap::from([(
                                "pet".to_string(),
                                boxed_schema_ref("Pet"),
                            )]),
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
    let pet = resolved.components.schemas.get("Pet").unwrap();

    // Site 1: Wrapper.pet
    let wrapper = resolved.components.schemas.get("Wrapper").unwrap();
    let ResolvedSchemaKind::Type(ResolvedType::Object(o)) = &wrapper.schema_kind else {
        panic!()
    };
    let ResolvedRefOr::Reference(w) = o.properties.get("pet").unwrap() else {
        panic!()
    };
    assert!(Resolved::ptr_eq(pet, &w.upgrade().unwrap()));

    // Sites 2 & 3: parameter schema and response content schema reach
    // through the resolved tree.
    let op = resolved
        .paths
        .paths
        .get("/pets")
        .unwrap()
        .get
        .as_ref()
        .unwrap();
    let ResolvedRefOr::Item(param) = &op.parameters[0] else {
        panic!()
    };
    let ResolvedParameter::Query { parameter_data, .. } = &**param else {
        panic!()
    };
    let ResolvedParameterSchemaOrContent::Schema(ResolvedRefOr::Reference(w_param)) =
        &parameter_data.format
    else {
        panic!()
    };
    assert!(Resolved::ptr_eq(pet, &w_param.upgrade().unwrap()));

    let ResolvedRefOr::Item(resp) = op.responses.responses.get(&StatusCode::Code(200)).unwrap()
    else {
        panic!()
    };
    let mt = resp.content.get("application/json").unwrap();
    let ResolvedRefOr::Reference(w_mt) = mt.schema.as_ref().unwrap() else {
        panic!()
    };
    assert!(Resolved::ptr_eq(pet, &w_mt.upgrade().unwrap()));
}

// ---------------------------------------------------------------------------
// Lifetime invariant: dropping the resolved doc invalidates outstanding weaks
// ---------------------------------------------------------------------------

#[test]
fn dropping_resolved_invalidates_weak_references() {
    let openapi = OpenAPI {
        components: Some(Components {
            schemas: IndexMap::from([
                ("Pet".to_string(), schema_item("Pet", string_kind())),
                (
                    "Holder".to_string(),
                    schema_item(
                        "Holder",
                        SchemaKind::Type(Type::Object(ObjectType {
                            properties: IndexMap::from([(
                                "pet".to_string(),
                                boxed_schema_ref("Pet"),
                            )]),
                            ..Default::default()
                        })),
                    ),
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().unwrap();
    let weak = {
        let holder = resolved.components.schemas.get("Holder").unwrap();
        let ResolvedSchemaKind::Type(ResolvedType::Object(o)) = &holder.schema_kind else {
            panic!()
        };
        let ResolvedRefOr::Reference(w) = o.properties.get("pet").unwrap() else {
            panic!()
        };
        w.clone()
    };

    assert!(weak.upgrade().is_some(), "weak upgrades while doc is alive");
    drop(resolved);
    assert!(
        weak.upgrade().is_none(),
        "weak fails to upgrade after doc drop"
    );
}

// ---------------------------------------------------------------------------
// Empty inputs, malformed refs, extension preservation
// ---------------------------------------------------------------------------

#[test]
fn empty_openapi_resolves() {
    let resolved = OpenAPI::default().resolve_all().expect("resolve");
    assert!(resolved.paths.paths.is_empty());
    assert!(resolved.components.schemas.is_empty());
    assert!(resolved.components.responses.is_empty());
    assert!(resolved.components.parameters.is_empty());
}

#[test]
fn empty_components_resolves() {
    let openapi = OpenAPI {
        components: Some(Components::default()),
        ..Default::default()
    };
    let resolved = openapi.resolve_all().expect("resolve");
    assert!(resolved.components.schemas.is_empty());
}

#[test]
fn malformed_refs_are_rejected() {
    fn make(reference: &str) -> OpenAPI {
        OpenAPI {
            components: Some(Components {
                schemas: IndexMap::from([(
                    "X".to_string(),
                    schema_item(
                        "X",
                        SchemaKind::Type(Type::Object(ObjectType {
                            properties: IndexMap::from([(
                                "p".to_string(),
                                ReferenceOr::Reference {
                                    reference: reference.to_string(),
                                },
                            )]),
                            ..Default::default()
                        })),
                    ),
                )]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    // Empty / bare-fragment refs don't begin with `#/components/`.
    assert!(matches!(
        make("").resolve_all().unwrap_err(),
        ResolveError::UnsupportedReference { .. }
    ));
    assert!(matches!(
        make("#/").resolve_all().unwrap_err(),
        ResolveError::UnsupportedReference { .. }
    ));

    // Stripped-prefix is empty or has no slash separating <kind>/<name>.
    assert!(matches!(
        make("#/components/").resolve_all().unwrap_err(),
        ResolveError::UnresolvedReference { .. }
    ));
    assert!(matches!(
        make("#/components/schemas").resolve_all().unwrap_err(),
        ResolveError::UnresolvedReference { .. }
    ));
}

#[test]
fn extensions_are_preserved_throughout() {
    use serde_json::json;
    let mut top = IndexMap::new();
    top.insert("x-top".to_string(), json!("t"));
    let mut comp = IndexMap::new();
    comp.insert("x-comp".to_string(), json!(1));
    let mut paths_ext = IndexMap::new();
    paths_ext.insert("x-paths".to_string(), json!(true));
    let mut op_ext = IndexMap::new();
    op_ext.insert("x-op".to_string(), json!("op"));
    let mut schema_ext = IndexMap::new();
    schema_ext.insert("x-schema".to_string(), json!("s"));

    let openapi = OpenAPI {
        extensions: top,
        paths: Paths {
            paths: IndexMap::from([(
                "/foo".to_string(),
                ReferenceOr::Item(PathItem {
                    get: Some(Operation {
                        extensions: op_ext,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: paths_ext,
        },
        components: Some(Components {
            extensions: comp,
            schemas: IndexMap::from([(
                "S".to_string(),
                ReferenceOr::Item(Schema {
                    schema_data: SchemaData {
                        extensions: schema_ext,
                        ..Default::default()
                    },
                    schema_kind: string_kind(),
                }),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    assert_eq!(resolved.extensions.get("x-top"), Some(&json!("t")));
    assert_eq!(
        resolved.components.extensions.get("x-comp"),
        Some(&json!(1))
    );
    assert_eq!(resolved.paths.extensions.get("x-paths"), Some(&json!(true)));
    let op = resolved
        .paths
        .paths
        .get("/foo")
        .unwrap()
        .get
        .as_ref()
        .unwrap();
    assert_eq!(op.extensions.get("x-op"), Some(&json!("op")));
    let s = resolved.components.schemas.get("S").unwrap();
    assert_eq!(s.schema_data.extensions.get("x-schema"), Some(&json!("s")));
}

// ---------------------------------------------------------------------------
// Operation-level inline callbacks; PathItem.parameters + Operation.security
// ---------------------------------------------------------------------------

#[test]
fn resolves_inline_operation_callbacks() {
    let mut cb: Callback = IndexMap::new();
    cb.insert(
        "{$request.body#/url}".to_string(),
        PathItem {
            post: Some(Operation::default()),
            ..Default::default()
        },
    );

    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/foo".to_string(),
                ReferenceOr::Item(PathItem {
                    post: Some(Operation {
                        callbacks: IndexMap::from([("onEvent".to_string(), cb)]),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let op = resolved
        .paths
        .paths
        .get("/foo")
        .unwrap()
        .post
        .as_ref()
        .unwrap();
    let cb = op.callbacks.get("onEvent").expect("callback present");
    let pi = cb.get("{$request.body#/url}").expect("inline path");
    assert!(pi.post.is_some());
}

#[test]
fn resolves_path_item_parameter_ref_and_operation_security() {
    let openapi = OpenAPI {
        paths: Paths {
            paths: IndexMap::from([(
                "/foo".to_string(),
                ReferenceOr::Item(PathItem {
                    parameters: vec![ReferenceOr::Reference {
                        reference: "#/components/parameters/Limit".to_string(),
                    }],
                    get: Some(Operation {
                        security: Some(vec![{
                            let mut sr = IndexMap::new();
                            sr.insert("apiKey".to_string(), vec!["read".to_string()]);
                            sr
                        }]),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )]),
            extensions: IndexMap::new(),
        },
        components: Some(Components {
            parameters: IndexMap::from([(
                "Limit".to_string(),
                ReferenceOr::Item(query_param("limit")),
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = openapi.resolve_all().expect("resolve");
    let pi = resolved.paths.paths.get("/foo").unwrap();
    assert_eq!(pi.parameters.len(), 1);
    let ResolvedRefOr::Reference(w) = &pi.parameters[0] else {
        panic!("expected $ref")
    };
    let p = w.upgrade().unwrap();
    assert_eq!(p.parameter_data().name, "limit");

    let security = pi
        .get
        .as_ref()
        .unwrap()
        .security
        .as_ref()
        .expect("security present");
    assert_eq!(security.len(), 1);
    assert_eq!(security[0].get("apiKey"), Some(&vec!["read".to_string()]));
}
