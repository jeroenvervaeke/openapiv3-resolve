use super::resolver::{Resolvable, Resolver};
use super::{ResolvedHeader, ResolvedSchema};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{Encoding, Example, Link, MediaType, QueryStyle, RequestBody, Response};
use std::sync::Arc;

/// [`RequestBody`] with every `$ref` replaced by the item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRequestBody {
    /// See [`RequestBody::description`].
    pub description: Option<String>,
    /// See [`RequestBody::content`].
    pub content: IndexMap<String, ResolvedMediaType>,
    /// See [`RequestBody::required`].
    pub required: bool,
    /// See [`RequestBody::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`Response`] with every `$ref` replaced by the item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedResponse {
    /// See [`Response::description`].
    pub description: String,
    /// See [`Response::headers`].
    pub headers: IndexMap<String, Arc<ResolvedHeader>>,
    /// See [`Response::content`].
    pub content: IndexMap<String, ResolvedMediaType>,
    /// See [`Response::links`].
    pub links: IndexMap<String, Arc<Link>>,
    /// See [`Response::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`MediaType`] with every `$ref` replaced by the item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMediaType {
    /// See [`MediaType::schema`].
    pub schema: Option<Arc<ResolvedSchema>>,
    /// See [`MediaType::example`].
    pub example: Option<serde_json::Value>,
    /// See [`MediaType::examples`].
    pub examples: IndexMap<String, Arc<Example>>,
    /// See [`MediaType::encoding`].
    pub encoding: IndexMap<String, ResolvedEncoding>,
    /// See [`MediaType::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

/// [`Encoding`] with every `$ref` replaced by the item it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedEncoding {
    /// See [`Encoding::content_type`].
    pub content_type: Option<String>,
    /// See [`Encoding::headers`].
    pub headers: IndexMap<String, Arc<ResolvedHeader>>,
    /// See [`Encoding::style`].
    pub style: Option<QueryStyle>,
    /// See [`Encoding::explode`].
    pub explode: bool,
    /// See [`Encoding::allow_reserved`].
    pub allow_reserved: bool,
    /// See [`Encoding::extensions`].
    pub extensions: IndexMap<String, serde_json::Value>,
}

impl Resolvable for RequestBody {
    type Resolved = ResolvedRequestBody;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedRequestBody {
            description: self.description.clone(),
            content: super::resolve_map(&self.content, cx)?,
            required: self.required,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for Response {
    type Resolved = ResolvedResponse;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedResponse {
            description: self.description.clone(),
            headers: cx.ref_or_map(&self.headers)?,
            content: super::resolve_map(&self.content, cx)?,
            links: cx.ref_or_map(&self.links)?,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for MediaType {
    type Resolved = ResolvedMediaType;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedMediaType {
            schema: self
                .schema
                .as_ref()
                .map(|schema| cx.ref_or(schema))
                .transpose()?,
            example: self.example.clone(),
            examples: cx.ref_or_map(&self.examples)?,
            encoding: super::resolve_map(&self.encoding, cx)?,
            extensions: self.extensions.clone(),
        })
    }
}

impl Resolvable for Encoding {
    type Resolved = ResolvedEncoding;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedEncoding {
            content_type: self.content_type.clone(),
            headers: cx.ref_or_map(&self.headers)?,
            style: self.style.clone(),
            explode: self.explode,
            allow_reserved: self.allow_reserved,
            extensions: self.extensions.clone(),
        })
    }
}
