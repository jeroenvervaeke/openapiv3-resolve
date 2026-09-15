//! `ResolvedOpenAPI` follows every `$ref` in a document, once per component.

mod common;

use common::spec;
use openapiv3::{OpenAPI, StatusCode};
use openapiv3_resolve::{
    NestedSchema, ResolveError, ResolvedAdditionalProperties, ResolvedDiscriminator,
    ResolvedOpenAPI, ResolvedParameter, ResolvedParameterSchemaOrContent, ResolvedSchema,
    ResolvedSchemaKind, ResolvedType, SchemaGuard, Section, Shared,
};
use std::ptr;

fn parse(document: &str) -> OpenAPI {
    serde_json::from_str(document).expect("fixture spec parses")
}

fn with_schemas(schemas: &str) -> OpenAPI {
    parse(&format!(
        r##"{{"openapi":"3.0.0","info":{{"title":"t","version":"1"}},"paths":{{}},
            "components":{{"schemas":{{{schemas}}}}}}}"##
    ))
}

fn resolve(openapi: &OpenAPI) -> ResolvedOpenAPI {
    ResolvedOpenAPI::try_from(openapi).expect("document resolves")
}

fn schema<'a>(resolved: &'a ResolvedOpenAPI, name: &str) -> &'a Shared<ResolvedSchema> {
    resolved
        .components()
        .expect("has components")
        .schemas
        .get(name)
        .unwrap_or_else(|| panic!("schema {name} present"))
}

fn title(schema: &ResolvedSchema) -> Option<&str> {
    schema.schema_data.title.as_deref()
}

/// Whether two shared values are the very same allocation.
fn shares<T>(left: &Shared<T>, right: &Shared<T>) -> bool {
    ptr::eq(Shared::as_ptr(left), Shared::as_ptr(right))
}

/// Whether `nested` is a plain (non-recursive) edge to exactly `schema`.
fn same(nested: &NestedSchema, schema: &Shared<ResolvedSchema>) -> bool {
    !nested.is_recursive() && points_at(nested, schema)
}

/// Whether `nested` leads to exactly `schema`, recursive or not.
fn points_at(nested: &NestedSchema, schema: &Shared<ResolvedSchema>) -> bool {
    ptr::eq(SchemaGuard::as_ptr(&nested.get()), Shared::as_ptr(schema))
}

fn items(schema: &ResolvedSchema) -> &NestedSchema {
    match &schema.schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Array(array)) => {
            array.items.as_ref().expect("has items")
        }
        other => panic!("not an array: {other:?}"),
    }
}

fn property<'a>(schema: &'a ResolvedSchema, name: &str) -> &'a NestedSchema {
    match &schema.schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Object(object)) => &object.properties[name],
        other => panic!("not an object: {other:?}"),
    }
}

fn discriminator(schema: &ResolvedSchema) -> &ResolvedDiscriminator {
    schema
        .schema_data
        .discriminator
        .as_ref()
        .expect("has a discriminator")
}

/// A document exercising every reference site outside `components/schemas`.
const SITES: &str = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Sites", "version": "1.0.0" },
  "paths": {
    "/pets": {
      "parameters": [ { "$ref": "#/components/parameters/Limit" } ],
      "get": {
        "parameters": [ { "$ref": "#/components/parameters/Limit" } ],
        "requestBody": { "$ref": "#/components/requestBodies/CreatePet" },
        "responses": {
          "default": { "$ref": "#/components/responses/PetList" },
          "200": { "$ref": "#/components/responses/PetList" },
          "201": {
            "description": "inline",
            "headers": { "X-Rate": { "$ref": "#/components/headers/XRate" } },
            "links": { "self": { "$ref": "#/components/links/Self" } },
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/Pet" },
                "examples": { "one": { "$ref": "#/components/examples/One" } },
                "encoding": {
                  "field": { "headers": { "X-Rate": { "$ref": "#/components/headers/XRate" } } }
                }
              }
            }
          }
        },
        "callbacks": {
          "onEvent": {
            "{$request.body#/url}": {
              "post": {
                "parameters": [ { "$ref": "#/components/parameters/Limit" } ],
                "responses": { "200": { "$ref": "#/components/responses/PetList" } }
              }
            }
          }
        }
      }
    },
    "/alias": { "$ref": "#/paths/~1pets" }
  },
  "components": {
    "schemas": { "Pet": { "title": "Pet", "type": "string" } },
    "responses": {
      "PetList": {
        "description": "pets",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Pet" } } }
      }
    },
    "parameters": {
      "Limit": {
        "name": "limit", "in": "query",
        "schema": { "$ref": "#/components/schemas/Pet" },
        "examples": { "one": { "$ref": "#/components/examples/One" } }
      },
      "Body": {
        "name": "body", "in": "header",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Pet" } } }
      }
    },
    "examples": { "One": { "value": 1 } },
    "requestBodies": {
      "CreatePet": {
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Pet" } } }
      }
    },
    "headers": {
      "XRate": {
        "schema": { "$ref": "#/components/schemas/Pet" },
        "examples": { "one": { "$ref": "#/components/examples/One" } }
      }
    },
    "securitySchemes": { "ApiKey": { "type": "apiKey", "name": "key", "in": "header" } },
    "links": { "Self": { "operationId": "getPets" } },
    "callbacks": {
      "OnEvent": {
        "{$request.body#/url}": {
          "parameters": [ { "$ref": "#/components/parameters/Limit" } ]
        }
      }
    }
  }
}"##;

