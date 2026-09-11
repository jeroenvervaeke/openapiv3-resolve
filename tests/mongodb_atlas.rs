//! Resolves a large real-world document: the MongoDB Atlas Administration API.
//!
//! The spec is fetched from a pinned commit, so the numbers below are stable,
//! and cached under the target directory so it is downloaded once per
//! checkout rather than once per run.

use openapiv3::OpenAPI;
use openapiv3_resolve::{
    NestedSchema, ResolvedOpenAPI, ResolvedSchema, ResolvedSchemaKind, ResolvedType, SchemaGuard,
    Shared,
};
use std::path::PathBuf;
use std::sync::OnceLock;

const COMMIT: &str = "bedfe9bbd397bf0f6dfd0807e62cd0d30972eb85";

fn url() -> String {
    format!("https://raw.githubusercontent.com/mongodb/openapi/{COMMIT}/openapi/.raw/v2.yaml")
}

fn cache_path() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mongodb-atlas-{COMMIT}.yaml"))
}

fn download() -> String {
    let cache = cache_path();
    if let Ok(text) = std::fs::read_to_string(&cache) {
        return text;
    }
    let text = ureq::get(url())
        .call()
        .expect("spec downloads")
        .body_mut()
        .read_to_string()
        .expect("spec is text");
    // Written whole then renamed, so a parallel test never reads a partial file.
    let partial = cache.with_extension("partial");
    std::fs::write(&partial, &text).expect("cache is writable");
    std::fs::rename(&partial, &cache).expect("cache is writable");
    text
}

fn resolved() -> &'static ResolvedOpenAPI {
    static RESOLVED: OnceLock<ResolvedOpenAPI> = OnceLock::new();
    RESOLVED.get_or_init(|| {
        let openapi: OpenAPI = serde_yaml::from_str(&download()).expect("spec parses");
        ResolvedOpenAPI::try_from(&openapi).expect("spec resolves")
    })
}

fn schema(name: &str) -> &'static Shared<ResolvedSchema> {
    resolved()
        .components()
        .expect("has components")
        .schemas
        .get(name)
        .unwrap_or_else(|| panic!("schema {name} present"))
}

fn property<'a>(schema: &'a ResolvedSchema, name: &str) -> &'a NestedSchema {
    match &schema.schema_kind {
        ResolvedSchemaKind::Type(ResolvedType::Object(object)) => &object.properties[name],
        other => panic!("not an object: {other:?}"),
    }
}

fn points_at(nested: &NestedSchema, schema: &Shared<ResolvedSchema>) -> bool {
    std::ptr::eq(SchemaGuard::as_ptr(&nested.get()), Shared::as_ptr(schema))
}

/// The `oneOf` list, whether upstream parsed the schema as a pure `oneOf` or,
/// because it also carries `type`/`properties`, as an untyped schema.
fn one_of(schema: &ResolvedSchema) -> &[NestedSchema] {
    match &schema.schema_kind {
        ResolvedSchemaKind::OneOf { one_of } => one_of,
        ResolvedSchemaKind::Any(any) => &any.one_of,
        other => panic!("no oneOf: {other:?}"),
    }
}

fn all_of(schema: &ResolvedSchema) -> &[NestedSchema] {
    match &schema.schema_kind {
        ResolvedSchemaKind::AllOf { all_of } => all_of,
        ResolvedSchemaKind::Any(any) => &any.all_of,
        other => panic!("no allOf: {other:?}"),
    }
}

#[test]
fn resolves_the_whole_document() {
    let resolved = resolved();
    let components = resolved.components().expect("has components");
    assert_eq!(resolved.paths().paths.len(), 363);
    assert_eq!(components.schemas.len(), 1232);
    let operations: usize = resolved
        .paths()
        .paths
        .values()
        .map(|path| path.iter().count())
        .sum();
    assert_eq!(operations, 580);
}

#[test]
fn a_self_referencing_property_is_a_recursive_edge() {
    let principal = schema("Principal");
    let on_behalf_of = property(principal, "onBehalfOf");
    assert!(on_behalf_of.is_recursive());
    assert!(points_at(on_behalf_of, principal));
}

#[test]
fn a_self_referencing_array_is_a_recursive_edge() {
    let invoice = schema("BillingInvoice");
    let ResolvedSchemaKind::Type(ResolvedType::Array(linked)) =
        &property(invoice, "linkedInvoices").get().schema_kind
    else {
        panic!("linkedInvoices is an array");
    };
    let items = linked.items.as_ref().expect("has items");
    assert!(items.is_recursive());
    assert!(points_at(items, invoice));
}

#[test]
fn a_polymorphic_cycle_closes_on_whichever_side_comes_second() {
    // `CloudRegionConfig` lists its subtypes in `oneOf`; each subtype extends
    // it through `allOf`. Where the cycle closes follows document order:
    // `AWSRegionConfig` precedes the parent, so the parent is first reached
    // from inside it and the parent's `oneOf` entry is the recursive edge;
    // `TenantRegionConfig` follows the parent, so its `allOf` entry is.
    let parent = schema("CloudRegionConfig");
    let one_of = one_of(parent);
    let base_of = |name: &str| all_of(schema(name)).first().expect("extends a base");

    let aws = schema("AWSRegionConfig");
    let aws_base = base_of("AWSRegionConfig");
    assert!(!aws_base.is_recursive());
    assert!(points_at(aws_base, parent));
    let aws_entry = one_of
        .iter()
        .find(|entry| points_at(entry, aws))
        .expect("lists AWS");
    assert!(aws_entry.is_recursive());

    let tenant = schema("TenantRegionConfig");
    let tenant_base = base_of("TenantRegionConfig");
    assert!(tenant_base.is_recursive());
    assert!(points_at(tenant_base, parent));
    let tenant_entry = one_of
        .iter()
        .find(|entry| points_at(entry, tenant))
        .expect("lists Tenant");
    assert!(!tenant_entry.is_recursive());
}
