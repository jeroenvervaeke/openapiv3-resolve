//! A spec that exercises every resolvable section, parsed through `serde_json`.
//!
//! Going through serde matters: hand-building `Components` in Rust bypasses
//! `#[serde(rename_all = "camelCase")]`, which is exactly how `requestBodies`
//! and `securitySchemes` stayed unresolvable while tests passed.

use openapiv3::OpenAPI;

pub const SPEC: &str = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Everything", "version": "1.0.0" },
  "paths": {
    "/pets": { "get": { "responses": {} } },
    "/pets/{id}": { "get": { "responses": {} } },
    "/alias": { "$ref": "#/paths/~1pets" }
  },
  "components": {
    "schemas": {
      "Pet": { "title": "Pet", "type": "string" },
      "PetAlias": { "$ref": "#/components/schemas/Pet" }
    },
    "responses": {
      "PetList": { "description": "pets" },
      "PetListAlias": { "$ref": "#/components/responses/PetList" }
    },
    "parameters": {
      "Limit": { "name": "limit", "in": "query", "schema": { "type": "integer" } },
      "LimitAlias": { "$ref": "#/components/parameters/Limit" }
    },
    "examples": {
      "One": { "value": 1 },
      "OneAlias": { "$ref": "#/components/examples/One" }
    },
    "requestBodies": {
      "CreatePet": { "content": {} },
      "CreatePetAlias": { "$ref": "#/components/requestBodies/CreatePet" }
    },
    "headers": {
      "XRate": { "schema": { "type": "string" } },
      "XRateAlias": { "$ref": "#/components/headers/XRate" }
    },
    "securitySchemes": {
      "ApiKey": { "type": "apiKey", "name": "key", "in": "header" },
      "ApiKeyAlias": { "$ref": "#/components/securitySchemes/ApiKey" }
    },
    "links": {
      "Self": { "operationId": "getPets" },
      "SelfAlias": { "$ref": "#/components/links/Self" }
    },
    "callbacks": {
      "OnEvent": {},
      "OnEventAlias": { "$ref": "#/components/callbacks/OnEvent" }
    }
  }
}"##;

pub fn spec() -> OpenAPI {
    serde_json::from_str(SPEC).expect("fixture spec parses")
}
