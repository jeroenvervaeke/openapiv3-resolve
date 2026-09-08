#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub use indexmap;
pub use openapiv3;

use indexmap::IndexMap;
use openapiv3::{
    Callback, Components, Example, Header, Link, OpenAPI, Parameter, ReferenceOr, RequestBody,
    Response, Schema, SecurityScheme,
};

/// Resolves a full `$ref` pointer against a document root.
///
/// Implemented on [`OpenAPI`] for every component type, so the pointer is
/// expected to look like `#/components/schemas/Pet`.
pub trait Resolve<T> {
    /// Resolves `path` to a `T`, following chains of `$ref`s.
    ///
    /// Returns `None` if the pointer is malformed, names a location this
    /// crate cannot resolve, or points at an item the document does not
    /// contain.
    fn resolve<'a>(&'a self, path: &str) -> Option<&'a T>;
}

/// Resolves a pointer that is relative to `self` rather than to the document root.
///
/// Implemented on [`Components`] and on `IndexMap<String, ReferenceOr<T>>`;
/// `openapi` is threaded through so a nested `$ref` can be followed back up to
/// the document root.
pub trait ResolveWithOpenAPIAndPath<T> {
    /// Resolves `path` relative to `self`, using `openapi` for nested `$ref`s.
    ///
    /// Returns `None` under the same conditions as [`Resolve::resolve`].
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI, path: &str) -> Option<&'a T>;
}

/// Resolves a `ReferenceOr<T>` (or its boxed and optional variants) to the item it denotes.
pub trait ResolveWithOpenAPI<T> {
    /// Returns the inline item, or the item the `$ref` points at.
    ///
    /// Returns `None` if the value is absent, or if the reference cannot be
    /// resolved (see [`Resolve::resolve`]).
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Option<&'a T>;
}

impl<T> ResolveWithOpenAPI<T> for ReferenceOr<T>
where
    OpenAPI: Resolve<T>,
{
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Option<&'a T> {
        match self {
            ReferenceOr::Reference { reference } => openapi.resolve(reference),
            ReferenceOr::Item(item) => Some(item),
        }
    }
}

impl<T> ResolveWithOpenAPI<T> for ReferenceOr<Box<T>>
where
    OpenAPI: Resolve<T>,
{
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Option<&'a T> {
        match self {
            ReferenceOr::Reference { reference } => openapi.resolve(reference),
            ReferenceOr::Item(item) => Some(item),
        }
    }
}

impl<T> ResolveWithOpenAPI<T> for Option<ReferenceOr<T>>
where
    OpenAPI: Resolve<T>,
{
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Option<&'a T> {
        self.as_ref()?.resolve(openapi)
    }
}

impl<T> ResolveWithOpenAPI<T> for Option<ReferenceOr<Box<T>>>
where
    OpenAPI: Resolve<T>,
{
    fn resolve<'a>(&'a self, openapi: &'a OpenAPI) -> Option<&'a T> {
        self.as_ref()?.resolve(openapi)
    }
}

// Macros
macro_rules! resolve_with_openapi {
    ($ty:ty, $property_name:ident, $resolve_type:ty) => {
        impl ResolveWithOpenAPIAndPath<$resolve_type> for $ty {
            fn resolve<'a>(
                &'a self,
                openapi: &'a OpenAPI,
                path: &str,
            ) -> Option<&'a $resolve_type> {
                let (root_path, sub_path) = path.split_once('/')?;

                match root_path {
                    stringify!($property_name) => self.$property_name.resolve(openapi, sub_path),
                    _ => None,
                }
            }
        }
    };
}

macro_rules! resolve_root_optional {
    ($property_name:ident, $type:ty) => {
        impl Resolve<$type> for OpenAPI {
            fn resolve<'a>(&'a self, path: &str) -> Option<&'a $type> {
                let path = path.strip_prefix("#/")?;
                let (root_path, sub_path) = path.split_once('/')?;

                match root_path {
                    stringify!($property_name) => {
                        self.$property_name.as_ref()?.resolve(self, sub_path)
                    }
                    _ => None,
                }
            }
        }
    };
}

macro_rules! resolve_with_openapi_index_map {
    ($ty:ty) => {
        impl ResolveWithOpenAPIAndPath<$ty> for IndexMap<String, ReferenceOr<$ty>> {
            fn resolve<'a>(&'a self, openapi: &'a OpenAPI, path: &str) -> Option<&'a $ty> {
                match self.get(path)? {
                    ReferenceOr::Reference { reference } => openapi.resolve(reference),
                    ReferenceOr::Item(item) => Some(item),
                }
            }
        }
    };
}

