//! Two-phase resolver that turns an [`OpenAPI`] document into a
//! [`ResolvedOpenAPI`].
//!
//! ## Algorithm
//!
//! Resolution proceeds in two phases:
//!
//! 1. **Allocate.** For every named component that is an inline definition
//!    (`Item`), allocate an empty [`Resolved`] handle and place it in the
//!    registry. For component entries that are themselves `$ref`s (aliases),
//!    follow the chain to its terminating inline definition and store an
//!    additional handle pointing at the same allocation.
//! 2. **Populate.** For every inline component definition, resolve its
//!    contents. `$ref`s encountered along the way are looked up in the
//!    registry from phase 1 and stored as [`ResolvedWeak`] back-edges.
//!
//! Because every named component already has an allocation by the time we
//! recurse into bodies, cycles between components are stored as
//! [`ResolvedWeak`] handles back to the registry — the strong references in
//! [`ResolvedComponents`] keep everything alive while preventing leaks.
//!
//! [`Resolved`]: crate::handle::Resolved
//! [`ResolvedWeak`]: crate::handle::ResolvedWeak
//! [`ResolvedComponents`]: crate::ResolvedComponents

use std::collections::HashSet;

use indexmap::IndexMap;
use openapiv3::*;
use thiserror::Error;

use crate::handle::{Resolved, ResolvedRefOr};
use crate::resolved::*;

/// Failure modes for the resolver.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// A `$ref` did not begin with `#/components/...`. External refs are not
    /// supported.
    #[error(
        "unsupported $ref `{reference}`: only internal refs starting with `#/components/` are supported"
    )]
    UnsupportedReference {
        /// The reference string that failed to parse.
        reference: String,
    },

    /// A `$ref` pointed at a component kind that does not match the expected
    /// type at this position (e.g. a Schema position pointed at
    /// `#/components/responses/...`).
    #[error("$ref `{reference}` points at `{found}` but the position requires `{expected}`")]
    WrongReferenceKind {
        /// The reference string.
        reference: String,
        /// The component kind expected at this position
        /// (e.g. `"schemas"`).
        expected: &'static str,
        /// The component kind found in the reference (e.g. `"responses"`).
        found: String,
    },

    /// A `$ref` pointed at a name that does not exist in the corresponding
    /// components map.
    #[error("$ref `{reference}` does not resolve to any component")]
    UnresolvedReference {
        /// The reference string.
        reference: String,
    },

    /// A chain of `$ref`s in the components map (e.g.
    /// `Foo -> Bar -> Foo`) never reaches an inline definition.
    #[error(
        "$ref chain in components/{kind} forms a cycle without an inline definition: {}",
        chain.join(" -> ")
    )]
    ReferenceCycle {
        /// The names visited while following the chain, in order.
        chain: Vec<String>,
        /// The component kind being chased (e.g. `"schemas"`).
        kind: &'static str,
    },

    /// A `$ref` to a [`PathItem`](openapiv3::PathItem) was encountered. Path
    /// items are not stored in `components` in OpenAPI 3.0, so refs to them
    /// cannot be resolved internally.
    #[error(
        "$ref `{reference}` to a PathItem cannot be resolved: path items are not stored in components"
    )]
    PathItemReference {
        /// The reference string.
        reference: String,
    },
}

/// For each named entry in `src`: if it is an inline item, allocate an empty
/// handle in `dst`; if it is a `$ref` chain, follow the chain and store an
/// additional handle pointing at the same allocation as the target.
macro_rules! allocate_kind {
    ($src:expr, $kind:expr, $dst:expr) => {{
        let src = $src;
        let dst = $dst;
        for (name, entry) in src.iter() {
            if matches!(entry, ReferenceOr::Item(_)) {
                dst.insert(name.clone(), Resolved::empty());
            }
        }
        let mut result: Result<(), ResolveError> = Ok(());
        for (name, entry) in src.iter() {
            if let ReferenceOr::Reference { reference } = entry {
                match follow_ref_chain(src, name, reference, $kind) {
                    Ok(target_name) => {
                        let target_handle = dst
                            .get(&target_name)
                            .expect("alias target was an inline item")
                            .clone();
                        dst.insert(name.clone(), target_handle);
                    }
                    Err(e) => {
                        result = Err(e);
                        break;
                    }
                }
            }
        }
        result
    }};
}

