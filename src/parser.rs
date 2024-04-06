use anyhow::{bail, Result};
use indexmap::{indexmap, IndexMap};
use itertools::Itertools;
use mediatype::{
    names::{APPLICATION, JSON},
    MediaTypeBuf,
};
use pest::Parser;
use std::{rc::Rc, vec};

use crate::util::*;

pub fn parse(input: &str) -> Result<ApiScript> {
    let mut parse_tree = ApiScript::parse(Rule::ApiScript, input)?;
    let root = parse_tree.next().unwrap();
    let api_script = ApiScript::from(root)?;
    return Ok(api_script);
}

#[derive(Parser, Debug)]
#[grammar = "grammar.pest"]
pub struct ApiScript {
    pub schemas: Vec<Rc<Schema>>,
    pub apis: Vec<Api>,
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub identifier: String,
    pub definition: Definition,
}

impl Schema {
    fn new(identifier: String, definition: Definition) -> Schema {
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
    fn primitive(kind: Kind, format: Option<Format>, constraints: Vec<Constraint>) -> Definition {
        return Definition::Primitive(Primitive {
            kind,
            format,
            constraints,
        });
    }

    fn array(definition: Rc<Schema>) -> Definition {
        return Definition::Array(definition);
    }

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
    fn new(name: String, definition: ReferenceOr<Schema, Definition>, required: bool) -> Self {
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
pub struct Api {
    name: String,
    version: String,
    paths: IndexMap<String, Path>,
}

#[derive(Debug, Clone, Getters)]
pub struct Path {
    endpoints: IndexMap<HttpMethod, Endpoint>,
}

#[derive(Debug, Clone, Getters)]
pub struct Endpoint {
    operation_id: String,
    parameters: Vec<Parameter>,
    responses: IndexMap<u16, Responses>,
}

#[derive(Debug, Clone, Getters)]
pub struct Parameter {
    name: String,
    kind: Definition,
    location: ParameterLocation,
    constraints: Vec<Constraint>,
}

impl Parameter {
    fn new<S>(identifier: S, kind: Definition, annotations: ParameterAnnotations) -> Parameter
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

pub type Responses = IndexMap<MediaTypeBuf, Definition>;
pub type HttpCode = u16;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Patch,
    Delete,
}

impl ApiScript {
    fn from(tree: Node) -> Result<Self> {
        let mut api_script = ApiScript {
            schemas: vec![],
            apis: vec![],
        };

        for pair in tree.into_inner() {
            match pair.as_rule() {
                Rule::Schema => api_script.push_schema(api_script.schema(pair)?),
                Rule::Api => api_script.apis.push(api_script.api(pair)?),
                Rule::Type => api_script.push_schema(api_script.kind(pair)?),
                _ => (),
            }
        }
        return Ok(api_script);
    }

    fn push_schema(&mut self, schema: Schema) {
        self.schemas.push(Rc::new(schema));
    }

    fn kind(&self, kind: Node) -> Result<Schema> {
        let mut nodes = kind.into_inner();
        let identifier = nodes.next().unwrap().as_str();
        let definition = self.parse_schema(nodes.next().unwrap())?;
        return Ok(Schema {
            identifier: identifier.to_string(),
            definition,
        });
    }

    fn api(&self, api: Node) -> Result<Api> {
        let mut nodes = api.into_inner();
        let _ = nodes.expect_next_token(Rule::Annotations)?;
        let identifier = nodes.expect_next_token(Rule::Identifier)?.as_str();
        let version = nodes.expect_next_token(Rule::String)?.as_str();
        let path_nodes = nodes.expect_next_token(Rule::ApiBody)?;

        let mut paths: IndexMap<String, Path> = indexmap!();
        let _servers: Vec<String> = vec![];
        for path_node in path_nodes.into_inner() {
            match path_node.as_rule() {
                Rule::Server => (),
                Rule::ApiPath => {
                    let (url, path) = self.path(path_node)?;
                    paths.insert(url, path);
                }
                _ => (),
            }
        }

        return Ok(Api {
            name: identifier.into(),
            version: version.into(),
            paths,
        });
    }

    fn path(&self, path: Node) -> Result<(String, Path)> {
        let mut inners = path.into_inner();
        let url_path = inners
            .next()
            .ok_or(ParseError::new("Expected path"))?
            .as_str();
        let endpoint_nodes = inners.next().ok_or(ParseError::new("Expected endpoints"))?;

        let mut endpoints: IndexMap<HttpMethod, Endpoint> = indexmap!();
        for endpoint_node in endpoint_nodes.into_inner() {
            let (method, endpoint) = self.endpoint(endpoint_node)?;
            endpoints.insert(method, endpoint);
        }

        return Ok((url_path.into(), Path { endpoints }));
    }

    fn endpoint(&self, endpoint: Node) -> Result<(HttpMethod, Endpoint)> {
        let mut inners = endpoint.into_inner();
        let method: HttpMethod = inners.expect_next_token(Rule::Method)?.into();
        let operation_id = inners.expect_next_token(Rule::Identifier)?.as_str();

        let parameter_node = inners.expect_next_token(Rule::Parameters)?;
        let mut parameters = vec![];
        for node in parameter_node.into_inner() {
            parameters.push(self.parameter(node)?);
        }

        let respone_nodes = inners.expect_next_token(Rule::Responses)?;
        let mut responses: IndexMap<u16, Responses> = indexmap!();
        for response_node in respone_nodes.into_inner() {
            let (http, response) = self.response(response_node)?;
            responses.insert(http, response);
        }

        return Ok((
            method,
            Endpoint {
                operation_id: operation_id.into(),
                parameters,
                responses,
            },
        ));
    }

    fn parameter(&self, parameter: Node) -> Result<Parameter> {
        let mut iter = parameter.into_inner();
        let annotations =
            ParameterAnnotations::process(iter.expect_next_token(Rule::Annotations)?)?;
        let identifier = iter.expect_next_token(Rule::Identifier)?;
        let mut schema = self.parse_schema(iter.expect_next_token(Rule::SchemaDefinition)?)?;
        // TODO apply annotations at type creaton? create new types when existing types are
        // referenced with annotations?
        schema.constrained_by(&annotations);
        return Ok(Parameter::new(
            identifier.as_str().to_string(),
            schema,
            annotations,
        ));
    }

    fn response(&self, response: Node) -> Result<(HttpCode, Responses)> {
        assert!(response.as_rule() == Rule::Response);
        let mut inners = response.into_inner();

        let http_code: u16 = inners.expect_next_token(Rule::HttpCode)?.as_str().parse()?;
        let type_def = self.parse_schema(inners.expect_next_token(Rule::SchemaDefinition)?)?;

        return Ok((
            http_code,
            indexmap! {
               MediaTypeBuf::new(APPLICATION, JSON) => type_def
            },
        ));
    }

    fn schema(&self, schema: Node) -> Result<Schema> {
        let mut inners = schema.into_inner().into_iter();
        let identificator = inners.next().unwrap().as_str();
        let definition = {
            let schema_definition = inners.next().unwrap();
            if schema_definition.as_rule() == Rule::Fields {
                let inners = schema_definition.into_inner();
                let fields = self.parse_fields(inners)?;
                Definition::Object(fields)
            } else {
                bail!("{identificator} was not an object");
            }
        };

        return Ok(Schema::new(identificator.to_owned(), definition));
    }

    fn parse_schema(&self, node: Node) -> Result<Definition> {
        let mut inners = node.into_inner().into_iter();
        let name = inners.next().unwrap();
        let schema = self.parse_definition(name)?;
        return Ok(schema);
    }

    fn parse_reference_or_definition(&self, node: Node) -> Result<ReferenceOr<Schema, Definition>> {
        let definition = if let Rule::Identifier = node.as_rule() {
            let identifier = node.as_str();
            let schema = self.find_schema(identifier)?;
            ReferenceOr::Reference(schema)
        } else {
            let definition = self.parse_definition(node)?;
            ReferenceOr::Actual(definition)
        };
        return Ok(definition);
    }

    fn parse_definition(&self, node: Node) -> Result<Definition> {
        let definition = match node.as_rule() {
            Rule::SchemaDefinition => return self.parse_schema(node),
            Rule::List => {
                // TODO allow declaring types in lists?
                let name = node
                    .into_inner()
                    .expect_next_token(Rule::SchemaDefinition)?
                    .into_inner()
                    .expect_next_token(Rule::Identifier)?;
                let schema = self.find_schema(name.as_str())?;
                Definition::array(schema)
            }
            Rule::Primitive => {
                let primitive = ApiScript::primitive(node)?;
                Definition::primitive(primitive, None, Vec::new())
            }
            _ => bail!("Could not determine type"),
        };
        return Ok(definition);
    }

    fn find_schema(&self, identificator: &str) -> Result<Rc<Schema>> {
        let schema = self
            .schemas
            .iter()
            .find(|schema| schema.identifier == identificator);
        if let Some(schema) = schema {
            return Ok(schema.to_owned());
        } else {
            bail!("{identificator} is not a kown type");
        }
    }

    fn parse_fields(&self, nodes: Nodes) -> Result<Vec<Field>> {
        let mut fields = Vec::<Field>::new();
        for node in nodes {
            let inners = node.into_inner();
            let required = inners.len() == 2;
            for (property, kind) in inners.tuples() {
                let kind = self.parse_reference_or_definition(kind)?;
                fields.push(Field::new(property.as_str().into(), kind, required));
            }
        }
        return Ok(fields);
    }

    fn format(format: Node) -> Format {
        let format_string = format.as_str();
        match format_string {
            "date" => Format::Date,
            "date-time" => Format::DateTime,
            "password" => Format::Password,
            "byte" => Format::Byte,
            "binary" => Format::Binary,
            _ => Format::Custom(format_string.to_owned()),
        }
    }

    fn primitive(primitive: Node) -> Result<Kind> {
        match primitive.as_str() {
            "string" => Ok(Kind::String),
            "number" => Ok(Kind::Number),
            "integer" => Ok(Kind::Integer),
            "boolean" => Ok(Kind::Boolean),
            _ => bail!("{primitive} was not a Primitive"),
        }
    }
}

#[derive(Debug, Clone)]
struct ParameterAnnotations {
    parameter_type: ParameterLocation,
    constraints: Vec<Constraint>,
}

impl ParameterAnnotations {
    fn process(annotations: Node) -> Result<ParameterAnnotations> {
        let mut parameter_type = None;
        let mut constraints = vec![];

        for annotation in annotations.into_inner() {
            let mut tokens = annotation.into_inner();
            let identifier = tokens.expect_next_token(Rule::Identifier)?;
            match identifier.as_str() {
                "Path" => parameter_type = Some(ParameterLocation::Path),
                "Query" => parameter_type = Some(ParameterLocation::Query),
                "Max" => {
                    let max = tokens
                        .expect_next_token(Rule::NumberValue)?
                        .as_str()
                        .parse()?;
                    constraints.push(Constraint::Maximum(max));
                }
                _ => (),
            }
        }

        return Ok(ParameterAnnotations {
            parameter_type: parameter_type.unwrap_or(ParameterLocation::Query),
            constraints,
        });
    }
}

impl<'a> Into<HttpMethod> for Node<'a> {
    fn into(self) -> HttpMethod {
        match self.as_str() {
            "GET" => HttpMethod::Get,
            "PUT" => HttpMethod::Put,
            "POST" => HttpMethod::Post,
            "DELETE" => HttpMethod::Delete,
            "PATCH" => HttpMethod::Patch,
            &_ => unreachable!(),
        }
    }
}