#[test]
fn resolves_every_section_of_the_shared_fixture() {
    let resolved = resolve(&spec());
    let components = resolved.components().expect("has components");

    assert_eq!(components.schemas.len(), 2);
    assert_eq!(components.responses.len(), 2);
    assert_eq!(components.parameters.len(), 2);
    assert_eq!(components.examples.len(), 2);
    assert_eq!(components.request_bodies.len(), 2);
    assert_eq!(components.headers.len(), 2);
    assert_eq!(components.security_schemes.len(), 2);
    assert_eq!(components.links.len(), 2);
    assert_eq!(components.callbacks.len(), 2);
    assert_eq!(resolved.paths().paths.len(), 3);
}

#[test]
fn a_component_that_is_itself_a_reference_shares_its_target() {
    let resolved = resolve(&spec());
    let components = resolved.components().expect("has components");

    assert!(shares(
        schema(&resolved, "PetAlias"),
        schema(&resolved, "Pet")
    ));
    assert!(shares(
        &components.responses["PetListAlias"],
        &components.responses["PetList"]
    ));
    assert!(shares(
        &components.parameters["LimitAlias"],
        &components.parameters["Limit"]
    ));
    assert!(shares(
        &components.examples["OneAlias"],
        &components.examples["One"]
    ));
    assert!(shares(
        &components.request_bodies["CreatePetAlias"],
        &components.request_bodies["CreatePet"]
    ));
    assert!(shares(
        &components.headers["XRateAlias"],
        &components.headers["XRate"]
    ));
    assert!(shares(
        &components.security_schemes["ApiKeyAlias"],
        &components.security_schemes["ApiKey"]
    ));
    assert!(shares(
        &components.links["SelfAlias"],
        &components.links["Self"]
    ));
    assert!(shares(
        &components.callbacks["OnEventAlias"],
        &components.callbacks["OnEvent"]
    ));
    assert!(shares(
        &resolved.paths().paths["/alias"],
        &resolved.paths().paths["/pets"]
    ));
}

#[test]
fn every_reference_site_shares_the_component_it_names() {
    let resolved = resolve(&parse(SITES));
    let components = resolved.components().expect("has components");
    let pet = &components.schemas["Pet"];
    let limit = &components.parameters["Limit"];
    let pet_list = &components.responses["PetList"];
    let x_rate = &components.headers["XRate"];
    let one = &components.examples["One"];

    let pets = &resolved.paths().paths["/pets"];
    let get = pets.get.as_ref().expect("GET /pets");
    assert!(shares(&pets.parameters[0], limit));
    assert!(shares(&get.parameters[0], limit));
    assert!(shares(
        get.request_body.as_ref().expect("body"),
        &components.request_bodies["CreatePet"]
    ));
    assert!(shares(
        get.responses.default.as_ref().expect("default"),
        pet_list
    ));
    assert!(shares(
        &get.responses.responses[&StatusCode::Code(200)],
        pet_list
    ));

    let created = &get.responses.responses[&StatusCode::Code(201)];
    assert!(shares(&created.headers["X-Rate"], x_rate));
    assert!(shares(&created.links["self"], &components.links["Self"]));
    let media = &created.content["application/json"];
    assert!(shares(media.schema.as_ref().expect("schema"), pet));
    assert!(shares(&media.examples["one"], one));
    assert!(shares(&media.encoding["field"].headers["X-Rate"], x_rate));

    let callback_post = get.callbacks["onEvent"]["{$request.body#/url}"]
        .post
        .as_ref()
        .expect("callback POST");
    assert!(shares(&callback_post.parameters[0], limit));
    assert!(shares(
        &callback_post.responses.responses[&StatusCode::Code(200)],
        pet_list
    ));
    assert!(shares(
        &components.callbacks["OnEvent"]["{$request.body#/url}"].parameters[0],
        limit
    ));

    let body = &pet_list.content["application/json"];
    assert!(shares(body.schema.as_ref().expect("schema"), pet));
    let ResolvedParameterSchemaOrContent::Schema(limit_schema) = &limit.parameter_data().format
    else {
        panic!("Limit has a schema");
    };
    assert!(shares(limit_schema, pet));
    assert!(shares(&limit.parameter_data().examples["one"], one));
    let ResolvedParameterSchemaOrContent::Content(content) =
        &components.parameters["Body"].parameter_data().format
    else {
        panic!("Body has content");
    };
    assert!(shares(
        content["application/json"].schema.as_ref().expect("schema"),
        pet
    ));
    let ResolvedParameterSchemaOrContent::Schema(header_schema) = &x_rate.format else {
        panic!("XRate has a schema");
    };
    assert!(shares(header_schema, pet));
    assert!(shares(&x_rate.examples["one"], one));
}

#[test]
fn keeps_every_kind_of_parameter() {
    let resolved = resolve(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{},
            "components":{"parameters":{
              "q": {"name":"q","in":"query","schema":{}},
              "h": {"name":"h","in":"header","schema":{}},
              "p": {"name":"p","in":"path","required":true,"schema":{}},
              "c": {"name":"c","in":"cookie","schema":{}}
            }}}"##,
    ));
    let parameters = &resolved.components().expect("components").parameters;
    assert!(matches!(*parameters["q"], ResolvedParameter::Query { .. }));
    assert!(matches!(*parameters["h"], ResolvedParameter::Header { .. }));
    assert!(matches!(*parameters["p"], ResolvedParameter::Path { .. }));
    assert!(matches!(*parameters["c"], ResolvedParameter::Cookie { .. }));
    for (name, parameter) in parameters {
        assert_eq!(&parameter.parameter_data().name, name);
    }
}