/// Resolve every `$ref` in `openapi`, returning a [`ResolvedOpenAPI`].
///
/// Returns an error if a `$ref` is unsupported (external, malformed, points
/// at the wrong component kind), if a referenced component is missing, or if
/// an alias chain in the components map forms a cycle without ever reaching
/// an inline definition.
pub fn resolve(openapi: &OpenAPI) -> Result<ResolvedOpenAPI, ResolveError> {
    Resolver::new(openapi).run()
}

/// Convenience extension trait. Re-exports [`resolve`] as
/// `OpenAPI::resolve_all()` so callers can chain it after deserialization
/// without a free-function import.
///
/// We deliberately use a different method name from [`Resolve::resolve`] /
/// [`ResolveWithOpenAPI::resolve`] because those traits are also implemented
/// on [`OpenAPI`] (or on values reachable from it) and the methods would
/// otherwise be ambiguous at the call site.
///
/// [`Resolve::resolve`]: crate::Resolve::resolve
/// [`ResolveWithOpenAPI::resolve`]: crate::ResolveWithOpenAPI::resolve
pub trait OpenAPIExt {
    /// Resolve every `$ref` in `self`, returning a [`ResolvedOpenAPI`].
    fn resolve_all(&self) -> Result<ResolvedOpenAPI, ResolveError>;
}

impl OpenAPIExt for OpenAPI {
    fn resolve_all(&self) -> Result<ResolvedOpenAPI, ResolveError> {
        resolve(self)
    }
}

// --------------------------------------------------------------------------
// Registry
// --------------------------------------------------------------------------

#[derive(Default)]
struct Registry {
    schemas: IndexMap<String, Resolved<ResolvedSchema>>,
    responses: IndexMap<String, Resolved<ResolvedResponse>>,
    parameters: IndexMap<String, Resolved<ResolvedParameter>>,
    examples: IndexMap<String, Resolved<Example>>,
    request_bodies: IndexMap<String, Resolved<ResolvedRequestBody>>,
    headers: IndexMap<String, Resolved<ResolvedHeader>>,
    security_schemes: IndexMap<String, Resolved<SecurityScheme>>,
    links: IndexMap<String, Resolved<Link>>,
    callbacks: IndexMap<String, Resolved<ResolvedCallback>>,
}

struct Resolver<'a> {
    openapi: &'a OpenAPI,
    registry: Registry,
}

impl<'a> Resolver<'a> {
    fn new(openapi: &'a OpenAPI) -> Self {
        Self {
            openapi,
            registry: Registry::default(),
        }
    }

    fn run(mut self) -> Result<ResolvedOpenAPI, ResolveError> {
        if let Some(components) = self.openapi.components.as_ref() {
            self.allocate_component_slots(components)?;
            self.populate_components(components)?;
        }
        self.build_top_level()
    }

    // ----------------------------------------------------------------------
    // Phase 1: allocate empty handles and resolve component-level aliases
    // ----------------------------------------------------------------------

    fn allocate_component_slots(&mut self, c: &Components) -> Result<(), ResolveError> {
        allocate_kind!(&c.schemas, "schemas", &mut self.registry.schemas)?;
        allocate_kind!(&c.responses, "responses", &mut self.registry.responses)?;
        allocate_kind!(&c.parameters, "parameters", &mut self.registry.parameters)?;
        allocate_kind!(&c.examples, "examples", &mut self.registry.examples)?;
        allocate_kind!(
            &c.request_bodies,
            "requestBodies",
            &mut self.registry.request_bodies
        )?;
        allocate_kind!(&c.headers, "headers", &mut self.registry.headers)?;
        allocate_kind!(
            &c.security_schemes,
            "securitySchemes",
            &mut self.registry.security_schemes
        )?;
        allocate_kind!(&c.links, "links", &mut self.registry.links)?;
        allocate_kind!(&c.callbacks, "callbacks", &mut self.registry.callbacks)?;
        Ok(())
    }

