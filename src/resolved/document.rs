use super::resolver::{Resolvable, Resolver};
use super::{
    ResolvedHeader, ResolvedOperation, ResolvedParameter, ResolvedRequestBody, ResolvedResponse,
    ResolvedSchema,
};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{
    Callback, Components, Example, ExternalDocumentation, Info, Link, OpenAPI, Operation, PathItem,
    Paths, SecurityRequirement, SecurityScheme, Server, Tag,
};
use std::sync::Arc;

/// [`OpenAPI`] with every `$ref` in the document replaced by the item it named.
///
/// Built with [`TryFrom<&OpenAPI>`](#impl-TryFrom<%26OpenAPI>-for-ResolvedOpenAPI).
/// Every reference to the same component resolves to the same `Arc`, so
/// [`Arc::ptr_eq`] tells whether two sites named the same component, and the
/// entries of [`components`](Self::components) are those very `Arc`s.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOpenAPI {
    /// See [`OpenAPI::openapi`].
    pub openapi: String,
    /// See [`OpenAPI::info`].
    pub info: Info,
    /// See [`OpenAPI::servers`].
    pub servers: Vec<Server>,
    /// See [`OpenAPI::paths`].
    pub paths: ResolvedPaths,
    /// See [`OpenAPI::components`].
    pub components: Option<ResolvedComponents>,
    /// See [`OpenAPI::security`].
    pub security: Option<Vec<SecurityRequirement>>,
    /// See [`OpenAPI::tags`].
    pub tags: Vec<Tag>,
    /// See [`OpenAPI::external_docs`].
    pub external_docs: Option<ExternalDocumentation>,
    /// See [`OpenAPI::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`Components`] with every entry resolved, including entries that were
/// themselves a `$ref` to another entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedComponents {
    /// See [`Components::schemas`].
    pub schemas: IndexMap<String, Arc<ResolvedSchema>>,
    /// See [`Components::responses`].
    pub responses: IndexMap<String, Arc<ResolvedResponse>>,
    /// See [`Components::parameters`].
    pub parameters: IndexMap<String, Arc<ResolvedParameter>>,
    /// See [`Components::examples`].
    pub examples: IndexMap<String, Arc<Example>>,
    /// See [`Components::request_bodies`].
    pub request_bodies: IndexMap<String, Arc<ResolvedRequestBody>>,
    /// See [`Components::headers`].
    pub headers: IndexMap<String, Arc<ResolvedHeader>>,
    /// See [`Components::security_schemes`].
    pub security_schemes: IndexMap<String, Arc<SecurityScheme>>,
    /// See [`Components::links`].
    pub links: IndexMap<String, Arc<Link>>,
    /// See [`Components::callbacks`].
    pub callbacks: IndexMap<String, Arc<ResolvedCallback>>,
    /// See [`Components::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`Paths`] with every `$ref` replaced by the path item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPaths {
    /// See [`Paths::paths`].
    pub paths: IndexMap<String, Arc<ResolvedPathItem>>,
    /// See [`Paths::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`PathItem`] with every `$ref` replaced by the item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPathItem {
    /// See [`PathItem::summary`].
    pub summary: Option<String>,
    /// See [`PathItem::description`].
    pub description: Option<String>,
    /// See [`PathItem::get`].
    pub get: Option<ResolvedOperation>,
    /// See [`PathItem::put`].
    pub put: Option<ResolvedOperation>,
    /// See [`PathItem::post`].
    pub post: Option<ResolvedOperation>,
    /// See [`PathItem::delete`].
    pub delete: Option<ResolvedOperation>,
    /// See [`PathItem::options`].
    pub options: Option<ResolvedOperation>,
    /// See [`PathItem::head`].
    pub head: Option<ResolvedOperation>,
    /// See [`PathItem::patch`].
    pub patch: Option<ResolvedOperation>,
    /// See [`PathItem::trace`].
    pub trace: Option<ResolvedOperation>,
    /// See [`PathItem::servers`].
    pub servers: Vec<Server>,
    /// See [`PathItem::parameters`].
    pub parameters: Vec<Arc<ResolvedParameter>>,
    /// See [`PathItem::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

impl ResolvedPathItem {
    /// The operations this path item defines, keyed by lower-case HTTP method.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &ResolvedOperation)> {
        [
            ("get", &self.get),
            ("put", &self.put),
            ("post", &self.post),
            ("delete", &self.delete),
            ("options", &self.options),
            ("head", &self.head),
            ("patch", &self.patch),
            ("trace", &self.trace),
        ]
        .into_iter()
        .filter_map(|(method, operation)| operation.as_ref().map(|operation| (method, operation)))
    }
}

/// [`Callback`] with every `$ref` replaced by the item it named.
pub type ResolvedCallback = IndexMap<String, ResolvedPathItem>;

impl Resolvable for OpenAPI {
    type Resolved = ResolvedOpenAPI;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedOpenAPI {
            openapi: self.openapi.clone(),
            info: self.info.clone(),
            servers: self.servers.clone(),
            // Components first, so the paths find every component already
            // resolved; the result is the same either way.
            components: self
                .components
                .as_ref()
                .map(|components| components.resolve_inline(cx))
                .transpose()?,
            paths: self.paths.resolve_inline(cx)?,
            security: self.security.clone(),
            tags: self.tags.clone(),
            external_docs: self.external_docs.clone(),
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for Components {
    type Resolved = ResolvedComponents;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedComponents {
            schemas: cx.section(&self.schemas)?,
            responses: cx.section(&self.responses)?,
            parameters: cx.section(&self.parameters)?,
            examples: cx.section(&self.examples)?,
            request_bodies: cx.section(&self.request_bodies)?,
            headers: cx.section(&self.headers)?,
            security_schemes: cx.section(&self.security_schemes)?,
            links: cx.section(&self.links)?,
            callbacks: cx.section(&self.callbacks)?,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for Paths {
    type Resolved = ResolvedPaths;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedPaths {
            paths: cx.section(&self.paths)?,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for PathItem {
    type Resolved = ResolvedPathItem;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        let mut operation = |operation: &Option<Operation>| {
            operation
                .as_ref()
                .map(|operation| operation.resolve_inline(cx))
                .transpose()
        };
        let get = operation(&self.get)?;
        let put = operation(&self.put)?;
        let post = operation(&self.post)?;
        let delete = operation(&self.delete)?;
        let options = operation(&self.options)?;
        let head = operation(&self.head)?;
        let patch = operation(&self.patch)?;
        let trace = operation(&self.trace)?;
        Ok(ResolvedPathItem {
            summary: self.summary.clone(),
            description: self.description.clone(),
            get,
            put,
            post,
            delete,
            options,
            head,
            patch,
            trace,
            servers: self.servers.clone(),
            parameters: cx.ref_or_vec(&self.parameters)?,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for Callback {
    type Resolved = ResolvedCallback;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        super::resolve_map(self, cx)
    }
}
