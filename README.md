# openapiv3-resolve

[![crates.io](https://img.shields.io/crates/v/openapiv3-resolve.svg)](https://crates.io/crates/openapiv3-resolve)
[![docs.rs](https://docs.rs/openapiv3-resolve/badge.svg)](https://docs.rs/openapiv3-resolve)

Reference resolution helpers for the [`openapiv3`](https://crates.io/crates/openapiv3) crate.

This crate adds traits that resolve `$ref` pointers (e.g.
`#/components/schemas/Pet`) against an `OpenAPI` document, returning a
borrowed reference to the resolved item. It walks chains of references
transparently and supports the boxed variant `ReferenceOr<Box<T>>` used by
fields such as `ArrayType::items`.

## Usage

```rust
use openapiv3::OpenAPI;
use openapiv3_resolve::ResolveWithOpenAPI;

let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "Pets", "version": "1.0.0" },
  "paths": {
    "/pets": {
      "get": {
        "responses": { "200": { "$ref": "#/components/responses/PetList" } }
      }
    }
  },
  "components": {
    "responses": {
      "PetList": {
        "description": "a list of pets",
        "content": {
          "application/json": { "schema": { "$ref": "#/components/schemas/Pet" } }
        }
      }
    },
    "schemas": {
      "Pet": { "title": "Pet", "type": "string" }
    }
  }
}"##;

let openapi: OpenAPI = serde_json::from_str(spec).unwrap();

let path = openapi.paths.paths.get("/pets").unwrap().as_item().unwrap();

// `responses` holds a `ReferenceOr<Response>`; `resolve` follows the `$ref`.
let response = path
    .get
    .as_ref()
    .unwrap()
    .responses
    .responses
    .get(&openapiv3::StatusCode::Code(200))
    .unwrap()
    .resolve(&openapi)
    .unwrap();

// `schema` is an `Option<ReferenceOr<Schema>>`; `resolve` handles both.
let schema = response
    .content
    .get("application/json")
    .unwrap()
    .schema
    .resolve(&openapi)
    .unwrap();

assert_eq!(schema.schema_data.title.as_deref(), Some("Pet"));
```

## Traits

- `Resolve<T>` — implemented on `OpenAPI` for each component type, takes a
  full pointer like `#/components/schemas/Pet`.
- `ResolveWithOpenAPI<T>` — implemented on `ReferenceOr<T>`,
  `ReferenceOr<Box<T>>` and their `Option<...>` variants; resolves the
  reference (or returns the inline item) using a borrowed `OpenAPI`.
- `ResolveWithOpenAPIAndPath<T>` — implemented on `Components` and
  `IndexMap<String, ReferenceOr<T>>`; used internally and when walking a
  pointer relative to a sub-document.

Every method returns `Option`: `None` means the pointer was malformed, named
an unsupported location, or pointed at something that is not present in the
document.

## Minimum supported Rust version

1.85. Bumping the MSRV is a minor version bump.

## License

Licensed under the [MIT license](https://github.com/jeroenvervaeke/openapiv3-resolve/blob/master/LICENSE).