#[test]
fn resolves_references_nested_inside_typed_schemas() {
    let resolved = resolve(&with_schemas(
        r##""Pet": { "title": "Pet", "type": "string" },
            "Object": { "type": "object",
                        "properties": { "pet": { "$ref": "#/components/schemas/Pet" } },
                        "additionalProperties": { "$ref": "#/components/schemas/Pet" } },
            "Array": { "type": "array", "items": { "$ref": "#/components/schemas/Pet" } },
            "OneOf": { "oneOf": [ { "$ref": "#/components/schemas/Pet" } ] },
            "AllOf": { "allOf": [ { "$ref": "#/components/schemas/Pet" } ] },
            "AnyOf": { "anyOf": [ { "$ref": "#/components/schemas/Pet" } ] },
            "Not": { "not": { "$ref": "#/components/schemas/Pet" } }"##,
    ));
    let pet = schema(&resolved, "Pet");

    let ResolvedSchemaKind::Type(ResolvedType::Object(object)) =
        &schema(&resolved, "Object").schema_kind
    else {
        panic!("Object is an object");
    };
    assert!(same(&object.properties["pet"], pet));
    let Some(ResolvedAdditionalProperties::Schema(additional)) = &object.additional_properties
    else {
        panic!("Object has additional properties");
    };
    assert!(same(additional, pet));
    assert!(same(items(schema(&resolved, "Array")), pet));

    for (name, expected) in [
        ("OneOf", "one_of"),
        ("AllOf", "all_of"),
        ("AnyOf", "any_of"),
    ] {
        let alternatives = match &schema(&resolved, name).schema_kind {
            ResolvedSchemaKind::OneOf { one_of } if expected == "one_of" => one_of,
            ResolvedSchemaKind::AllOf { all_of } if expected == "all_of" => all_of,
            ResolvedSchemaKind::AnyOf { any_of } if expected == "any_of" => any_of,
            other => panic!("{name} resolved to {other:?}"),
        };
        assert!(same(&alternatives[0], pet), "{name}");
    }

    let ResolvedSchemaKind::Not { not } = &schema(&resolved, "Not").schema_kind else {
        panic!("Not is a negation");
    };
    assert!(same(not, pet));
}

#[test]
fn resolves_references_nested_inside_an_untyped_schema() {
    // No `type` plus mixed keywords is what upstream parses as `AnySchema`.
    let resolved = resolve(&with_schemas(
        r##""Pet": { "title": "Pet", "type": "string" },
            "Any": { "properties": { "pet": { "$ref": "#/components/schemas/Pet" } },
                     "additionalProperties": { "$ref": "#/components/schemas/Pet" },
                     "items": { "$ref": "#/components/schemas/Pet" },
                     "oneOf": [ { "$ref": "#/components/schemas/Pet" } ],
                     "allOf": [ { "$ref": "#/components/schemas/Pet" } ],
                     "anyOf": [ { "$ref": "#/components/schemas/Pet" } ],
                     "not": { "$ref": "#/components/schemas/Pet" } }"##,
    ));
    let pet = schema(&resolved, "Pet");
    let ResolvedSchemaKind::Any(any) = &schema(&resolved, "Any").schema_kind else {
        panic!("Any is untyped");
    };
    assert!(same(&any.properties["pet"], pet));
    let Some(ResolvedAdditionalProperties::Schema(additional)) = &any.additional_properties else {
        panic!("Any has additional properties");
    };
    assert!(same(additional, pet));
    assert!(same(any.items.as_ref().expect("items"), pet));
    assert!(same(&any.one_of[0], pet));
    assert!(same(&any.all_of[0], pet));
    assert!(same(&any.any_of[0], pet));
    assert!(same(any.not.as_ref().expect("not"), pet));
}

#[test]
fn keeps_a_boolean_additional_properties() {
    let resolved = resolve(&with_schemas(
        r##""Open": { "type": "object", "additionalProperties": true },
            "Closed": { "type": "object", "additionalProperties": false }"##,
    ));
    let additional = |name: &str| match &schema(&resolved, name).schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Object(object)) => &object.additional_properties,
        other => panic!("{name} resolved to {other:?}"),
    };
    assert_eq!(
        additional("Open"),
        &Some(ResolvedAdditionalProperties::Any(true))
    );
    assert_eq!(
        additional("Closed"),
        &Some(ResolvedAdditionalProperties::Any(false))
    );
}

#[test]
fn resolves_a_header_described_by_content() {
    let resolved = resolve(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{},
            "components":{
              "schemas":{"Pet":{"type":"string"}},
              "headers":{"X-Pet":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Pet"}}}}}
            }}"##,
    ));
    let components = resolved.components().expect("components");
    let ResolvedParameterSchemaOrContent::Content(content) = &components.headers["X-Pet"].format
    else {
        panic!("X-Pet has content");
    };
    assert!(shares(
        content["application/json"].schema.as_ref().expect("schema"),
        &components.schemas["Pet"]
    ));
}

#[test]
fn inline_items_are_kept_and_not_shared() {
    let resolved = resolve(&with_schemas(
        r##""A": { "type": "array", "items": { "title": "inline", "type": "string" } },
            "B": { "type": "array", "items": { "title": "inline", "type": "string" } }"##,
    ));
    let a = items(schema(&resolved, "A")).get();
    let b = items(schema(&resolved, "B")).get();
    assert_eq!(title(&a), Some("inline"));
    assert_eq!(*a, *b);
    assert!(!ptr::eq(SchemaGuard::as_ptr(&a), SchemaGuard::as_ptr(&b)));
}

#[test]
fn a_chain_of_references_resolves_to_the_item_at_its_end() {
    let resolved = resolve(&with_schemas(
        r##""A": { "$ref": "#/components/schemas/B" },
            "B": { "$ref": "#/components/schemas/C" },
            "C": { "title": "end", "type": "string" }"##,
    ));
    assert_eq!(title(schema(&resolved, "A")), Some("end"));
    assert!(shares(schema(&resolved, "A"), schema(&resolved, "C")));
    assert!(shares(schema(&resolved, "B"), schema(&resolved, "C")));
}

