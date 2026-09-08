//! Every resolvable section resolves, both directly and through a `$ref` alias.

mod common;

use common::spec;
use openapiv3::ReferenceOr;
use openapiv3::{
    Callback, Example, Header, Link, Parameter, PathItem, RequestBody, Response, Schema,
    SecurityScheme,
};
use openapiv3_resolve::{Component, Resolve};

macro_rules! section_resolves {
    ($direct:ident, $via_ref:ident, $ty:ty, $item:literal, $name:literal, $alias:literal) => {
        #[test]
        fn $direct() {
            let openapi = spec();
            let resolved = openapi.resolve_ref::<$ty>($item).expect($item);

            let stored = <$ty as Component>::section(&openapi)
                .expect("section is present")
                .get($name)
                .expect("fixture holds this entry");
            let ReferenceOr::Item(expected) = stored else {
                panic!("fixture entry is a reference, not an item");
            };
            assert!(
                std::ptr::eq(resolved, expected),
                "resolved a different item"
            );
        }

        #[test]
        fn $via_ref() {
            let openapi = spec();
            let direct = openapi.resolve_ref::<$ty>($item).expect($item);
            let aliased = openapi.resolve_ref::<$ty>($alias).expect($alias);
            assert!(
                std::ptr::eq(direct, aliased),
                "alias resolved to a different item"
            );
        }
    };
}

section_resolves!(
    schema,
    schema_via_ref,
    Schema,
    "#/components/schemas/Pet",
    "Pet",
    "#/components/schemas/PetAlias"
);
section_resolves!(
    response,
    response_via_ref,
    Response,
    "#/components/responses/PetList",
    "PetList",
    "#/components/responses/PetListAlias"
);
section_resolves!(
    parameter,
    parameter_via_ref,
    Parameter,
    "#/components/parameters/Limit",
    "Limit",
    "#/components/parameters/LimitAlias"
);
section_resolves!(
    example,
    example_via_ref,
    Example,
    "#/components/examples/One",
    "One",
    "#/components/examples/OneAlias"
);
section_resolves!(
    request_body,
    request_body_via_ref,
    RequestBody,
    "#/components/requestBodies/CreatePet",
    "CreatePet",
    "#/components/requestBodies/CreatePetAlias"
);
section_resolves!(
    header,
    header_via_ref,
    Header,
    "#/components/headers/XRate",
    "XRate",
    "#/components/headers/XRateAlias"
);
section_resolves!(
    security_scheme,
    security_scheme_via_ref,
    SecurityScheme,
    "#/components/securitySchemes/ApiKey",
    "ApiKey",
    "#/components/securitySchemes/ApiKeyAlias"
);
section_resolves!(
    link,
    link_via_ref,
    Link,
    "#/components/links/Self",
    "Self",
    "#/components/links/SelfAlias"
);
section_resolves!(
    callback,
    callback_via_ref,
    Callback,
    "#/components/callbacks/OnEvent",
    "OnEvent",
    "#/components/callbacks/OnEventAlias"
);
section_resolves!(
    path_item,
    path_item_via_ref,
    PathItem,
    "#/paths/~1pets",
    "/pets",
    "#/paths/~1alias"
);

#[test]
fn resolves_a_templated_path_through_its_percent_encoded_pointer() {
    // `{` and `}` cannot appear literally in a URI fragment, so this is the
    // conformant way to reference the path `/pets/{id}`.
    let openapi = spec();

    let encoded = openapi
        .resolve_ref::<PathItem>("#/paths/~1pets~1%7Bid%7D")
        .expect("percent-encoded pointer resolves");
    let literal = openapi
        .resolve_ref::<PathItem>("#/paths/~1pets~1{id}")
        .expect("literal pointer resolves too");

    assert!(std::ptr::eq(encoded, literal));
}
