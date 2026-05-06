# openapiv3-resolve

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

let openapi: OpenAPI = serde_yaml::from_str(spec).unwrap();

let path = openapi.paths.paths.get("/pets").unwrap().as_item().unwrap();
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

let schema = response
    .content
    .get("application/json")
    .unwrap()
    .schema
    .resolve(&openapi)
    .unwrap();
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

## License

Licensed under either of

- Apache License, Version 2.0
- MIT license

at your option.
