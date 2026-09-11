//! Resolution must stay usable from threads and from `Send` futures.
//!
//! These are compile-time assertions: if a future version stores a cache
//! behind a `RefCell`, or upstream swaps an `Rc` in, this file stops building
//! instead of silently narrowing what callers can do.

use openapiv3::{
    Callback, Example, Header, Link, OpenAPI, Parameter, PathItem, ReferenceOr, RequestBody,
    Response, Schema, SecurityScheme,
};
use openapiv3_resolve::{Resolve, ResolveError};

const fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn documents_and_components_are_send_and_sync() {
    assert_send_sync::<OpenAPI>();
    assert_send_sync::<ResolveError>();
    assert_send_sync::<Callback>();
    assert_send_sync::<Example>();
    assert_send_sync::<Header>();
    assert_send_sync::<Link>();
    assert_send_sync::<Parameter>();
    assert_send_sync::<PathItem>();
    assert_send_sync::<RequestBody>();
    assert_send_sync::<Response>();
    assert_send_sync::<Schema>();
    assert_send_sync::<SecurityScheme>();
    assert_send_sync::<ReferenceOr<Schema>>();
    assert_send_sync::<ReferenceOr<Box<Schema>>>();
}

#[test]
fn a_resolved_borrow_is_send_and_sync() {
    assert_send_sync::<&Schema>();
    assert_send_sync::<Result<&Schema, ResolveError>>();
}

#[test]
fn a_future_holding_a_resolved_borrow_is_send() {
    fn assert_send<F: Send>(_: F) {}

    let openapi = OpenAPI::default();
    assert_send(async move {
        let resolved = openapi.resolve_ref::<Schema>("#/components/schemas/Pet");
        std::future::ready(()).await;
        let _ = resolved;
    });
}

#[test]
fn two_threads_can_resolve_against_one_document_concurrently() {
    let openapi: OpenAPI = serde_json::from_str(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{},
             "components":{"schemas":{"Pet":{"title":"Pet","type":"string"}}}}"##,
    )
    .expect("fixture spec parses");

    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let schema = openapi
                    .resolve_ref::<Schema>("#/components/schemas/Pet")
                    .expect("resolves");
                assert_eq!(schema.schema_data.title.as_deref(), Some("Pet"));
            });
        }
    });
}

#[test]
fn a_resolved_document_is_send_and_sync() {
    assert_send_sync::<openapiv3_resolve::ResolvedOpenAPI>();
}

#[test]
fn a_nested_schema_edge_is_send_and_sync() {
    assert_send_sync::<openapiv3_resolve::NestedSchema>();
    assert_send_sync::<openapiv3_resolve::Shared<openapiv3_resolve::ResolvedSchema>>();
    assert_send_sync::<openapiv3_resolve::SchemaGuard<'_>>();
}

#[test]
fn threads_can_follow_a_recursive_edge_of_a_shared_document() {
    use openapiv3_resolve::{
        ResolvedOpenAPI, ResolvedSchemaKind, ResolvedType, SchemaGuard, Shared,
    };
    use std::sync::Arc;

    let openapi: OpenAPI = serde_json::from_str(
        r##"{"openapi":"3.0.0","info":{"title":"t","version":"1"},"paths":{},
             "components":{"schemas":{"Node":{"title":"Node","type":"object",
               "properties":{"next":{"$ref":"#/components/schemas/Node"}}}}}}"##,
    )
    .expect("fixture spec parses");
    let resolved = Arc::new(ResolvedOpenAPI::try_from(&openapi).expect("resolves"));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let resolved = Arc::clone(&resolved);
            std::thread::spawn(move || {
                let node = &resolved.components().expect("components").schemas["Node"];
                let ResolvedSchemaKind::Type(ResolvedType::Object(object)) = &node.schema_kind
                else {
                    panic!("Node is an object");
                };
                let next = object.properties["next"].get();
                assert!(std::ptr::eq(
                    SchemaGuard::as_ptr(&next),
                    Shared::as_ptr(node)
                ));
                assert_eq!(next.schema_data.title.as_deref(), Some("Node"));
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread completes");
    }
}