// Implement resolve for OpenAPI
resolve_root_optional!(components, Callback);
resolve_root_optional!(components, Example);
resolve_root_optional!(components, Header);
resolve_root_optional!(components, Link);
resolve_root_optional!(components, Parameter);
resolve_root_optional!(components, RequestBody);
resolve_root_optional!(components, Response);
resolve_root_optional!(components, Schema);
resolve_root_optional!(components, SecurityScheme);

// Implement resolve for Components
resolve_with_openapi!(Components, callbacks, Callback);
resolve_with_openapi!(Components, examples, Example);
resolve_with_openapi!(Components, headers, Header);
resolve_with_openapi!(Components, links, Link);
resolve_with_openapi!(Components, parameters, Parameter);
resolve_with_openapi!(Components, request_bodies, RequestBody);
resolve_with_openapi!(Components, responses, Response);
resolve_with_openapi!(Components, schemas, Schema);
resolve_with_openapi!(Components, security_schemes, SecurityScheme);

// Implement resolve for IndexMap
resolve_with_openapi_index_map!(Callback);
resolve_with_openapi_index_map!(Example);
resolve_with_openapi_index_map!(Header);
resolve_with_openapi_index_map!(Link);
resolve_with_openapi_index_map!(Parameter);
resolve_with_openapi_index_map!(RequestBody);
resolve_with_openapi_index_map!(Response);
resolve_with_openapi_index_map!(Schema);
resolve_with_openapi_index_map!(SecurityScheme);

#[cfg(test)]
mod tests {
    use super::*;
    use openapiv3::{
        MediaType, Operation, PathItem, Paths, Responses, SchemaData, SchemaKind, StatusCode,
        StringType, Type,
    };

    #[test]
    fn resolve_index_map_schema() {
        let openapi = OpenAPI {
            paths: Paths {
                paths: IndexMap::from([(
                    "/".to_string(),
                    ReferenceOr::Item(PathItem {
                        get: Some(Operation {
                            responses: Responses {
                                responses: IndexMap::from([(
                                    StatusCode::Code(200),
                                    ReferenceOr::Reference {
                                        reference: "#/components/responses/response_1".to_string(),
                                    },
                                )]),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                )]),
                extensions: IndexMap::new(),
            },
            components: Some(Components {
                responses: IndexMap::from([
                    (
                        "response_1".to_string(),
                        ReferenceOr::Reference {
                            reference: "#/components/responses/response_2".to_string(),
                        },
                    ),
                    (
                        "response_2".to_string(),
                        ReferenceOr::Item(Response {
                            content: IndexMap::from([(
                                "application/json".to_string(),
                                MediaType {
                                    schema: Some(ReferenceOr::Reference {
                                        reference: "#/components/schemas/schema_1".to_string(),
                                    }),
                                    ..Default::default()
                                },
                            )]),
                            ..Default::default()
                        }),
                    ),
                ]),
                schemas: IndexMap::from([
                    (
                        "schema_1".to_string(),
                        ReferenceOr::Reference {
                            reference: "#/components/schemas/schema_2".to_string(),
                        },
                    ),
                    (
                        "schema_2".to_string(),
                        ReferenceOr::Item(Schema {
                            schema_data: SchemaData {
                                title: Some("schema_2".to_string()),
                                ..Default::default()
                            },
                            schema_kind: SchemaKind::Type(Type::String(StringType {
                                ..Default::default()
                            })),
                        }),
                    ),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let path: &PathItem = openapi
            .paths
            .paths
            .get("/")
            .expect("able to get path")
            .as_item()
            .expect("able to resolve path");

        let get = path.get.as_ref().expect("expect to be able to get 'get'");
        let reponse_200 = get
            .responses
            .responses
            .get(&StatusCode::Code(200))
            .expect("able to get response 200");

        let content_json = reponse_200
            .resolve(&openapi)
            .expect("able to resolve response 200")
            .content
            .get("application/json")
            .expect("able to get content");

        let schema = content_json
            .schema
            .resolve(&openapi)
            .expect("able to resolve schema");

        assert_eq!(schema.schema_data.title.as_ref().unwrap(), "schema_2");
    }
}
