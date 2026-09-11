//! `ResolvedOpenAPI` follows every `$ref` in a document, once per component.

mod common;

use common::spec;
use openapiv3::{OpenAPI, StatusCode};
use openapiv3_resolve::{
    ResolveError, ResolvedAdditionalProperties, ResolvedOpenAPI, ResolvedParameter,
    ResolvedParameterSchemaOrContent, ResolvedSchema, ResolvedSchemaKind, ResolvedType, Section,
};
use std::sync::Arc;

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

fn schema<'a>(resolved: &'a ResolvedOpenAPI, name: &str) -> &'a Arc<ResolvedSchema> {
    resolved
        .components
        .as_ref()
        .expect("has components")
        .schemas
        .get(name)
        .unwrap_or_else(|| panic!("schema {name} present"))
}

fn title(schema: &ResolvedSchema) -> Option<&str> {
    schema.schema_data.title.as_deref()
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
    let components = resolved.components.as_ref().expect("has components");

    assert_eq!(components.schemas.len(), 2);
    assert_eq!(components.responses.len(), 2);
    assert_eq!(components.parameters.len(), 2);
    assert_eq!(components.examples.len(), 2);
    assert_eq!(components.request_bodies.len(), 2);
    assert_eq!(components.headers.len(), 2);
    assert_eq!(components.security_schemes.len(), 2);
    assert_eq!(components.links.len(), 2);
    assert_eq!(components.callbacks.len(), 2);
    assert_eq!(resolved.paths.paths.len(), 3);
}

#[test]
fn a_component_that_is_itself_a_reference_shares_its_target() {
    let resolved = resolve(&spec());
    let components = resolved.components.as_ref().expect("has components");

    assert!(Arc::ptr_eq(
        schema(&resolved, "PetAlias"),
        schema(&resolved, "Pet")
    ));
    assert!(Arc::ptr_eq(
        &components.responses["PetListAlias"],
        &components.responses["PetList"]
    ));
    assert!(Arc::ptr_eq(
        &components.parameters["LimitAlias"],
        &components.parameters["Limit"]
    ));
    assert!(Arc::ptr_eq(
        &components.examples["OneAlias"],
        &components.examples["One"]
    ));
    assert!(Arc::ptr_eq(
        &components.request_bodies["CreatePetAlias"],
        &components.request_bodies["CreatePet"]
    ));
    assert!(Arc::ptr_eq(
        &components.headers["XRateAlias"],
        &components.headers["XRate"]
    ));
    assert!(Arc::ptr_eq(
        &components.security_schemes["ApiKeyAlias"],
        &components.security_schemes["ApiKey"]
    ));
    assert!(Arc::ptr_eq(
        &components.links["SelfAlias"],
        &components.links["Self"]
    ));
    assert!(Arc::ptr_eq(
        &components.callbacks["OnEventAlias"],
        &components.callbacks["OnEvent"]
    ));
    assert!(Arc::ptr_eq(
        &resolved.paths.paths["/alias"],
        &resolved.paths.paths["/pets"]
    ));
}