#[test]
fn a_document_without_components_resolves_to_none() {
    let resolved = resolve(&parse(
        r#"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{}}"#,
    ));
    assert_eq!(resolved.components(), None);
    assert!(resolved.paths().paths.is_empty());
}

#[test]
fn a_document_without_references_copies_over_unchanged() {
    let openapi = parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},
            "paths":{"/pets":{"get":{"operationId":"list","responses":{"200":{"description":"ok"}}}}},
            "tags":[{"name":"pets"}],"x-root":true}"##,
    );
    let resolved = resolve(&openapi);
    assert_eq!(resolved.openapi(), openapi.openapi);
    assert_eq!(resolved.info(), &openapi.info);
    assert_eq!(resolved.tags(), openapi.tags);
    assert_eq!(resolved.extensions(), &openapi.extensions);
    let get = resolved.paths().paths["/pets"].get.as_ref().expect("GET");
    assert_eq!(get.operation_id.as_deref(), Some("list"));
    assert_eq!(
        get.responses.responses[&StatusCode::Code(200)].description,
        "ok"
    );
}

#[test]
fn a_path_item_lists_its_operations_in_method_order() {
    let resolved = resolve(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},
            "paths":{"/pets":{"post":{"responses":{}},"get":{"responses":{}},"delete":{"responses":{}}}}}"##,
    ));
    let methods: Vec<&str> = resolved.paths().paths["/pets"]
        .iter()
        .map(|(method, _)| method)
        .collect();
    assert_eq!(methods, ["get", "post", "delete"]);
}

#[test]
fn a_dangling_reference_fails_the_whole_document() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Pet": { "type": "array", "items": { "$ref": "#/components/schemas/Missing" } }"##,
    ))
    .expect_err("dangling reference");
    assert_eq!(
        error,
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Missing".to_owned()
        }
    );
}

#[test]
fn a_reference_into_the_wrong_section_is_an_error() {
    let error = ResolvedOpenAPI::try_from(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},
            "paths":{"/pets":{"$ref":"#/components/schemas/Pet"}}}"##,
    ))
    .expect_err("wrong section");
    assert_eq!(
        error,
        ResolveError::SectionMismatch {
            expected: Section::Paths,
            found: Section::Schemas
        }
    );
}

#[test]
fn a_reference_without_components_to_look_in_is_an_error() {
    let error = ResolvedOpenAPI::try_from(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},
            "paths":{"/pets":{"get":{"responses":{"200":{"$ref":"#/components/responses/Ok"}}}}}}"##,
    ))
    .expect_err("no components");
    assert_eq!(
        error,
        ResolveError::ComponentsMissing {
            section: Section::Responses
        }
    );
}

#[test]
fn a_schema_that_contains_itself_gets_a_recursive_edge_back_to_itself() {
    let resolved = resolve(&with_schemas(
        r##""Node": { "title": "Node", "type": "object",
                      "properties": { "next": { "$ref": "#/components/schemas/Node" } } }"##,
    ));
    let node = schema(&resolved, "Node");
    let next = property(node, "next");
    assert!(next.is_recursive());
    assert!(points_at(next, node));
    assert_eq!(title(&next.get()), Some("Node"));
    // Dereferencing keeps working however deep the recursion goes.
    let once = next.get();
    let twice = property(&once, "next").get();
    assert!(ptr::eq(SchemaGuard::as_ptr(&twice), Shared::as_ptr(node)));
}

#[test]
fn many_guards_on_one_recursive_edge_can_be_held_at_once() {
    let resolved = resolve(&with_schemas(
        r##""Node": { "type": "object",
                      "properties": { "next": { "$ref": "#/components/schemas/Node" } } }"##,
    ));
    let node = schema(&resolved, "Node");
    let next = property(node, "next");
    let guards: Vec<SchemaGuard<'_>> = (0..1000).map(|_| next.get()).collect();
    assert!(guards
        .iter()
        .all(|guard| ptr::eq(SchemaGuard::as_ptr(guard), Shared::as_ptr(node))));
    drop(guards);
    assert!(points_at(next, node));
}

#[test]
fn a_cycle_through_another_schema_is_recursive_only_where_it_closes() {
    // Resolved in document order: A first, so the edge from B back to A is
    // the one that finds A still under construction.
    let resolved = resolve(&with_schemas(
        r##""A": { "type": "array", "items": { "$ref": "#/components/schemas/B" } },
            "B": { "type": "array", "items": { "$ref": "#/components/schemas/A" } }"##,
    ));
    let a = schema(&resolved, "A");
    let b = schema(&resolved, "B");
    assert!(same(items(a), b));
    assert!(items(b).is_recursive());
    assert!(points_at(items(b), a));
}

