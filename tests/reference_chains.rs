//! Chains of `$ref`s terminate: long ones resolve, cyclic ones error.
//!
//! Before the walk was bounded, the cyclic cases here overflowed the stack in
//! debug and spun forever in release — neither is catchable, so both took the
//! whole process down.

use openapiv3::{OpenAPI, Schema, SchemaKind, Type};
use openapiv3_resolve::{Resolve, ResolveError, ResolveWithOpenAPI, MAX_REFERENCE_HOPS};

/// A document whose schemas form `s0 -> s1 -> ... -> s{hops}`, ending in an item.
fn chain(hops: usize) -> OpenAPI {
    let mut schemas = String::new();
    for hop in 0..hops {
        schemas.push_str(&format!(
            r##""s{hop}": {{ "$ref": "#/components/schemas/s{}" }},"##,
            hop + 1
        ));
    }
    schemas.push_str(&format!(
        r##""s{hops}": {{ "title": "end", "type": "string" }}"##
    ));
    spec(&schemas)
}

fn spec(schemas: &str) -> OpenAPI {
    serde_json::from_str(&format!(
        r##"{{"openapi":"3.0.0","info":{{"title":"t","version":"1"}},"paths":{{}},
            "components":{{"schemas":{{{schemas}}}}}}}"##
    ))
    .expect("fixture spec parses")
}

#[test]
fn resolves_the_longest_chain_that_stays_within_the_hop_limit() {
    let openapi = chain(MAX_REFERENCE_HOPS - 1);
    let schema = openapi
        .resolve_ref::<Schema>("#/components/schemas/s0")
        .expect("chain within the limit resolves");
    assert_eq!(schema.schema_data.title.as_deref(), Some("end"));
}

#[test]
fn gives_up_on_a_chain_one_hop_past_the_limit() {
    let openapi = chain(MAX_REFERENCE_HOPS);
    assert_eq!(
        openapi.resolve_ref::<Schema>("#/components/schemas/s0"),
        Err(ResolveError::ReferenceChainTooLong {
            reference: "#/components/schemas/s0".to_owned(),
            max_hops: MAX_REFERENCE_HOPS,
        })
    );
}

#[test]
fn gives_up_on_a_self_reference() {
    let openapi = spec(r##""A": { "$ref": "#/components/schemas/A" }"##);
    assert!(matches!(
        openapi.resolve_ref::<Schema>("#/components/schemas/A"),
        Err(ResolveError::ReferenceChainTooLong { .. })
    ));
}

#[test]
fn gives_up_on_a_two_node_cycle() {
    let openapi = spec(
        r##""A": { "$ref": "#/components/schemas/B" },
           "B": { "$ref": "#/components/schemas/A" }"##,
    );
    assert!(matches!(
        openapi.resolve_ref::<Schema>("#/components/schemas/A"),
        Err(ResolveError::ReferenceChainTooLong { .. })
    ));
}

#[test]
fn gives_up_on_a_cycle_reached_through_a_boxed_reference() {
    let openapi = spec(
        r##""List": { "type": "array", "items": { "$ref": "#/components/schemas/Loop" } },
           "Loop": { "$ref": "#/components/schemas/Loop" }"##,
    );
    let list = openapi
        .resolve_ref::<Schema>("#/components/schemas/List")
        .expect("the array itself resolves");
    let SchemaKind::Type(Type::Array(array)) = &list.schema_kind else {
        panic!("expected an array schema");
    };
    let items = array.items.as_ref().expect("array has items");

    assert!(matches!(
        items.resolve(&openapi),
        Err(ResolveError::ReferenceChainTooLong { .. })
    ));
}

#[test]
fn a_self_recursive_schema_is_not_a_cycle() {
    // `Node.children: [Node]` is legal and common; the reference terminates at
    // an inline item, so it must resolve rather than trip the hop limit.
    let openapi = spec(
        r##""Node": { "title": "Node", "type": "array",
                     "items": { "$ref": "#/components/schemas/Node" } }"##,
    );
    let node = openapi
        .resolve_ref::<Schema>("#/components/schemas/Node")
        .expect("recursive schema resolves");
    let SchemaKind::Type(Type::Array(array)) = &node.schema_kind else {
        panic!("expected an array schema");
    };
    let items = array.items.as_ref().expect("array has items");
    let resolved = items.resolve(&openapi).expect("items resolve back to Node");

    assert_eq!(resolved.schema_data.title.as_deref(), Some("Node"));
}