#[test]
fn every_reference_site_shares_the_component_it_names() {
    let resolved = resolve(&parse(SITES));
    let components = resolved.components.as_ref().expect("has components");
    let pet = &components.schemas["Pet"];
    let limit = &components.parameters["Limit"];
    let pet_list = &components.responses["PetList"];
    let x_rate = &components.headers["XRate"];
    let one = &components.examples["One"];

    let pets = &resolved.paths.paths["/pets"];
    let get = pets.get.as_ref().expect("GET /pets");
    assert!(Arc::ptr_eq(&pets.parameters[0], limit));
    assert!(Arc::ptr_eq(&get.parameters[0], limit));
    assert!(Arc::ptr_eq(
        get.request_body.as_ref().expect("body"),
        &components.request_bodies["CreatePet"]
    ));
    assert!(Arc::ptr_eq(
        get.responses.default.as_ref().expect("default"),
        pet_list
    ));
    assert!(Arc::ptr_eq(
        &get.responses.responses[&StatusCode::Code(200)],
        pet_list
    ));

    let created = &get.responses.responses[&StatusCode::Code(201)];
    assert!(Arc::ptr_eq(&created.headers["X-Rate"], x_rate));
    assert!(Arc::ptr_eq(
        &created.links["self"],
        &components.links["Self"]
    ));
    let media = &created.content["application/json"];
    assert!(Arc::ptr_eq(media.schema.as_ref().expect("schema"), pet));
    assert!(Arc::ptr_eq(&media.examples["one"], one));
    assert!(Arc::ptr_eq(
        &media.encoding["field"].headers["X-Rate"],
        x_rate
    ));

    let callback_post = get.callbacks["onEvent"]["{$request.body#/url}"]
        .post
        .as_ref()
        .expect("callback POST");
    assert!(Arc::ptr_eq(&callback_post.parameters[0], limit));
    assert!(Arc::ptr_eq(
        &callback_post.responses.responses[&StatusCode::Code(200)],
        pet_list
    ));
    assert!(Arc::ptr_eq(
        &components.callbacks["OnEvent"]["{$request.body#/url}"].parameters[0],
        limit
    ));

    let body = &pet_list.content["application/json"];
    assert!(Arc::ptr_eq(body.schema.as_ref().expect("schema"), pet));
    let ResolvedParameterSchemaOrContent::Schema(limit_schema) = &limit.parameter_data().format
    else {
        panic!("Limit has a schema");
    };
    assert!(Arc::ptr_eq(limit_schema, pet));
    assert!(Arc::ptr_eq(&limit.parameter_data().examples["one"], one));
    let ResolvedParameterSchemaOrContent::Content(content) =
        &components.parameters["Body"].parameter_data().format
    else {
        panic!("Body has content");
    };
    assert!(Arc::ptr_eq(
        content["application/json"].schema.as_ref().expect("schema"),
        pet
    ));
    let ResolvedParameterSchemaOrContent::Schema(header_schema) = &x_rate.format else {
        panic!("XRate has a schema");
    };
    assert!(Arc::ptr_eq(header_schema, pet));
    assert!(Arc::ptr_eq(&x_rate.examples["one"], one));
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
    let parameters = &resolved.components.as_ref().expect("components").parameters;
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
    assert!(Arc::ptr_eq(&object.properties["pet"], pet));
    let Some(ResolvedAdditionalProperties::Schema(additional)) = &object.additional_properties
    else {
        panic!("Object has additional properties");
    };
    assert!(Arc::ptr_eq(additional, pet));

    let ResolvedSchemaKind::Type(ResolvedType::Array(array)) =
        &schema(&resolved, "Array").schema_kind
    else {
        panic!("Array is an array");
    };
    assert!(Arc::ptr_eq(array.items.as_ref().expect("items"), pet));

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
        assert!(Arc::ptr_eq(&alternatives[0], pet), "{name}");
    }

    let ResolvedSchemaKind::Not { not } = &schema(&resolved, "Not").schema_kind else {
        panic!("Not is a negation");
    };
    assert!(Arc::ptr_eq(not, pet));
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
    assert!(Arc::ptr_eq(&any.properties["pet"], pet));
    let Some(ResolvedAdditionalProperties::Schema(additional)) = &any.additional_properties else {
        panic!("Any has additional properties");
    };
    assert!(Arc::ptr_eq(additional, pet));
    assert!(Arc::ptr_eq(any.items.as_ref().expect("items"), pet));
    assert!(Arc::ptr_eq(&any.one_of[0], pet));
    assert!(Arc::ptr_eq(&any.all_of[0], pet));
    assert!(Arc::ptr_eq(&any.any_of[0], pet));
    assert!(Arc::ptr_eq(any.not.as_ref().expect("not"), pet));
}

