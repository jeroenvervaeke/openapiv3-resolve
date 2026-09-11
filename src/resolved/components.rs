use super::resolver::{Resolvable, Resolver};
use super::{
    ResolvedCallback, ResolvedHeader, ResolvedParameter, ResolvedRequestBody, ResolvedResponse,
    ResolvedSchema, Shared,
};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{Components, Example, Link, SecurityScheme};

/// [`Components`] with every entry resolved, including entries that were
/// themselves a `$ref` to another entry.
#[derive(Debug, PartialEq)]
pub struct ResolvedComponents {
    /// See [`Components::schemas`].
    pub schemas: IndexMap<String, Shared<ResolvedSchema>>,
    /// See [`Components::responses`].
    pub responses: IndexMap<String, Shared<ResolvedResponse>>,
    /// See [`Components::parameters`].
    pub parameters: IndexMap<String, Shared<ResolvedParameter>>,
    /// See [`Components::examples`].
    pub examples: IndexMap<String, Shared<Example>>,
    /// See [`Components::request_bodies`].
    pub request_bodies: IndexMap<String, Shared<ResolvedRequestBody>>,
    /// See [`Components::headers`].
    pub headers: IndexMap<String, Shared<ResolvedHeader>>,
    /// See [`Components::security_schemes`].
    pub security_schemes: IndexMap<String, Shared<SecurityScheme>>,
    /// See [`Components::links`].
    pub links: IndexMap<String, Shared<Link>>,
    /// See [`Components::callbacks`].
    pub callbacks: IndexMap<String, Shared<ResolvedCallback>>,
    /// See [`Components::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
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