#[test]
fn every_nested_position_can_point_back_at_its_own_schema() {
    let resolved = resolve(&with_schemas(
        r##""Object": { "type": "object",
                        "additionalProperties": { "$ref": "#/components/schemas/Object" } },
            "OneOf": { "oneOf": [ { "$ref": "#/components/schemas/OneOf" } ] },
            "AllOf": { "allOf": [ { "$ref": "#/components/schemas/AllOf" } ] },
            "AnyOf": { "anyOf": [ { "$ref": "#/components/schemas/AnyOf" } ] },
            "Not": { "not": { "$ref": "#/components/schemas/Not" } },
            "Any": { "properties": { "p": { "$ref": "#/components/schemas/Any" } },
                     "additionalProperties": { "$ref": "#/components/schemas/Any" },
                     "items": { "$ref": "#/components/schemas/Any" },
                     "oneOf": [ { "$ref": "#/components/schemas/Any" } ],
                     "allOf": [ { "$ref": "#/components/schemas/Any" } ],
                     "anyOf": [ { "$ref": "#/components/schemas/Any" } ],
                     "not": { "$ref": "#/components/schemas/Any" } }"##,
    ));
    let back_to = |name: &str, edge: &NestedSchema| {
        assert!(edge.is_recursive(), "{name}");
        assert!(points_at(edge, schema(&resolved, name)), "{name}");
    };
    match &schema(&resolved, "Object").schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Object(object)) => {
            let Some(ResolvedAdditionalProperties::Schema(edge)) = &object.additional_properties
            else {
                panic!("Object has additional properties");
            };
            back_to("Object", edge);
        }
        other => panic!("Object resolved to {other:?}"),
    }
    match &schema(&resolved, "OneOf").schema_kind {
        ResolvedSchemaKind::OneOf { one_of } => back_to("OneOf", &one_of[0]),
        other => panic!("OneOf resolved to {other:?}"),
    }
    match &schema(&resolved, "AllOf").schema_kind {
        ResolvedSchemaKind::AllOf { all_of } => back_to("AllOf", &all_of[0]),
        other => panic!("AllOf resolved to {other:?}"),
    }
    match &schema(&resolved, "AnyOf").schema_kind {
        ResolvedSchemaKind::AnyOf { any_of } => back_to("AnyOf", &any_of[0]),
        other => panic!("AnyOf resolved to {other:?}"),
    }
    match &schema(&resolved, "Not").schema_kind {
        ResolvedSchemaKind::Not { not } => back_to("Not", not),
        other => panic!("Not resolved to {other:?}"),
    }
    let ResolvedSchemaKind::Any(any) = &schema(&resolved, "Any").schema_kind else {
        panic!("Any is untyped");
    };
    back_to("Any", &any.properties["p"]);
    let Some(ResolvedAdditionalProperties::Schema(additional)) = &any.additional_properties else {
        panic!("Any has additional properties");
    };
    back_to("Any", additional);
    back_to("Any", any.items.as_ref().expect("items"));
    back_to("Any", &any.one_of[0]);
    back_to("Any", &any.all_of[0]);
    back_to("Any", &any.any_of[0]);
    back_to("Any", any.not.as_ref().expect("not"));
}

#[test]
fn which_edge_is_recursive_follows_document_order() {
    let resolved = resolve(&with_schemas(
        r##""B": { "type": "array", "items": { "$ref": "#/components/schemas/A" } },
            "A": { "type": "array", "items": { "$ref": "#/components/schemas/B" } }"##,
    ));
    // `B` comes first, so it is under construction when `A` refers back to it.
    assert!(items(schema(&resolved, "A")).is_recursive());
    assert!(!items(schema(&resolved, "B")).is_recursive());
}

#[test]
fn a_parameter_naming_a_recursive_schema_shares_it() {
    // Schemas are resolved before any other section, so `Node` is already
    // done when the parameter reaches it and the parameter's edge is a plain
    // shared one, not a recursive one.
    let resolved = resolve(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{},
            "components":{
              "parameters":{"n":{"name":"n","in":"query","schema":{"$ref":"#/components/schemas/Node"}}},
              "schemas":{"Node":{"type":"object","properties":{"next":{"$ref":"#/components/schemas/Node"}}}}
            }}"##,
    ));
    let components = resolved.components().expect("components");
    let node = &components.schemas["Node"];
    let ResolvedParameterSchemaOrContent::Schema(from_parameter) =
        &components.parameters["n"].parameter_data().format
    else {
        panic!("n has a schema");
    };
    assert!(shares(from_parameter, node));
    assert!(points_at(property(node, "next"), node));
}

#[test]
fn a_recursive_document_can_be_printed_and_compared() {
    let openapi = with_schemas(
        r##""Node": { "type": "object",
                      "properties": { "next": { "$ref": "#/components/schemas/Node" } } }"##,
    );
    let resolved = resolve(&openapi);
    let printed = format!("{resolved:?}");
    assert!(printed.contains("Recursive(..)"), "{printed}");
    // Recursive edges compare by identity, so a second resolution differs.
    assert_ne!(resolved, resolve(&openapi));
}

#[test]
fn two_resolutions_of_an_acyclic_document_are_equal() {
    let openapi = spec();
    assert_eq!(resolve(&openapi), resolve(&openapi));
}

#[test]
fn a_header_that_contains_itself_is_a_cycle() {
    // The one non-schema cycle a document can express: a header described
    // by content whose encoding names the header itself.
    let error = ResolvedOpenAPI::try_from(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{},
            "components":{"headers":{"Loop":{"content":{"a/b":{"encoding":{"f":{
              "headers":{"Loop":{"$ref":"#/components/headers/Loop"}}}}}}}}}}"##,
    ))
    .expect_err("cyclic");
    assert_eq!(
        error,
        ResolveError::CyclicReference {
            reference: "#/components/headers/Loop".to_owned()
        }
    );
}

#[test]
fn a_failure_inside_a_recursive_schema_is_reported_not_swallowed() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Node": { "type": "object", "properties": {
              "next": { "$ref": "#/components/schemas/Node" },
              "bad": { "$ref": "#/components/schemas/Missing" } } }"##,
    ))
    .expect_err("dangling reference");
    assert_eq!(
        error,
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Missing".to_owned()
        }
    );
}

