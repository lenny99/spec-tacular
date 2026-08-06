use anyhow::Result;
use indexmap::IndexMap;
use mediatype::MediaTypeBuf;
use std::fmt::Display;

use crate::util::ReferenceOr;
use std::rc::Rc;

#[derive(Debug)]
pub struct Document {
    pub schemas: Vec<Rc<Schema>>,
    pub apis: Vec<API>,
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub identifier: String,
    pub definition: ReferenceOr<Schema, Definition>,
}

impl Schema {
    pub(crate) fn new(identifier: String, definition: ReferenceOr<Schema, Definition>) -> Schema {
        return Schema {
            identifier,
            definition,
        };
    }
}

#[derive(Debug, Clone)]
pub enum Definition {
    Primitive(Primitive),
    Array(Rc<Schema>),
    Object(Vec<Field>),
}

impl Definition {
    fn constrained_by(self: &mut Self, annotations: &ParameterAnnotations) {
        match self {
            Definition::Primitive(basic) => basic.constrained_by(annotations),
            Definition::Array(_element) => todo!(),
            Definition::Object(_fields) => todo!(),
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Primitive {
    kind: Kind,
    format: Option<Format>,
    constraints: Vec<Constraint>,
}

impl Primitive {
    pub(crate) fn new(kind: Kind, format: Option<Format>, constraints: Vec<Constraint>) -> Self {
        Self {
            kind,
            format,
            constraints,
        }
    }

    fn constrained_by(self: &mut Self, annotations: &ParameterAnnotations) {
        for constraint in &annotations.constraints {
            self.constraints.push(constraint.clone())
        }
    }
}

#[derive(Debug, Clone)]
pub enum Constraint {
    Maximum(usize),
    Minimum(usize),
}

#[derive(Debug, Clone, Getters)]
pub struct Field {
    name: String,
    definition: ReferenceOr<Schema, Definition>,
    required: bool,
}

impl Field {
    pub fn new(name: String, definition: ReferenceOr<Schema, Definition>, required: bool) -> Self {
        Field {
            name,
            definition,
            required,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Kind {
    String,
    Number,
    Integer,
    Boolean,
}

#[derive(Debug, Clone)]
pub enum Format {
    Date,
    DateTime,
    Password,
    Byte,
    Binary,
    Custom(String),
}

#[derive(Debug, Clone, Getters)]
pub struct API {
    name: String,
    version: String,
    paths: IndexMap<String, Path>,
}
impl API {
    pub(crate) fn new(name: String, version: String, paths: IndexMap<String, Path>) -> Self {
        Self {
            name,
            version,
            paths,
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Path {
    endpoints: IndexMap<HttpMethod, Endpoint>,
}

impl Path {
    pub fn new(endpoints: IndexMap<HttpMethod, Endpoint>) -> Self {
        Path { endpoints }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Endpoint {
    operation_id: String,
    parameters: Vec<Parameter>,
    responses: IndexMap<u16, Responses>,
}

impl Endpoint {
    pub(crate) fn new(
        operation_id: String,
        parameters: Vec<Parameter>,
        responses: IndexMap<u16, IndexMap<MediaTypeBuf, ReferenceOr<Schema, Definition>>>,
    ) -> Self {
        Self {
            operation_id,
            parameters,
            responses,
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Parameter {
    name: String,
    kind: ReferenceOr<Schema, Definition>,
    location: ParameterLocation,
    constraints: Vec<Constraint>,
}

impl Parameter {
    pub fn new<S>(
        identifier: S,
        kind: ReferenceOr<Schema, Definition>,
        annotations: ParameterAnnotations,
    ) -> Parameter
    where
        S: AsRef<str>,
    {
        Parameter {
            name: identifier.as_ref().to_string(),
            location: annotations.parameter_type,
            constraints: annotations.constraints,
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParameterLocation {
    Query,
    Path,
    Header,
    Cookie,
}

pub type Responses = IndexMap<MediaTypeBuf, ReferenceOr<Schema, Definition>>;
pub type HttpCode = u16;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Patch,
    Delete,
}

#[derive(Debug, Clone)]
pub struct ParameterAnnotations {
    parameter_type: ParameterLocation,
    constraints: Vec<Constraint>,
}

impl ParameterAnnotations {
    pub(crate) fn new(location: ParameterLocation, constraints: Vec<Constraint>) -> Self {
        Self {
            parameter_type: location,
            constraints: constraints,
        }
    }
}
