use super::resolver::{Resolvable, Resolver};
use super::{ResolvedAdditionalProperties, ResolvedSchema};
use crate::ResolveError;
use indexmap::IndexMap;
use openapiv3::{
    AnySchema, ArrayType, BooleanType, IntegerType, NumberType, ObjectType, StringType, Type,
};
use std::sync::Arc;

/// [`Type`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedType {
    /// See [`Type::String`].
    String(StringType),
    /// See [`Type::Number`].
    Number(NumberType),
    /// See [`Type::Integer`].
    Integer(IntegerType),
    /// See [`Type::Object`].
    Object(ResolvedObjectType),
    /// See [`Type::Array`].
    Array(ResolvedArrayType),
    /// See [`Type::Boolean`].
    Boolean(BooleanType),
}

/// [`ObjectType`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedObjectType {
    /// See [`ObjectType::properties`].
    pub properties: IndexMap<String, Arc<ResolvedSchema>>,
    /// See [`ObjectType::required`].
    pub required: Vec<String>,
    /// See [`ObjectType::additional_properties`].
    pub additional_properties: Option<ResolvedAdditionalProperties>,
    /// See [`ObjectType::min_properties`].
    pub min_properties: Option<usize>,
    /// See [`ObjectType::max_properties`].
    pub max_properties: Option<usize>,
}

/// [`ArrayType`] with a `$ref` in `items` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedArrayType {
    /// See [`ArrayType::items`].
    pub items: Option<Arc<ResolvedSchema>>,
    /// See [`ArrayType::min_items`].
    pub min_items: Option<usize>,
    /// See [`ArrayType::max_items`].
    pub max_items: Option<usize>,
    /// See [`ArrayType::unique_items`].
    pub unique_items: bool,
}

/// [`AnySchema`] with every `$ref` replaced by the schema it named.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAnySchema {
    /// See [`AnySchema::typ`].
    pub typ: Option<String>,
    /// See [`AnySchema::pattern`].
    pub pattern: Option<String>,
    /// See [`AnySchema::multiple_of`].
    pub multiple_of: Option<f64>,
    /// See [`AnySchema::exclusive_minimum`].
    pub exclusive_minimum: Option<bool>,
    /// See [`AnySchema::exclusive_maximum`].
    pub exclusive_maximum: Option<bool>,
    /// See [`AnySchema::minimum`].
    pub minimum: Option<f64>,
    /// See [`AnySchema::maximum`].
    pub maximum: Option<f64>,
    /// See [`AnySchema::properties`].
    pub properties: IndexMap<String, Arc<ResolvedSchema>>,
    /// See [`AnySchema::required`].
    pub required: Vec<String>,
    /// See [`AnySchema::additional_properties`].
    pub additional_properties: Option<ResolvedAdditionalProperties>,
    /// See [`AnySchema::min_properties`].
    pub min_properties: Option<usize>,
    /// See [`AnySchema::max_properties`].
    pub max_properties: Option<usize>,
    /// See [`AnySchema::items`].
    pub items: Option<Arc<ResolvedSchema>>,
    /// See [`AnySchema::min_items`].
    pub min_items: Option<usize>,
    /// See [`AnySchema::max_items`].
    pub max_items: Option<usize>,
    /// See [`AnySchema::unique_items`].
    pub unique_items: Option<bool>,
    /// See [`AnySchema::enumeration`].
    pub enumeration: Vec<serde_json::Value>,
    /// See [`AnySchema::format`].
    pub format: Option<String>,
    /// See [`AnySchema::min_length`].
    pub min_length: Option<usize>,
    /// See [`AnySchema::max_length`].
    pub max_length: Option<usize>,
    /// See [`AnySchema::one_of`].
    pub one_of: Vec<Arc<ResolvedSchema>>,
    /// See [`AnySchema::all_of`].
    pub all_of: Vec<Arc<ResolvedSchema>>,
    /// See [`AnySchema::any_of`].
    pub any_of: Vec<Arc<ResolvedSchema>>,
    /// See [`AnySchema::not`].
    pub not: Option<Arc<ResolvedSchema>>,
}

impl Resolvable for Type {
    type Resolved = ResolvedType;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(match self {
            Self::String(string) => ResolvedType::String(string.clone()),
            Self::Number(number) => ResolvedType::Number(number.clone()),
            Self::Integer(integer) => ResolvedType::Integer(integer.clone()),
            Self::Object(object) => ResolvedType::Object(object.resolve_inline(cx)?),
            Self::Array(array) => ResolvedType::Array(array.resolve_inline(cx)?),
            Self::Boolean(boolean) => ResolvedType::Boolean(boolean.clone()),
        })
    }
}

impl Resolvable for ObjectType {
    type Resolved = ResolvedObjectType;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedObjectType {
            properties: cx.ref_or_boxed_map(&self.properties)?,
            required: self.required.clone(),
            additional_properties: self
                .additional_properties
                .as_ref()
                .map(|additional| additional.resolve_inline(cx))
                .transpose()?,
            min_properties: self.min_properties,
            max_properties: self.max_properties,
        })
    }
}

impl Resolvable for ArrayType {
    type Resolved = ResolvedArrayType;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedArrayType {
            items: self
                .items
                .as_ref()
                .map(|items| cx.ref_or_boxed(items))
                .transpose()?,
            min_items: self.min_items,
            max_items: self.max_items,
            unique_items: self.unique_items,
        })
    }
}

impl Resolvable for AnySchema {
    type Resolved = ResolvedAnySchema;

    fn resolve_inline(&self, cx: &mut Resolver<'_>) -> Result<Self::Resolved, ResolveError> {
        Ok(ResolvedAnySchema {
            typ: self.typ.clone(),
            pattern: self.pattern.clone(),
            multiple_of: self.multiple_of,
            exclusive_minimum: self.exclusive_minimum,
            exclusive_maximum: self.exclusive_maximum,
            minimum: self.minimum,
            maximum: self.maximum,
            properties: cx.ref_or_boxed_map(&self.properties)?,
            required: self.required.clone(),
            additional_properties: self
                .additional_properties
                .as_ref()
                .map(|additional| additional.resolve_inline(cx))
                .transpose()?,
            min_properties: self.min_properties,
            max_properties: self.max_properties,
            items: self
                .items
                .as_ref()
                .map(|items| cx.ref_or_boxed(items))
                .transpose()?,
            min_items: self.min_items,
            max_items: self.max_items,
            unique_items: self.unique_items,
            enumeration: self.enumeration.clone(),
            format: self.format.clone(),
            min_length: self.min_length,
            max_length: self.max_length,
            one_of: cx.ref_or_vec(&self.one_of)?,
            all_of: cx.ref_or_vec(&self.all_of)?,
            any_of: cx.ref_or_vec(&self.any_of)?,
            not: self.not.as_ref().map(|not| cx.ref_or(not)).transpose()?,
        })
    }
}