#[test]
fn keeps_a_boolean_additional_properties() {
    let resolved = resolve(&with_schemas(
        r##""Open": { "type": "object", "additionalProperties": true },
            "Closed": { "type": "object", "additionalProperties": false }"##,
    ));
    let additional = |name: &str| match &schema(&resolved, name).schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Object(object)) => {
            object.additional_properties.clone()
        }
        other => panic!("{name} resolved to {other:?}"),
    };
    assert_eq!(
        additional("Open"),
        Some(ResolvedAdditionalProperties::Any(true))
    );
    assert_eq!(
        additional("Closed"),
        Some(ResolvedAdditionalProperties::Any(false))
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
    let components = resolved.components.as_ref().expect("components");
    let ResolvedParameterSchemaOrContent::Content(content) = &components.headers["X-Pet"].format
    else {
        panic!("X-Pet has content");
    };
    assert!(Arc::ptr_eq(
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
    let items = |name: &str| match &schema(&resolved, name).schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Array(array)) => {
            Arc::clone(array.items.as_ref().expect("items"))
        }
        other => panic!("{name} resolved to {other:?}"),
    };
    assert_eq!(title(&items("A")), Some("inline"));
    assert_eq!(items("A"), items("B"));
    assert!(!Arc::ptr_eq(&items("A"), &items("B")));
}

#[test]
fn a_chain_of_references_resolves_to_the_item_at_its_end() {
    let resolved = resolve(&with_schemas(
        r##""A": { "$ref": "#/components/schemas/B" },
            "B": { "$ref": "#/components/schemas/C" },
            "C": { "title": "end", "type": "string" }"##,
    ));
    assert_eq!(title(schema(&resolved, "A")), Some("end"));
    assert!(Arc::ptr_eq(schema(&resolved, "A"), schema(&resolved, "C")));
    assert!(Arc::ptr_eq(schema(&resolved, "B"), schema(&resolved, "C")));
}

#[test]
fn a_document_without_components_resolves_to_none() {
    let resolved = resolve(&parse(
        r#"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{}}"#,
    ));
    assert_eq!(resolved.components, None);
    assert!(resolved.paths.paths.is_empty());
}

#[test]
fn a_document_without_references_copies_over_unchanged() {
    let openapi = parse(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},
            "paths":{"/pets":{"get":{"operationId":"list","responses":{"200":{"description":"ok"}}}}},
            "tags":[{"name":"pets"}],"x-root":true}"##,
    );
    let resolved = resolve(&openapi);
    assert_eq!(resolved.openapi, openapi.openapi);
    assert_eq!(resolved.info, openapi.info);
    assert_eq!(resolved.tags, openapi.tags);
    assert_eq!(resolved.extensions, openapi.extensions);
    let get = resolved.paths.paths["/pets"].get.as_ref().expect("GET");
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
    let methods: Vec<&str> = resolved.paths.paths["/pets"]
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
fn a_schema_that_contains_itself_is_a_cycle() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""Node": { "type": "object",
                      "properties": { "next": { "$ref": "#/components/schemas/Node" } } }"##,
    ))
    .expect_err("cyclic");
    assert_eq!(
        error,
        ResolveError::CyclicReference {
            reference: "#/components/schemas/Node".to_owned()
        }
    );
}

#[test]
fn a_cycle_through_another_component_is_reported_where_it_closes() {
    let error = ResolvedOpenAPI::try_from(&with_schemas(
        r##""A": { "type": "array", "items": { "$ref": "#/components/schemas/B" } },
            "B": { "type": "array", "items": { "$ref": "#/components/schemas/A" } }"##,
    ))
    .expect_err("cyclic");
    assert_eq!(
        error,
        ResolveError::CyclicReference {
            reference: "#/components/schemas/A".to_owned()
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
    assert!(Arc::ptr_eq(&all_of[0], schema(&resolved, "Left")));
    assert!(Arc::ptr_eq(&all_of[1], schema(&resolved, "Right")));
}

#[test]
fn resolving_by_value_matches_resolving_by_reference() {
    let openapi = spec();
    let by_reference = resolve(&openapi);
    let by_value = ResolvedOpenAPI::try_from(openapi).expect("document resolves");
    assert_eq!(by_reference, by_value);
}