#[test]
fn a_cycle_between_bare_references_is_a_chain_that_never_ends() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""A": { "$ref": "#/components/schemas/B" },
            "B": { "$ref": "#/components/schemas/A" }"##,
    ))
    .expect_err("cyclic");
    assert!(
        matches!(error, ResolveError::ReferenceChainTooLong { .. }),
        "{error:?}"
    );
}

#[test]
fn a_component_used_twice_is_not_mistaken_for_a_cycle() {
    // Diamond: Both -> Left -> Pet and Both -> Right -> Pet. `Pet` is reached
    // twice but never while it is still being resolved.
    let resolved = resolve(&with_schemas(
        r##""Pet": { "title": "Pet", "type": "string" },
            "Left": { "type": "array", "items": { "$ref": "#/components/schemas/Pet" } },
            "Right": { "type": "array", "items": { "$ref": "#/components/schemas/Pet" } },
            "Both": { "allOf": [ { "$ref": "#/components/schemas/Left" },
                                 { "$ref": "#/components/schemas/Right" } ] }"##,
    ));
    let ResolvedSchemaKind::AllOf { all_of } = &schema(&resolved, "Both").schema_kind else {
        panic!("Both is allOf");
    };
    assert!(same(&all_of[0], schema(&resolved, "Left")));
    assert!(same(&all_of[1], schema(&resolved, "Right")));
    assert!(same(
        items(schema(&resolved, "Left")),
        schema(&resolved, "Pet")
    ));
    assert!(same(
        items(schema(&resolved, "Right")),
        schema(&resolved, "Pet")
    ));
}

#[test]
fn resolving_by_value_matches_resolving_by_reference() {
    let openapi = spec();
    let by_reference = resolve(&openapi);
    let by_value = ResolvedOpenAPI::try_from(openapi).expect("document resolves");
    assert_eq!(by_reference, by_value);
}

#[test]
fn a_discriminator_mapping_by_reference_shares_the_schema_it_names() {
    let resolved = resolve(&with_schemas(
        r##""Cat": { "title": "Cat", "type": "object" },
            "Dog": { "title": "Dog", "type": "object" },
            "Pet": {
              "discriminator": {
                "propertyName": "kind",
                "mapping": { "cat": "#/components/schemas/Cat", "dog": "#/components/schemas/Dog" },
                "x-note": true
              },
              "oneOf": [ { "$ref": "#/components/schemas/Cat" }, { "$ref": "#/components/schemas/Dog" } ]
            }"##,
    ));
    let discriminator = discriminator(schema(&resolved, "Pet"));
    assert_eq!(discriminator.property_name, "kind");
    assert_eq!(discriminator.extensions["x-note"], serde_json::json!(true));
    assert_eq!(
        discriminator.mapping.keys().collect::<Vec<_>>(),
        ["cat", "dog"]
    );
    assert!(same(
        &discriminator.mapping["cat"],
        schema(&resolved, "Cat")
    ));
    assert!(same(
        &discriminator.mapping["dog"],
        schema(&resolved, "Dog")
    ));
}

#[test]
fn a_discriminator_mapping_by_bare_name_shares_the_schema_it_names() {
    let resolved = resolve(&with_schemas(
        r##""Cat": { "title": "Cat", "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Cat" } ]
            }"##,
    ));
    let discriminator = discriminator(schema(&resolved, "Pet"));
    assert!(same(
        &discriminator.mapping["cat"],
        schema(&resolved, "Cat")
    ));
}

#[test]
fn a_bare_mapping_name_is_looked_up_verbatim() {
    // Names outside the specification's grammar, including ones that look
    // JSON-pointer-escaped or percent-encoded, are matched exactly as written.
    for name in ["Cat", "a/b~c", "caf%C3%A9", "100%25", "a%2Fb"] {
        let resolved = resolve(&with_schemas(&format!(
            r##""{name}": {{ "type": "object" }},
                "Pet": {{
                  "type": "object",
                  "discriminator": {{ "propertyName": "kind", "mapping": {{ "x": "{name}" }} }}
                }}"##
        )));
        let discriminator = discriminator(schema(&resolved, "Pet"));
        assert!(
            same(&discriminator.mapping["x"], schema(&resolved, name)),
            "{name}"
        );
    }
}

#[test]
fn a_bare_mapping_name_is_not_percent_decoded() {
    let resolved = resolve(&with_schemas(
        r##""café": { "title": "decoded", "type": "object" },
            "caf%C3%A9": { "title": "raw", "type": "object" },
            "Pet": {
              "type": "object",
              "discriminator": { "propertyName": "kind", "mapping": { "x": "caf%C3%A9" } }
            }"##,
    ));
    let discriminator = discriminator(schema(&resolved, "Pet"));
    assert!(same(
        &discriminator.mapping["x"],
        schema(&resolved, "caf%C3%A9")
    ));
}

#[test]
fn a_discriminator_mapping_back_to_its_own_schema_is_recursive() {
    let resolved = resolve(&with_schemas(
        r##""Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "pet": "#/components/schemas/Pet" } },
              "oneOf": [ { "$ref": "#/components/schemas/Pet" } ]
            }"##,
    ));
    let pet = schema(&resolved, "Pet");
    let edge = &discriminator(pet).mapping["pet"];
    assert!(edge.is_recursive());
    assert!(points_at(edge, pet));
}

#[test]
fn a_dangling_discriminator_mapping_fails_the_whole_document() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "#/components/schemas/Cat" } },
              "oneOf": [ { "type": "object" } ]
            }"##,
    ))
    .expect_err("mapping target is missing");
    assert_eq!(
        error,
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Cat".to_owned(),
        }
    );
}

#[test]
fn a_dangling_bare_discriminator_mapping_name_fails_the_whole_document() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "type": "object" } ]
            }"##,
    ))
    .expect_err("mapping target is missing");
    assert_eq!(
        error,
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Cat".to_owned(),
        }
    );
}

