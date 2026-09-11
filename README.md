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

Resolvable targets are the nine `#/components` sections plus `#/paths`, listed
by the `Section` enum. A `$ref` is read as a URI reference: the fragment is
percent-decoded first, then RFC 6901 unescaped, so the path `/pets/{id}` is
reachable as `#/paths/~1pets~1%7Bid%7D`.

The `Component` trait that maps a Rust type to its section is sealed, so the
two can never disagree. Note that `openapiv3::Callback` is a transparent alias
for `IndexMap<String, PathItem>` rather than a distinct type, so any value of
that shape resolves as a callback.

## Resolving a whole document

`ResolvedOpenAPI` is the document with every `$ref` followed up front: a
mirror of `openapiv3::OpenAPI` in which each `ReferenceOr<T>` has become an
`Arc<ResolvedT>` (or `Arc<T>` for `Example`, `Link` and `SecurityScheme`,
which hold no references). Every reference to the same component shares one
`Arc`, so `Arc::ptr_eq` tells whether two sites named the same component, and
`components` holds those same `Arc`s.

```rust
use openapiv3::OpenAPI;
use openapiv3_resolve::{ResolvedOpenAPI, ResolvedParameterSchemaOrContent};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = r##"{
      "openapi": "3.0.0",
      "info": { "title": "Pets", "version": "1.0.0" },
      "paths": {
        "/pets": {
          "get": {
            "parameters": [ { "$ref": "#/components/parameters/Limit" } ],
            "responses": {}
          }
        }
      },
      "components": {
        "parameters": {
          "Limit": {
            "name": "limit", "in": "query",
            "schema": { "$ref": "#/components/schemas/Limit" }
          }
        },
        "schemas": { "Limit": { "type": "integer" } }
      }
    }"##;

    let openapi: OpenAPI = serde_json::from_str(spec)?;
    let resolved = ResolvedOpenAPI::try_from(&openapi)?;

    let get = resolved.paths.paths["/pets"].get.as_ref().ok_or("no GET")?;
    let limit = get.parameters.first().ok_or("no parameter")?;
    let ResolvedParameterSchemaOrContent::Schema(schema) = &limit.parameter_data().format else {
        return Err("limit has content, not a schema".into());
    };

    let components = resolved.components.as_ref().ok_or("no components")?;
    assert!(Arc::ptr_eq(limit, &components.parameters["Limit"]));
    assert!(Arc::ptr_eq(schema, &components.schemas["Limit"]));
    Ok(())
}
```

Resolution fails on the first reference that does not resolve, with the same
`ResolveError` the borrowing traits return.

### Recursive schemas

A schema nested inside another schema is a `NestedSchema`, which is
`Schema(Arc<ResolvedSchema>)` except where a `$ref` points back at a schema
that contains it: a tree node whose children are nodes, say. That edge is
`Recursive(Weak<ResolvedSchema>)`, because a cycle of `Arc`s would never be
freed. `upgrade()` turns either variant into an `Arc`, and succeeds for a
recursive edge as long as the document (or the schema it points at) is alive.
`is_recursive()` tells a code generator where the indirection goes.

Which edge of a cycle is the recursive one is decided by document order: the
first `$ref`, walking `components` then `paths`, that closes the cycle. A
component that contains itself in any other way (a header whose content
encoding names that same header is the only one a document can express) has
no finite tree form and fails with `CyclicReference`.

## Errors

Every failure is a distinct [`ResolveError`](https://docs.rs/openapiv3-resolve/latest/openapiv3_resolve/enum.ResolveError.html) variant, so a caller can tell a
typo in the document (`NotFound`, `SectionMismatch`) from a reference this
crate structurally does not follow (`ExternalDocument`, `PointerTooDeep`) from
a document that is broken (`ReferenceChainTooLong`, which is what a cycle of
bare `$ref`s looks like, and `CyclicReference` for a non-schema component
that contains itself, which only a full resolution can detect).

Reference chains are walked iteratively and capped at `MAX_REFERENCE_HOPS`, so
a cyclic document returns an error rather than overflowing the stack.

## Thread safety

`OpenAPI` and every resolvable component are `Send + Sync`, resolution takes
`&self`, and the returned borrow is `Send + Sync` too — so a resolved reference
can be held across an `.await` in a `Send` future.

Resolving a `#/components/...` pointer allocates nothing, however long the
reference chain. Pointers carrying an escape (`~0`, `~1`, `%XX`) are the
exception: the decoded name has to be built. Both are pinned by a test.

## Minimum supported Rust version

1.85, which is the floor `indexmap` imposes rather than anything this crate
needs, and it is checked by its own CI job. A dependency raising its MSRV
raises this one; that is a minor version bump.

## License

Licensed under the [MIT license](https://github.com/jeroenvervaeke/openapiv3-resolve/blob/master/LICENSE).
