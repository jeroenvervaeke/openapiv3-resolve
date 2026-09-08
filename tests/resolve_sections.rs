//! Every resolvable section resolves, both directly and through a `$ref` alias.

mod common;

use common::spec;
use openapiv3::{
    Callback, Example, Header, Link, Parameter, PathItem, RequestBody, Response, Schema,
    SecurityScheme,
};
use openapiv3_resolve::Resolve;

macro_rules! section_resolves {
    ($direct:ident, $via_ref:ident, $ty:ty, $item:literal, $alias:literal) => {
        #[test]
        fn $direct() {
            let openapi = spec();
            assert!(openapi.resolve_ref::<$ty>($item).is_ok(), "{}", $item);
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
    "#/components/schemas/PetAlias"
);
section_resolves!(
    response,
    response_via_ref,
    Response,
    "#/components/responses/PetList",
    "#/components/responses/PetListAlias"
);
section_resolves!(
    parameter,
    parameter_via_ref,
    Parameter,
    "#/components/parameters/Limit",
    "#/components/parameters/LimitAlias"
);
section_resolves!(
    example,
    example_via_ref,
    Example,
    "#/components/examples/One",
    "#/components/examples/OneAlias"
);
section_resolves!(
    request_body,
    request_body_via_ref,
    RequestBody,
    "#/components/requestBodies/CreatePet",
    "#/components/requestBodies/CreatePetAlias"
);
section_resolves!(
    header,
    header_via_ref,
    Header,
    "#/components/headers/XRate",
    "#/components/headers/XRateAlias"
);
section_resolves!(
    security_scheme,
    security_scheme_via_ref,
    SecurityScheme,
    "#/components/securitySchemes/ApiKey",
    "#/components/securitySchemes/ApiKeyAlias"
);
section_resolves!(
    link,
    link_via_ref,
    Link,
    "#/components/links/Self",
    "#/components/links/SelfAlias"
);
section_resolves!(
    callback,
    callback_via_ref,
    Callback,
    "#/components/callbacks/OnEvent",
    "#/components/callbacks/OnEventAlias"
);
section_resolves!(
    path_item,
    path_item_via_ref,
    PathItem,
    "#/paths/~1pets",
    "#/paths/~1alias"
);
