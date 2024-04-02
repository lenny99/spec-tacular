use crate::util::*;
use anyhow::{bail, Result};
use indexmap::{indexmap, IndexMap};
use itertools::Itertools;
use mediatype::{
    names::{APPLICATION, JSON},
    MediaTypeBuf,
};
use pest::Parser;
use std::{rc::Rc, vec};

pub fn parse(input: &str) -> Result<ApiScript> {
    let mut parse_tree = ApiScript::parse(Rule::ApiScript, input)?;
    let root = parse_tree.next().unwrap();
    let api_script = ApiScript::from(root)?;
    return Ok(api_script);
}

#[derive(Parser, Debug)]
#[grammar = "grammar.pest"]
pub struct ApiScript {
    pub schemas: Vec<Schema>,
    pub apis: Vec<Api>,
}

impl ApiScript {
    fn from(tree: Node) -> Result<Self> {
        let mut api_script = ApiScript {
            schemas: vec![],
            apis: vec![],
        };

        for pair in tree.into_inner() {
            match pair.as_rule() {
                Rule::Schema => api_script.schemas.push(api_script.schema(pair)?),
                Rule::Api => api_script.apis.push(api_script.api(pair)?),
                Rule::Type => api_script.schemas.push(api_script.kind(pair)?),
                _ => (),
            }
        }
        return Ok(api_script);
    }

    fn kind(&self, kind: Node) -> Result<Schema> {
        let mut nodes = kind.into_inner();
        let identifier = nodes.next().unwrap().as_str();
        let definition = self.type_def(nodes.next().unwrap())?;
        return Ok(Schema {
            identifier: identifier.to_string(),
            schema_definition: definition,
        });
    }

    fn api(&self, api: Node) -> Result<Api> {
        let mut nodes = api.into_inner();
        let _ = nodes.expect_next_token(Rule::Annotations)?;
        let identifier = nodes.expect_next_token(Rule::Identifier)?.as_str();
        let version = nodes.expect_next_token(Rule::String)?.as_str();
        let path_nodes = nodes.expect_next_token(Rule::ApiBody)?;

        let mut paths: IndexMap<String, Path> = indexmap!();
        let mut servers: Vec<String> = vec![];
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
        let mut schema = self.type_def(iter.expect_next_token(Rule::SchemaDefinition)?)?;
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
        let type_def = self.type_def(inners.expect_next_token(Rule::SchemaDefinition)?)?;

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
        let schema_definition = self.schema_definition(inners.next().unwrap())?;
        return Ok(Schema {
            identifier: identificator.to_owned(),
            schema_definition,
        });
    }

    fn schema_definition(&self, schema_definition: Node) -> Result<Definition> {
        if schema_definition.as_rule() == Rule::Fields {
            let inners = schema_definition.into_inner();
            let fields = self.fields(inners)?;
            return Ok(Definition::Object { fields });
        } else {
            //print!("{:#?}", inner);
            unimplemented!()
        }
    }

    fn type_def(&self, node: Node) -> Result<Definition> {
        let mut inners = node.into_inner().into_iter();
        let name = inners.next().unwrap();
        let schema = self.parse_type(name)?;
        return Ok(schema);
    }

    fn parse_type(&self, node: Node) -> Result<Definition> {
        if let Rule::SchemaDefinition = node.as_rule() {
            return self.type_def(node);
        }
        if let Rule::List = node.as_rule() {
            // TODO allow declaring types in lists?
            let name = node
                .into_inner()
                .expect_next_token(Rule::SchemaDefinition)?
                .into_inner()
                .expect_next_token(Rule::Identifier)?;
            let ident: &str = &self.find_schema(name.as_str())?.identifier;
            let reference = Definition::Reference { name: ident.into() };
            return Ok(Definition::Array {
                schema: Rc::new(reference),
            });
        }
        if let Rule::Identifier = node.as_rule() {
            let schema = self.find_schema(node.as_str())?;
            return Ok(schema.into());
        }
        if let Rule::Primitive = node.as_rule() {
            let primitive = ApiScript::primitive(node)?;

            return Ok(Definition::Primitive(Basic {
                privitive: primitive,
                format: None,
                constraints: Vec::new(),
            }));
        }

        bail!("Could not determine type")
    }

    fn find_schema(&self, identificator: &str) -> Result<&Schema> {
        let schema = self
            .schemas
            .iter()
            .find(|schema| schema.identifier == identificator);
        if schema.is_none() {
            bail!("{identificator} is not a kown type");
        }
        return Ok(schema.unwrap());
    }

    fn fields(&self, nodes: Nodes) -> Result<Vec<Field>> {
        let mut fields = Vec::<Field>::new();
        for node in nodes {
            let inners = node.into_inner();
            let required = inners.len() == 2;
            for (property, kind) in inners.tuples() {
                fields.push(Field::new(
                    property.as_str().into(),
                    self.type_def(kind)?,
                    required,
                ));
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

    fn primitive(primitive: Node) -> Result<Primitive> {
        match primitive.as_str() {
            "string" => Ok(Primitive::String),
            "number" => Ok(Primitive::Number),
            "integer" => Ok(Primitive::Integer),
            "boolean" => Ok(Primitive::Boolean),
            _ => bail!("{primitive} was not a Primitive"),
        }
    }
}

#[derive(Debug, Clone)]
struct ParameterAnnotations {
    parameter_type: ParameterType,
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
                "Path" => parameter_type = Some(ParameterType::Path),
                "Query" => parameter_type = Some(ParameterType::Query),
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
            parameter_type: parameter_type.unwrap_or(ParameterType::Query),
            constraints,
        });
    }
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub identifier: String,
    pub schema_definition: Definition,
}

impl Into<Definition> for &Schema {
    fn into(self) -> Definition {
        return Definition::Reference {
            name: self.identifier.clone(),
        };
    }
}

#[derive(Debug, Clone)]
pub enum Definition {
    Primitive(Basic),
    Array { schema: Rc<Definition> },
    Object { fields: Vec<Field> },
    // TODO move into ReferenceOr type like openapiv3
    Reference { name: String },
}

impl Definition {
    fn constrained_by(self: &mut Self, annotations: &ParameterAnnotations) {
        match self {
            Definition::Primitive(basic) => basic.constrained_by(annotations),
            Definition::Array { schema } => todo!(),
            Definition::Object { fields } => todo!(),
            Definition::Reference { name } => todo!(),
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Basic {
    privitive: Primitive,
    format: Option<Format>,
    constraints: Vec<Constraint>,
}

impl Basic {
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
    definition: Definition,
    required: bool,
}

impl Field {
    fn new(name: String, definition: Definition, required: bool) -> Self {
        Field {
            name,
            definition,
            required,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Primitive {
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
    parameter_type: ParameterType,
    constraints: Vec<Constraint>,
}

impl Parameter {
    fn new<S>(identifier: S, kind: Definition, annotations: ParameterAnnotations) -> Parameter
    where
        S: AsRef<str>,
    {
        Parameter {
            name: identifier.as_ref().to_string(),
            parameter_type: annotations.parameter_type,
            constraints: annotations.constraints,
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParameterType {
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