    // ----------------------------------------------------------------------
    // Phase 2: resolve component bodies
    // ----------------------------------------------------------------------

    fn populate_components(&self, c: &Components) -> Result<(), ResolveError> {
        for (name, entry) in &c.schemas {
            if let ReferenceOr::Item(s) = entry {
                let resolved = self.resolve_schema(s)?;
                set_handle(&self.registry.schemas, name, resolved);
            }
        }
        for (name, entry) in &c.responses {
            if let ReferenceOr::Item(r) = entry {
                let resolved = self.resolve_response(r)?;
                set_handle(&self.registry.responses, name, resolved);
            }
        }
        for (name, entry) in &c.parameters {
            if let ReferenceOr::Item(p) = entry {
                let resolved = self.resolve_parameter(p)?;
                set_handle(&self.registry.parameters, name, resolved);
            }
        }
        for (name, entry) in &c.examples {
            if let ReferenceOr::Item(e) = entry {
                set_handle(&self.registry.examples, name, e.clone());
            }
        }
        for (name, entry) in &c.request_bodies {
            if let ReferenceOr::Item(rb) = entry {
                let resolved = self.resolve_request_body(rb)?;
                set_handle(&self.registry.request_bodies, name, resolved);
            }
        }
        for (name, entry) in &c.headers {
            if let ReferenceOr::Item(h) = entry {
                let resolved = self.resolve_header(h)?;
                set_handle(&self.registry.headers, name, resolved);
            }
        }
        for (name, entry) in &c.security_schemes {
            if let ReferenceOr::Item(s) = entry {
                set_handle(&self.registry.security_schemes, name, s.clone());
            }
        }
        for (name, entry) in &c.links {
            if let ReferenceOr::Item(l) = entry {
                set_handle(&self.registry.links, name, l.clone());
            }
        }
        for (name, entry) in &c.callbacks {
            if let ReferenceOr::Item(cb) = entry {
                let resolved = self.resolve_callback(cb)?;
                set_handle(&self.registry.callbacks, name, resolved);
            }
        }
        Ok(())
    }

    // ----------------------------------------------------------------------
    // Top-level assembly
    // ----------------------------------------------------------------------

    fn build_top_level(self) -> Result<ResolvedOpenAPI, ResolveError> {
        let paths = self.resolve_paths(&self.openapi.paths)?;
        let components = self.build_resolved_components();
        Ok(ResolvedOpenAPI {
            openapi: self.openapi.openapi.clone(),
            info: self.openapi.info.clone(),
            servers: self.openapi.servers.clone(),
            paths,
            components,
            security: self.openapi.security.clone(),
            tags: self.openapi.tags.clone(),
            external_docs: self.openapi.external_docs.clone(),
            extensions: self.openapi.extensions.clone(),
        })
    }

    fn build_resolved_components(&self) -> ResolvedComponents {
        ResolvedComponents {
            schemas: self.registry.schemas.clone(),
            responses: self.registry.responses.clone(),
            parameters: self.registry.parameters.clone(),
            examples: self.registry.examples.clone(),
            request_bodies: self.registry.request_bodies.clone(),
            headers: self.registry.headers.clone(),
            security_schemes: self.registry.security_schemes.clone(),
            links: self.registry.links.clone(),
            callbacks: self.registry.callbacks.clone(),
            extensions: self
                .openapi
                .components
                .as_ref()
                .map(|c| c.extensions.clone())
                .unwrap_or_default(),
        }
    }

    // ----------------------------------------------------------------------
    // Per-type resolvers
    // ----------------------------------------------------------------------

    fn resolve_paths(&self, paths: &Paths) -> Result<ResolvedPaths, ResolveError> {
        let mut out: IndexMap<String, Resolved<ResolvedPathItem>> = IndexMap::new();
        for (path, entry) in &paths.paths {
            match entry {
                ReferenceOr::Item(item) => {
                    let resolved = self.resolve_path_item(item)?;
                    out.insert(path.clone(), Resolved::new(resolved));
                }
                ReferenceOr::Reference { reference } => {
                    return Err(ResolveError::PathItemReference {
                        reference: reference.clone(),
                    });
                }
            }
        }
        Ok(ResolvedPaths {
            paths: out,
            extensions: paths.extensions.clone(),
        })
    }