#[test]
fn a_discriminator_mapping_into_another_document_is_an_error() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "pets.yaml#/components/schemas/Cat" } },
              "oneOf": [ { "type": "object" } ]
            }"##,
    ))
    .expect_err("mapping target is external");
    assert_eq!(
        error,
        ResolveError::ExternalDocument {
            document: "pets.yaml".to_owned(),
        }
    );
}

#[test]
fn a_discriminator_mapping_into_another_section_is_an_error() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "#/components/responses/Cat" } },
              "oneOf": [ { "type": "object" } ]
            }"##,
    ))
    .expect_err("mapping target is a response");
    assert_eq!(
        error,
        ResolveError::SectionMismatch {
            expected: Section::Schemas,
            found: Section::Responses,
        }
    );
}

#[test]
fn a_discriminator_mapping_into_the_middle_of_a_schema_is_an_error() {
    let reference = "#/components/schemas/Cat/properties/name";
    let error = ResolvedOpenAPI::try_from(&with_schemas(&format!(
        r##""Pet": {{
              "discriminator": {{ "propertyName": "kind", "mapping": {{ "cat": "{reference}" }} }},
              "oneOf": [ {{ "type": "object" }} ]
            }}"##
    )))
    .expect_err("mapping target is inside a schema");
    assert_eq!(
        error,
        ResolveError::PointerTooDeep {
            reference: reference.to_owned(),
        }
    );
}

#[test]
fn a_discriminator_on_an_inline_property_schema_resolves_its_mapping() {
    let resolved = resolve(&with_schemas(
        r##""Cat": { "title": "Cat", "type": "object" },
            "Owner": { "type": "object", "properties": { "pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Cat" } ]
            } } }"##,
    ));
    let pet = property(schema(&resolved, "Owner"), "pet").get();
    assert!(same(
        &discriminator(&pet).mapping["cat"],
        schema(&resolved, "Cat")
    ));
}

#[test]
fn a_discriminator_on_a_path_inline_schema_resolves_its_mapping() {
    let resolved = resolve(&parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},
            "paths":{"/pets":{"post":{"requestBody":{"content":{"application/json":{"schema":{
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Cat" } ]
            }}}},"responses":{}}}},
            "components":{"schemas":{"Cat": { "title": "Cat", "type": "object" }}}}"##,
    ));
    let post = resolved.paths().paths["/pets"]
        .post
        .as_ref()
        .expect("POST /pets");
    let body = post.request_body.as_ref().expect("body");
    let in_body = body.content["application/json"]
        .schema
        .as_ref()
        .expect("schema");
    assert!(same(
        &discriminator(in_body).mapping["cat"],
        schema(&resolved, "Cat")
    ));
}

#[test]
fn two_resolutions_of_a_discriminator_mapping_are_equal() {
    let openapi = with_schemas(
        r##""Cat": { "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Cat" } ]
            }"##,
    );
    assert_eq!(resolve(&openapi), resolve(&openapi));
}

#[test]
fn a_self_referential_discriminator_mapping_can_be_printed_and_compared() {
    let openapi = with_schemas(
        r##""Pet": {
              "type": "object",
              "discriminator": { "propertyName": "kind", "mapping": { "pet": "Pet" } }
            }"##,
    );
    let resolved = resolve(&openapi);
    let printed = format!("{resolved:?}");
    assert!(printed.contains("Recursive(..)"), "{printed}");
    // Recursive edges compare by identity, so a second resolution differs.
    assert_ne!(resolved, resolve(&openapi));
}

#[test]
fn a_discriminator_mapping_is_the_alternative_edge_it_names() {
    let resolved = resolve(&with_schemas(
        r##""Cat": { "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Cat" } ]
            }"##,
    ));
    let pet = schema(&resolved, "Pet");
    let ResolvedSchemaKind::OneOf { one_of } = &pet.schema_kind else {
        panic!("Pet is oneOf");
    };
    let mapped = &discriminator(pet).mapping["cat"];
    assert!(ptr::eq(
        SchemaGuard::as_ptr(&mapped.get()),
        SchemaGuard::as_ptr(&one_of[0].get())
    ));
    assert_eq!(mapped.is_recursive(), one_of[0].is_recursive());
}

#[test]
fn a_discriminator_mapping_matches_an_any_of_alternative() {
    let resolved = resolve(&with_schemas(
        r##""Cat": { "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "#/components/schemas/Cat" } },
              "anyOf": [ { "$ref": "#/components/schemas/Cat" } ]
            }"##,
    ));
    assert!(same(
        &discriminator(schema(&resolved, "Pet")).mapping["cat"],
        schema(&resolved, "Cat")
    ));
}

#[test]
fn a_discriminator_on_a_typed_one_of_matches_its_alternatives() {
    // `type` next to `oneOf` parses as an untyped `Any` schema upstream; the
    // alternatives live in its `one_of` and `any_of` fields.
    let resolved = resolve(&with_schemas(
        r##""AWS": { "type": "object" }, "GCP": { "type": "object" },
            "Account": {
              "type": "object",
              "discriminator": { "propertyName": "cloudProvider",
                                 "mapping": { "aws": "#/components/schemas/AWS", "gcp": "GCP" } },
              "oneOf": [ { "$ref": "#/components/schemas/AWS" } ],
              "anyOf": [ { "$ref": "#/components/schemas/GCP" } ]
            }"##,
    ));
    let account = schema(&resolved, "Account");
    assert!(matches!(account.schema_kind, ResolvedSchemaKind::Any(_)));
    let discriminator = discriminator(account);
    assert!(same(
        &discriminator.mapping["aws"],
        schema(&resolved, "AWS")
    ));
    assert!(same(
        &discriminator.mapping["gcp"],
        schema(&resolved, "GCP")
    ));
}

