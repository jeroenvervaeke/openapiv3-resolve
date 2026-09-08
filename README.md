# openapiv3-resolve

[![crates.io](https://img.shields.io/crates/v/openapiv3-resolve.svg)](https://crates.io/crates/openapiv3-resolve)
[![docs.rs](https://docs.rs/openapiv3-resolve/badge.svg)](https://docs.rs/openapiv3-resolve)

Reference resolution helpers for the [`openapiv3`](https://crates.io/crates/openapiv3) crate.

This crate adds traits that resolve `$ref` pointers (e.g.
`#/components/schemas/Pet`) against an `OpenAPI` document, returning a
borrowed reference to the resolved item. It walks chains of references
transparently, supports the boxed variant `ReferenceOr<Box<T>>` used by fields
such as `ArrayType::items`, and reports why a reference could not be resolved
instead of collapsing every failure into "not found".

## Usage

```rust
use openapiv3::{OpenAPI, StatusCode};
use openapiv3_resolve::{ResolveOptionalWithOpenAPI, ResolveWithOpenAPI};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let openapi: OpenAPI = serde_json::from_str(spec)?;

    let path = openapi.paths.paths.get("/pets").ok_or("no /pets")?;
    let get = path.resolve(&openapi)?.get.as_ref().ok_or("no GET")?;

    // `responses` holds a `ReferenceOr<Response>`; `resolve` follows the `$ref`.
    let response = get
        .responses
        .responses
        .get(&StatusCode::Code(200))
        .ok_or("no 200")?
        .resolve(&openapi)?;

    // `schema` is an `Option<ReferenceOr<Schema>>`: absent is not an error,
    // so it gets its own method.
    let media = response.content.get("application/json").ok_or("no JSON body")?;
    let schema = media.schema.resolve_optional(&openapi)?.ok_or("untyped body")?;

    assert_eq!(schema.schema_data.title.as_deref(), Some("Pet"));
    Ok(())
}
```

## Traits

- `Resolve` — implemented on `OpenAPI`. `openapi.resolve_ref::<Schema>(ptr)`
  takes a full pointer like `#/components/schemas/Pet`. The type argument
  decides which section is searched.
- `ResolveWithOpenAPI<T>` — implemented on `ReferenceOr<T>` and
  `ReferenceOr<Box<T>>`; returns the inline item, or resolves the reference.
- `ResolveOptionalWithOpenAPI<T>` — implemented on `Option<R>` for any
  resolvable `R`; `resolve_optional` returns `Ok(None)` for an absent field
  and an error only for a reference that is present but broken.

Resolvable targets are the nine `#/components` sections plus `#/paths`
(`#/paths/~1pets`, with RFC 6901 escaping). The `Component` trait that lists
them is sealed.

## Errors

Every failure is a distinct [`ResolveError`](https://docs.rs/openapiv3-resolve/latest/openapiv3_resolve/enum.ResolveError.html) variant, so a caller can tell a
typo in the document (`NotFound`, `SectionMismatch`) from a reference this
crate structurally does not follow (`ExternalDocument`, `PointerTooDeep`) from
a document that is broken (`ReferenceChainTooLong`, which is what a cycle
looks like).

Reference chains are walked iteratively and capped at `MAX_REFERENCE_HOPS`, so
a cyclic document returns an error rather than overflowing the stack.

## Thread safety

`OpenAPI` and every resolvable component are `Send + Sync`, resolution takes
`&self` and allocates nothing, and the returned borrow is `Send + Sync` too —
so a resolved reference can be held across an `.await` in a `Send` future.

## Minimum supported Rust version

1.85. Bumping the MSRV is a minor version bump.

## License

Licensed under the [MIT license](https://github.com/jeroenvervaeke/openapiv3-resolve/blob/master/LICENSE).