    fn resolve_path_item(&self, p: &PathItem) -> Result<ResolvedPathItem, ResolveError> {
        Ok(ResolvedPathItem {
            summary: p.summary.clone(),
            description: p.description.clone(),
            get: p
                .get
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            put: p
                .put
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            post: p
                .post
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            delete: p
                .delete
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            options: p
                .options
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            head: p
                .head
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            patch: p
                .patch
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            trace: p
                .trace
                .as_ref()
                .map(|o| self.resolve_operation(o))
                .transpose()?,
            servers: p.servers.clone(),
            parameters: self.resolve_parameter_list(&p.parameters)?,
            extensions: p.extensions.clone(),
        })
    }

    fn resolve_operation(&self, op: &Operation) -> Result<ResolvedOperation, ResolveError> {
        let request_body = match &op.request_body {
            None => None,
            Some(r) => Some(self.resolve_request_body_ref(r)?),
        };

        let mut callbacks: IndexMap<String, ResolvedCallback> = IndexMap::new();
        for (name, cb) in &op.callbacks {
            callbacks.insert(name.clone(), self.resolve_callback(cb)?);
        }

        Ok(ResolvedOperation {
            tags: op.tags.clone(),
            summary: op.summary.clone(),
            description: op.description.clone(),
            external_docs: op.external_docs.clone(),
            operation_id: op.operation_id.clone(),
            parameters: self.resolve_parameter_list(&op.parameters)?,
            request_body,
            responses: self.resolve_responses(&op.responses)?,
            callbacks,
            deprecated: op.deprecated,
            security: op.security.clone(),
            servers: op.servers.clone(),
            extensions: op.extensions.clone(),
        })
    }

    fn resolve_parameter_list(
        &self,
        list: &[ReferenceOr<Parameter>],
    ) -> Result<Vec<ResolvedRefOr<ResolvedParameter>>, ResolveError> {
        list.iter().map(|p| self.resolve_parameter_ref(p)).collect()
    }

