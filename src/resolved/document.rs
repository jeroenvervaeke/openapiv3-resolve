use super::resolver::{Resolvable, Resolver};
use super::{ResolvedComponents, ResolvedOperation, ResolvedParameter, Shared};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{
    Callback, ExternalDocumentation, Info, OpenAPI, Operation, PathItem, Paths,
    SecurityRequirement, Server, Tag,
};

/// [`OpenAPI`] with every `$ref` in the document replaced by the item it named.
///
/// Built with [`TryFrom<&OpenAPI>`](#impl-TryFrom<%26OpenAPI>-for-ResolvedOpenAPI).
/// Every reference to the same component resolves to the same [`Shared`]
/// allocation, so [`Shared::as_ptr`] tells whether two sites named the same
/// component, and the entries of [`components`](Self::components) are those
/// very allocations. The document is only ever borrowed from, never taken
/// apart; see [`Shared`] for why.
#[derive(Debug, PartialEq)]
pub struct ResolvedOpenAPI {
    openapi: String,
    info: Info,
    servers: Vec<Server>,
    paths: ResolvedPaths,
    components: Option<ResolvedComponents>,
    security: Option<Vec<SecurityRequirement>>,
    tags: Vec<Tag>,
    external_docs: Option<ExternalDocumentation>,
    extensions: IndexMap<String, serde_json::Value>,
}

// The fields are private so that no part of the document can be moved out
// of it: every recursive schema edge relies on its target staying in
// `components` for as long as anything borrowed from the document exists.
impl ResolvedOpenAPI {
    /// See [`OpenAPI::openapi`].
    pub fn openapi(&self) -> &str {
        &self.openapi
    }

    /// See [`OpenAPI::info`].
    pub fn info(&self) -> &Info {
        &self.info
    }

    /// See [`OpenAPI::servers`].
    pub fn servers(&self) -> &[Server] {
        &self.servers
    }

    /// See [`OpenAPI::paths`].
    pub fn paths(&self) -> &ResolvedPaths {
        &self.paths
    }

    /// See [`OpenAPI::components`].
    pub fn components(&self) -> Option<&ResolvedComponents> {
        self.components.as_ref()
    }

    /// See [`OpenAPI::security`].
    pub fn security(&self) -> Option<&[SecurityRequirement]> {
        self.security.as_deref()
    }

    /// See [`OpenAPI::tags`].
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// See [`OpenAPI::external_docs`].
    pub fn external_docs(&self) -> Option<&ExternalDocumentation> {
        self.external_docs.as_ref()
    }

    /// See [`OpenAPI::extensions`].
    pub fn extensions(&self) -> &IndexMap<String, serde_json::Value> {
        &self.extensions
    }
}

/// [`Paths`] with every `$ref` replaced by the path item it named.
#[derive(Debug, PartialEq)]
pub struct ResolvedPaths {
    /// See [`Paths::paths`].
    pub paths: IndexMap<String, Shared<ResolvedPathItem>>,
    /// See [`Paths::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`PathItem`] with every `$ref` replaced by the item it named.
#[derive(Debug, PartialEq)]
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
    pub parameters: Vec<Shared<ResolvedParameter>>,
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
