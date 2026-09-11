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