#[test]
fn a_discriminator_mapping_matches_an_alternative_through_an_alias() {
    // Both sides are compared by the name at the end of the chain.
    let resolved = resolve(&with_schemas(
        r##""Cat": { "type": "object" },
            "CatAlias": { "$ref": "#/components/schemas/Cat" },
            "Pet": {
              "discriminator": { "propertyName": "kind",
                                 "mapping": { "a": "Cat", "b": "CatAlias", "c": "#/components/schemas/CatAlias" } },
              "oneOf": [ { "$ref": "#/components/schemas/CatAlias" } ]
            }"##,
    ));
    let discriminator = discriminator(schema(&resolved, "Pet"));
    for value in ["a", "b", "c"] {
        assert!(
            same(&discriminator.mapping[value], schema(&resolved, "Cat")),
            "{value}"
        );
    }
}

#[test]
fn a_discriminator_mapping_to_a_schema_that_is_not_an_alternative_is_an_error() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Cat": { "type": "object" }, "Dog": { "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Dog" } ]
            }"##,
    ))
    .expect_err("Cat is not an alternative");
    assert_eq!(
        error,
        ResolveError::DiscriminatorMappingMismatch {
            property_name: "kind".to_owned(),
            value: "cat".to_owned(),
            schema: "Cat".to_owned(),
        }
    );
}

#[test]
fn a_discriminator_mapping_cannot_name_an_inline_alternative() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Cat": { "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "type": "object" } ]
            }"##,
    ))
    .expect_err("only an inline alternative exists");
    assert!(
        matches!(error, ResolveError::DiscriminatorMappingMismatch { .. }),
        "{error:?}"
    );
}

#[test]
fn a_dangling_mapping_on_a_one_of_reports_the_dangling_reference() {
    // Existence is checked before membership, so the more specific error wins.
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Dog": { "type": "object" },
            "Pet": {
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } },
              "oneOf": [ { "$ref": "#/components/schemas/Dog" } ]
            }"##,
    ))
    .expect_err("Cat is missing");
    assert_eq!(
        error,
        ResolveError::NotFound {
            section: Section::Schemas,
            name: "Cat".to_owned(),
        }
    );
}

#[test]
fn a_discriminator_on_an_inheritance_parent_resolves_its_children() {
    let resolved = resolve(&with_schemas(
        r##""Pet": {
              "type": "object",
              "discriminator": { "propertyName": "kind", "mapping": { "cat": "Cat" } }
            },
            "Cat": { "allOf": [ { "$ref": "#/components/schemas/Pet" } ] }"##,
    ));
    let pet = schema(&resolved, "Pet");
    let cat = schema(&resolved, "Cat");
    assert!(same(&discriminator(pet).mapping["cat"], cat));
    let ResolvedSchemaKind::AllOf { all_of } = &cat.schema_kind else {
        panic!("Cat is allOf");
    };
    assert!(all_of[0].is_recursive());
    assert!(points_at(&all_of[0], pet));
}

#[test]
fn a_schema_walks_its_kind_before_its_discriminator_mapping() {
    // A's property reaches C first, so the cycle between B and C closes at
    // B's edge; walking the mapping first would close it at C's instead.
    let resolved = resolve(&with_schemas(
        r##""A": {
              "type": "object",
              "properties": { "c": { "$ref": "#/components/schemas/C" } },
              "discriminator": { "propertyName": "kind", "mapping": { "b": "B" } }
            },
            "B": { "type": "object", "properties": { "c": { "$ref": "#/components/schemas/C" } } },
            "C": { "type": "object", "properties": { "b": { "$ref": "#/components/schemas/B" } } }"##,
    ));
    assert!(property(schema(&resolved, "B"), "c").is_recursive());
    assert!(!property(schema(&resolved, "C"), "b").is_recursive());
    assert!(!discriminator(schema(&resolved, "A")).mapping["b"].is_recursive());
}

#[test]
fn a_discriminator_without_a_mapping_keeps_its_property_name() {
    let resolved = resolve(&with_schemas(
        r##""Pet": { "discriminator": { "propertyName": "kind" }, "oneOf": [ { "type": "object" } ] }"##,
    ));
    let discriminator = discriminator(schema(&resolved, "Pet"));
    assert_eq!(discriminator.property_name, "kind");
    assert!(discriminator.mapping.is_empty());
}

#[test]
fn schema_data_is_copied_over_field_by_field() {
    let resolved = resolve(&with_schemas(
        r##""Pet": {
              "type": "string", "title": "Pet", "description": "a pet", "nullable": true,
              "readOnly": true, "writeOnly": true, "deprecated": true, "default": "cat",
              "example": "dog", "externalDocs": { "url": "https://example.com" }, "x-tag": 1
            }"##,
    ));
    let data = &schema(&resolved, "Pet").schema_data;
    assert_eq!(data.title.as_deref(), Some("Pet"));
    assert_eq!(data.description.as_deref(), Some("a pet"));
    assert!(data.nullable);
    assert!(data.read_only);
    assert!(data.write_only);
    assert!(data.deprecated);
    assert_eq!(data.default, Some(serde_json::json!("cat")));
    assert_eq!(data.example, Some(serde_json::json!("dog")));
    assert_eq!(
        data.external_docs.as_ref().map(|docs| docs.url.as_str()),
        Some("https://example.com")
    );
    assert_eq!(data.extensions["x-tag"], serde_json::json!(1));
    assert!(data.discriminator.is_none());
}