    fn resolve_parameter_ref(
        &self,
        p: &ReferenceOr<Parameter>,
    ) -> Result<ResolvedRefOr<ResolvedParameter>, ResolveError> {
        match p {
            ReferenceOr::Item(param) => {
                let resolved = self.resolve_parameter(param)?;
                Ok(ResolvedRefOr::Item(Resolved::new(resolved)))
            }
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "parameters")?;
                let handle = self.registry.parameters.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_parameter(&self, p: &Parameter) -> Result<ResolvedParameter, ResolveError> {
        Ok(match p {
            Parameter::Query {
                parameter_data,
                allow_reserved,
                style,
                allow_empty_value,
            } => ResolvedParameter::Query {
                parameter_data: self.resolve_parameter_data(parameter_data)?,
                allow_reserved: *allow_reserved,
                style: style.clone(),
                allow_empty_value: *allow_empty_value,
            },
            Parameter::Header {
                parameter_data,
                style,
            } => ResolvedParameter::Header {
                parameter_data: self.resolve_parameter_data(parameter_data)?,
                style: style.clone(),
            },
            Parameter::Path {
                parameter_data,
                style,
            } => ResolvedParameter::Path {
                parameter_data: self.resolve_parameter_data(parameter_data)?,
                style: style.clone(),
            },
            Parameter::Cookie {
                parameter_data,
                style,
            } => ResolvedParameter::Cookie {
                parameter_data: self.resolve_parameter_data(parameter_data)?,
                style: style.clone(),
            },
        })
    }

    fn resolve_parameter_data(
        &self,
        d: &ParameterData,
    ) -> Result<ResolvedParameterData, ResolveError> {
        Ok(ResolvedParameterData {
            name: d.name.clone(),
            description: d.description.clone(),
            required: d.required,
            deprecated: d.deprecated,
            format: self.resolve_parameter_format(&d.format)?,
            example: d.example.clone(),
            examples: self.resolve_example_map(&d.examples)?,
            explode: d.explode,
            extensions: d.extensions.clone(),
        })
    }

    fn resolve_parameter_format(
        &self,
        f: &ParameterSchemaOrContent,
    ) -> Result<ResolvedParameterSchemaOrContent, ResolveError> {
        Ok(match f {
            ParameterSchemaOrContent::Schema(s) => {
                ResolvedParameterSchemaOrContent::Schema(self.resolve_schema_ref(s)?)
            }
            ParameterSchemaOrContent::Content(c) => {
                ResolvedParameterSchemaOrContent::Content(self.resolve_media_type_map(c)?)
            }
        })
    }

    fn resolve_request_body_ref(
        &self,
        r: &ReferenceOr<RequestBody>,
    ) -> Result<ResolvedRefOr<ResolvedRequestBody>, ResolveError> {
        match r {
            ReferenceOr::Item(body) => {
                let resolved = self.resolve_request_body(body)?;
                Ok(ResolvedRefOr::Item(Resolved::new(resolved)))
            }
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "requestBodies")?;
                let handle = self.registry.request_bodies.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_request_body(&self, rb: &RequestBody) -> Result<ResolvedRequestBody, ResolveError> {
        Ok(ResolvedRequestBody {
            description: rb.description.clone(),
            content: self.resolve_media_type_map(&rb.content)?,
            required: rb.required,
            extensions: rb.extensions.clone(),
        })
    }

    fn resolve_responses(&self, r: &Responses) -> Result<ResolvedResponses, ResolveError> {
        let default = match &r.default {
            None => None,
            Some(d) => Some(self.resolve_response_ref(d)?),
        };
        let mut responses: IndexMap<StatusCode, ResolvedRefOr<ResolvedResponse>> = IndexMap::new();
        for (code, entry) in &r.responses {
            responses.insert(code.clone(), self.resolve_response_ref(entry)?);
        }
        Ok(ResolvedResponses {
            default,
            responses,
            extensions: r.extensions.clone(),
        })
    }

    fn resolve_response_ref(
        &self,
        r: &ReferenceOr<Response>,
    ) -> Result<ResolvedRefOr<ResolvedResponse>, ResolveError> {
        match r {
            ReferenceOr::Item(resp) => {
                let resolved = self.resolve_response(resp)?;
                Ok(ResolvedRefOr::Item(Resolved::new(resolved)))
            }
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "responses")?;
                let handle = self.registry.responses.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_response(&self, resp: &Response) -> Result<ResolvedResponse, ResolveError> {
        Ok(ResolvedResponse {
            description: resp.description.clone(),
            headers: self.resolve_header_map(&resp.headers)?,
            content: self.resolve_media_type_map(&resp.content)?,
            links: self.resolve_link_map(&resp.links)?,
            extensions: resp.extensions.clone(),
        })
    }

    fn resolve_header_map(
        &self,
        m: &IndexMap<String, ReferenceOr<Header>>,
    ) -> Result<IndexMap<String, ResolvedRefOr<ResolvedHeader>>, ResolveError> {
        let mut out = IndexMap::with_capacity(m.len());
        for (k, v) in m {
            out.insert(k.clone(), self.resolve_header_ref(v)?);
        }
        Ok(out)
    }

    fn resolve_header_ref(
        &self,
        h: &ReferenceOr<Header>,
    ) -> Result<ResolvedRefOr<ResolvedHeader>, ResolveError> {
        match h {
            ReferenceOr::Item(header) => {
                let resolved = self.resolve_header(header)?;
                Ok(ResolvedRefOr::Item(Resolved::new(resolved)))
            }
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "headers")?;
                let handle = self.registry.headers.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_header(&self, h: &Header) -> Result<ResolvedHeader, ResolveError> {
        Ok(ResolvedHeader {
            description: h.description.clone(),
            style: h.style.clone(),
            required: h.required,
            deprecated: h.deprecated,
            format: self.resolve_parameter_format(&h.format)?,
            example: h.example.clone(),
            examples: self.resolve_example_map(&h.examples)?,
            extensions: h.extensions.clone(),
        })
    }

    fn resolve_link_map(
        &self,
        m: &IndexMap<String, ReferenceOr<Link>>,
    ) -> Result<IndexMap<String, ResolvedRefOr<Link>>, ResolveError> {
        let mut out = IndexMap::with_capacity(m.len());
        for (k, v) in m {
            out.insert(k.clone(), self.resolve_link_ref(v)?);
        }
        Ok(out)
    }

    fn resolve_link_ref(&self, l: &ReferenceOr<Link>) -> Result<ResolvedRefOr<Link>, ResolveError> {
        match l {
            ReferenceOr::Item(link) => Ok(ResolvedRefOr::Item(Resolved::new(link.clone()))),
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "links")?;
                let handle = self.registry.links.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_example_map(
        &self,
        m: &IndexMap<String, ReferenceOr<Example>>,
    ) -> Result<IndexMap<String, ResolvedRefOr<Example>>, ResolveError> {
        let mut out = IndexMap::with_capacity(m.len());
        for (k, v) in m {
            out.insert(k.clone(), self.resolve_example_ref(v)?);
        }
        Ok(out)
    }

    fn resolve_example_ref(
        &self,
        e: &ReferenceOr<Example>,
    ) -> Result<ResolvedRefOr<Example>, ResolveError> {
        match e {
            ReferenceOr::Item(ex) => Ok(ResolvedRefOr::Item(Resolved::new(ex.clone()))),
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "examples")?;
                let handle = self.registry.examples.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_media_type_map(
        &self,
        m: &IndexMap<String, MediaType>,
    ) -> Result<IndexMap<String, ResolvedMediaType>, ResolveError> {
        let mut out = IndexMap::with_capacity(m.len());
        for (k, v) in m {
            out.insert(k.clone(), self.resolve_media_type(v)?);
        }
        Ok(out)
    }

    fn resolve_media_type(&self, m: &MediaType) -> Result<ResolvedMediaType, ResolveError> {
        let schema = match &m.schema {
            None => None,
            Some(s) => Some(self.resolve_schema_ref(s)?),
        };
        let mut encoding: IndexMap<String, ResolvedEncoding> = IndexMap::new();
        for (k, v) in &m.encoding {
            encoding.insert(k.clone(), self.resolve_encoding(v)?);
        }
        Ok(ResolvedMediaType {
            schema,
            example: m.example.clone(),
            examples: self.resolve_example_map(&m.examples)?,
            encoding,
            extensions: m.extensions.clone(),
        })
    }

    fn resolve_encoding(&self, e: &Encoding) -> Result<ResolvedEncoding, ResolveError> {
        Ok(ResolvedEncoding {
            content_type: e.content_type.clone(),
            headers: self.resolve_header_map(&e.headers)?,
            style: e.style.clone(),
            explode: e.explode,
            allow_reserved: e.allow_reserved,
            extensions: e.extensions.clone(),
        })
    }

    fn resolve_callback(&self, cb: &Callback) -> Result<ResolvedCallback, ResolveError> {
        let mut out: ResolvedCallback = IndexMap::with_capacity(cb.len());
        for (key, item) in cb {
            out.insert(key.clone(), self.resolve_path_item(item)?);
        }
        Ok(out)
    }

    fn resolve_schema_ref(
        &self,
        s: &ReferenceOr<Schema>,
    ) -> Result<ResolvedRefOr<ResolvedSchema>, ResolveError> {
        match s {
            ReferenceOr::Item(schema) => {
                let resolved = self.resolve_schema(schema)?;
                Ok(ResolvedRefOr::Item(Resolved::new(resolved)))
            }
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "schemas")?;
                let handle = self.registry.schemas.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_schema_box_ref(
        &self,
        s: &ReferenceOr<Box<Schema>>,
    ) -> Result<ResolvedRefOr<ResolvedSchema>, ResolveError> {
        match s {
            ReferenceOr::Item(schema) => {
                let resolved = self.resolve_schema(schema)?;
                Ok(ResolvedRefOr::Item(Resolved::new(resolved)))
            }
            ReferenceOr::Reference { reference } => {
                let name = parse_component_ref(reference, "schemas")?;
                let handle = self.registry.schemas.get(name).ok_or_else(|| {
                    ResolveError::UnresolvedReference {
                        reference: reference.clone(),
                    }
                })?;
                Ok(ResolvedRefOr::Reference(handle.downgrade()))
            }
        }
    }

    fn resolve_schema_props(
        &self,
        m: &IndexMap<String, ReferenceOr<Box<Schema>>>,
    ) -> Result<IndexMap<String, ResolvedRefOr<ResolvedSchema>>, ResolveError> {
        let mut out = IndexMap::with_capacity(m.len());
        for (k, v) in m {
            out.insert(k.clone(), self.resolve_schema_box_ref(v)?);
        }
        Ok(out)
    }

    fn resolve_schema_list(
        &self,
        list: &[ReferenceOr<Schema>],
    ) -> Result<Vec<ResolvedRefOr<ResolvedSchema>>, ResolveError> {
        list.iter().map(|s| self.resolve_schema_ref(s)).collect()
    }

    fn resolve_schema(&self, s: &Schema) -> Result<ResolvedSchema, ResolveError> {
        Ok(ResolvedSchema {
            schema_data: s.schema_data.clone(),
            schema_kind: self.resolve_schema_kind(&s.schema_kind)?,
        })
    }

    fn resolve_schema_kind(&self, k: &SchemaKind) -> Result<ResolvedSchemaKind, ResolveError> {
        Ok(match k {
            SchemaKind::Type(t) => ResolvedSchemaKind::Type(self.resolve_type(t)?),
            SchemaKind::OneOf { one_of } => ResolvedSchemaKind::OneOf {
                one_of: self.resolve_schema_list(one_of)?,
            },
            SchemaKind::AllOf { all_of } => ResolvedSchemaKind::AllOf {
                all_of: self.resolve_schema_list(all_of)?,
            },
            SchemaKind::AnyOf { any_of } => ResolvedSchemaKind::AnyOf {
                any_of: self.resolve_schema_list(any_of)?,
            },
            SchemaKind::Not { not } => ResolvedSchemaKind::Not {
                not: self.resolve_schema_ref(not)?,
            },
            SchemaKind::Any(any) => ResolvedSchemaKind::Any(self.resolve_any_schema(any)?),
        })
    }

    fn resolve_type(&self, t: &Type) -> Result<ResolvedType, ResolveError> {
        Ok(match t {
            Type::String(s) => ResolvedType::String(s.clone()),
            Type::Number(n) => ResolvedType::Number(n.clone()),
            Type::Integer(i) => ResolvedType::Integer(i.clone()),
            Type::Boolean(b) => ResolvedType::Boolean(b.clone()),
            Type::Object(o) => ResolvedType::Object(self.resolve_object_type(o)?),
            Type::Array(a) => ResolvedType::Array(self.resolve_array_type(a)?),
        })
    }

    fn resolve_object_type(&self, o: &ObjectType) -> Result<ResolvedObjectType, ResolveError> {
        Ok(ResolvedObjectType {
            properties: self.resolve_schema_props(&o.properties)?,
            required: o.required.clone(),
            additional_properties: self.resolve_additional_properties(&o.additional_properties)?,
            min_properties: o.min_properties,
            max_properties: o.max_properties,
        })
    }

    fn resolve_array_type(&self, a: &ArrayType) -> Result<ResolvedArrayType, ResolveError> {
        let items = match &a.items {
            None => None,
            Some(s) => Some(self.resolve_schema_box_ref(s)?),
        };
        Ok(ResolvedArrayType {
            items,
            min_items: a.min_items,
            max_items: a.max_items,
            unique_items: a.unique_items,
        })
    }

    fn resolve_additional_properties(
        &self,
        a: &Option<AdditionalProperties>,
    ) -> Result<Option<ResolvedAdditionalProperties>, ResolveError> {
        Ok(match a {
            None => None,
            Some(AdditionalProperties::Any(b)) => Some(ResolvedAdditionalProperties::Any(*b)),
            Some(AdditionalProperties::Schema(b)) => Some(ResolvedAdditionalProperties::Schema(
                self.resolve_schema_ref(b)?,
            )),
        })
    }

    fn resolve_any_schema(&self, a: &AnySchema) -> Result<ResolvedAnySchema, ResolveError> {
        let not = match &a.not {
            None => None,
            Some(b) => Some(self.resolve_schema_ref(b)?),
        };
        let items = match &a.items {
            None => None,
            Some(b) => Some(self.resolve_schema_box_ref(b)?),
        };
        Ok(ResolvedAnySchema {
            typ: a.typ.clone(),
            pattern: a.pattern.clone(),
            multiple_of: a.multiple_of,
            exclusive_minimum: a.exclusive_minimum,
            exclusive_maximum: a.exclusive_maximum,
            minimum: a.minimum,
            maximum: a.maximum,
            properties: self.resolve_schema_props(&a.properties)?,
            required: a.required.clone(),
            additional_properties: self.resolve_additional_properties(&a.additional_properties)?,
            min_properties: a.min_properties,
            max_properties: a.max_properties,
            items,
            min_items: a.min_items,
            max_items: a.max_items,
            unique_items: a.unique_items,
            enumeration: a.enumeration.clone(),
            format: a.format.clone(),
            min_length: a.min_length,
            max_length: a.max_length,
            one_of: self.resolve_schema_list(&a.one_of)?,
            all_of: self.resolve_schema_list(&a.all_of)?,
            any_of: self.resolve_schema_list(&a.any_of)?,
            not,
        })
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn set_handle<T>(map: &IndexMap<String, Resolved<T>>, name: &str, value: T) {
    let handle = map
        .get(name)
        .expect("phase 1 always allocates a handle for every inline component");
    let _ = handle.set(value);
    // If `set` returns Err, the handle was populated already; that can only
    // happen if a component name appears twice in the source map, which the
    // openapiv3 IndexMap forbids by construction. Ignore the error in any
    // case: re-setting with the same content would be a no-op.
}

/// Follows a chain of `$ref` aliases inside `map` starting from `start_name`
/// (whose entry's `$ref` string is `first_ref`). Returns the name of the
/// first inline entry encountered, or an error if the chain leaves `map`,
/// loops, or terminates at a missing entry.
fn follow_ref_chain<T>(
    map: &IndexMap<String, ReferenceOr<T>>,
    start_name: &str,
    first_ref: &str,
    kind: &'static str,
) -> Result<String, ResolveError> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<String> = vec![start_name.to_string()];
    visited.insert(start_name.to_string());

    let mut cur_ref = first_ref.to_string();
    loop {
        let next = parse_component_ref(&cur_ref, kind)?.to_string();
        if !visited.insert(next.clone()) {
            chain.push(next);
            return Err(ResolveError::ReferenceCycle { chain, kind });
        }
        chain.push(next.clone());
        match map.get(&next) {
            None => {
                return Err(ResolveError::UnresolvedReference { reference: cur_ref });
            }
            Some(ReferenceOr::Item(_)) => return Ok(next),
            Some(ReferenceOr::Reference { reference }) => {
                cur_ref = reference.clone();
            }
        }
    }
}

/// Parses a `#/components/<kind>/<name>` ref and returns `<name>`.
fn parse_component_ref<'a>(
    reference: &'a str,
    expected: &'static str,
) -> Result<&'a str, ResolveError> {
    let inner = reference.strip_prefix("#/components/").ok_or_else(|| {
        ResolveError::UnsupportedReference {
            reference: reference.to_string(),
        }
    })?;
    let (kind, name) = inner
        .split_once('/')
        .ok_or_else(|| ResolveError::UnresolvedReference {
            reference: reference.to_string(),
        })?;
    if kind != expected {
        return Err(ResolveError::WrongReferenceKind {
            reference: reference.to_string(),
            expected,
            found: kind.to_string(),
        });
    }
    Ok(name)
}
