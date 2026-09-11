use super::resolver::{Resolvable, Resolver};
use super::{ResolvedCallback, ResolvedParameter, ResolvedRequestBody, ResolvedResponse};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{
    ExternalDocumentation, Operation, Responses, SecurityRequirement, Server, StatusCode,
};
use std::sync::Arc;

/// [`Operation`] with every `$ref` replaced by the item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOperation {
    /// See [`Operation::tags`].
    pub tags: Vec<String>,
    /// See [`Operation::summary`].
    pub summary: Option<String>,
    /// See [`Operation::description`].
    pub description: Option<String>,
    /// See [`Operation::external_docs`].
    pub external_docs: Option<ExternalDocumentation>,
    /// See [`Operation::operation_id`].
    pub operation_id: Option<String>,
    /// See [`Operation::parameters`].
    pub parameters: Vec<Arc<ResolvedParameter>>,
    /// See [`Operation::request_body`].
    pub request_body: Option<Arc<ResolvedRequestBody>>,
    /// See [`Operation::responses`].
    pub responses: ResolvedResponses,
    /// See [`Operation::callbacks`].
    pub callbacks: IndexMap<String, ResolvedCallback>,
    /// See [`Operation::deprecated`].
    pub deprecated: bool,
    /// See [`Operation::security`].
    pub security: Option<Vec<SecurityRequirement>>,
    /// See [`Operation::servers`].
    pub servers: Vec<Server>,
    /// See [`Operation::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`Responses`] with every `$ref` replaced by the response it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedResponses {
    /// See [`Responses::default`].
    pub default: Option<Arc<ResolvedResponse>>,
    /// See [`Responses::responses`].
    pub responses: IndexMap<StatusCode, Arc<ResolvedResponse>>,
    /// See [`Responses::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

impl Resolvable for Operation {
    type Resolved = ResolvedOperation;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedOperation {
            tags: self.tags.clone(),
            summary: self.summary.clone(),
            description: self.description.clone(),
            external_docs: self.external_docs.clone(),
            operation_id: self.operation_id.clone(),
            parameters: cx.ref_or_vec(&self.parameters)?,
            request_body: self
                .request_body
                .as_ref()
                .map(|body| cx.ref_or(body))
                .transpose()?,
            responses: self.responses.resolve_inline(cx)?,
            callbacks: super::resolve_map(&self.callbacks, cx)?,
            deprecated: self.deprecated,
            security: self.security.clone(),
            servers: self.servers.clone(),
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for Responses {
    type Resolved = ResolvedResponses;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedResponses {
            default: self
                .default
                .as_ref()
                .map(|default| cx.ref_or(default))
                .transpose()?,
            responses: cx.ref_or_map(&self.responses)?,
            extensions: self.extensions.clone(),
        })
    }
}
