use anyhow::{bail, Result};
use indexmap::{indexmap, IndexMap};
use itertools::Itertools;
use mediatype::{
    names::{APPLICATION, JSON},
    MediaTypeBuf,
};
use pest::Parser;
use std::rc::Rc;

use crate::{
    ast::{self, Document},
    util::*,
};

pub fn parse(input: &str) -> Result<ast::Document> {
    let mut tree = ApiParser::parse(Rule::ApiScript, input)?;
    let root = tree.next().unwrap();
    let api_script = parse_tree(root)?;
    return Ok(api_script);
}

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct ApiParser;

fn parse_tree(tree: Node) -> Result<ast::Document> {
    let mut api_script = ast::Document {
        schemas: vec![],
        apis: vec![],
    };

    for pair in tree.into_inner() {
        match pair.as_rule() {
            Rule::Schema => {
                let schema = parse_schema(&api_script, pair)?;
                api_script.schemas.push(Rc::new(schema));
            }
            Rule::Api => {
                let api = parse_api(&api_script, pair)?;
                api_script.apis.push(api);
            }
            Rule::Type => {
                let kind = parse_type(&api_script, pair)?;
                api_script.schemas.push(Rc::new(kind));
            }
            _ => (),
        }
    }
    return Ok(api_script);
}

fn parse_type(api: &Document, kind: Node) -> Result<ast::Schema> {
    let mut nodes = kind.into_inner();
    let identifier = nodes.expect_next_token(Rule::Identifier)?.as_str();
    let definition = parse_type_definition(api, nodes.next().expect_token(Rule::TypeDefinition)?)?;
    return Ok(ast::Schema {
        identifier: identifier.to_string(),
        definition,
    });
}

fn parse_api(doc: &ast::Document, node: Node) -> Result<ast::API> {
    let mut nodes = node.into_inner();
    let _ = nodes.expect_next_token(Rule::Annotations)?;
    let identifier = nodes.expect_next_token(Rule::String)?.as_str();
    let version = nodes.expect_next_token(Rule::String)?.as_str();
    let path_nodes = nodes.expect_next_token(Rule::ApiBody)?;

    let mut paths: IndexMap<String, ast::Path> = indexmap!();
    let _servers: Vec<String> = vec![];
    for path_node in path_nodes.into_inner() {
        match path_node.as_rule() {
            Rule::Server => (),
            Rule::ApiPath => {
                let (url, path) = parse_path(doc, path_node)?;
                paths.insert(url, path);
            }
            _ => (),
        }
    }

    return Ok(ast::API::new(identifier.into(), version.into(), paths));
}

fn parse_path(doc: &ast::Document, path: Node) -> Result<(String, ast::Path)> {
    let mut inners = path.into_inner();
    let url_path = inners.expect_next_token(Rule::UrlPath)?.as_str();
    let endpoint_nodes = inners.expect_next_token(Rule::Endpoints)?;

    let mut endpoints: IndexMap<ast::HttpMethod, ast::Endpoint> = indexmap!();
    for endpoint_node in endpoint_nodes.into_inner() {
        let (method, endpoint) = parse_endpoint(doc, endpoint_node)?;
        endpoints.insert(method, endpoint);
    }

    return Ok((url_path.into(), ast::Path::new(endpoints)));
}

fn parse_endpoint(doc: &ast::Document, endpoint: Node) -> Result<(ast::HttpMethod, ast::Endpoint)> {
    let mut inners = endpoint.into_inner();
    let _ = inners.expect_next_token(Rule::Annotations)?;
    let method: ast::HttpMethod = inners.expect_next_token(Rule::Method)?.into();
    let operation_id = inners.expect_next_token(Rule::Identifier)?.as_str();

    let parameter_node = inners.expect_next_token(Rule::Parameters)?;
    let mut parameters = vec![];
    for node in parameter_node.into_inner() {
        parameters.push(parse_parameter(doc, node)?);
    }

    let respone_nodes = inners.expect_next_token(Rule::Responses)?;
    let mut responses: IndexMap<u16, ast::Responses> = indexmap!();
    for response_node in respone_nodes.into_inner() {
        let (http, response) = parse_response(doc, response_node)?;
        responses.insert(http, response);
    }

    return Ok((
        method,
        ast::Endpoint::new(operation_id.into(), parameters, responses),
    ));
}

fn parse_parameter(api: &Document, parameter: Node) -> Result<ast::Parameter> {
    let mut iter = parameter.into_inner();
    let annotations = parse_parameter_annotations(iter.expect_next_token(Rule::Annotations)?)?;
    let identifier = iter.expect_next_token(Rule::Identifier)?;
    let kind = parse_type_definition(api, iter.expect_next_token(Rule::TypeDefinition)?)?;
    // TODO apply annotations at type creaton? create new types when existing types are
    // referenced with annotations?
    // TODO kind.constrained_by(&annotations);
    return Ok(ast::Parameter::new(
        identifier.as_str().to_string(),
        kind,
        annotations,
    ));
}

fn parse_response(doc: &ast::Document, response: Node) -> Result<(ast::HttpCode, ast::Responses)> {
    assert!(response.as_rule() == Rule::Response);
    let mut inners = response.into_inner();
    let _ = inners.expect_next_token(Rule::Annotations)?;
    let http_code: u16 = inners.expect_next_token(Rule::HttpCode)?.as_str().parse()?;
    let type_def = parse_type_definition(doc, inners.expect_next_token(Rule::TypeDefinition)?)?;

    return Ok((
        http_code,
        indexmap! {
           MediaTypeBuf::new(APPLICATION, JSON) => type_def
        },
    ));
}

fn parse_schema(api: &Document, schema: Node) -> Result<ast::Schema> {
    let mut inners = schema.into_inner().into_iter();
    let identificator = inners.next().unwrap().as_str();
    let definition = {
        let schema_definition = inners.next().unwrap();
        if schema_definition.as_rule() == Rule::Fields {
            let inners = schema_definition.into_inner();
            let fields = parse_fields(api, inners)?;
            ast::Definition::Object(fields)
        } else {
            bail!("{identificator} was not an object");
        }
    };
    return Ok(ast::Schema::new(
        identificator.to_owned(),
        ReferenceOr::Actual(definition),
    ));
}

fn parse_type_definition(
    doc: &Document,
    mut node: Node,
) -> Result<ReferenceOr<ast::Schema, ast::Definition>> {
    assert!(node.as_rule() == Rule::TypeDefinition);
    node = node.into_inner().next().unwrap();
    let definition = if let Rule::Identifier = node.as_rule() {
        let identifier = node.as_str();
        let schema = find_schema(doc, identifier)?;
        ReferenceOr::Reference(schema)
    } else {
        let definition = parse_definition(doc, node)?;
        ReferenceOr::Actual(definition)
    };
    return Ok(definition);
}

fn parse_definition(doc: &ast::Document, node: Node) -> Result<ast::Definition> {
    let definition = match node.as_rule() {
        Rule::List => {
            // TODO allow declaring types in lists?
            let name = node
                .into_inner()
                .expect_next_token(Rule::TypeDefinition)?
                .into_inner()
                .expect_next_token(Rule::Identifier)?;
            let schema = find_schema(doc, name.as_str())?;
            ast::Definition::Array(schema)
        }
        Rule::Kind => {
            let primitive = parse_primitive(doc, node)?;
            ast::Definition::Primitive(primitive)
        }
        //Rule::Identifier => return self.parse_reference_or_definition(node),
        _ => {
            panic!("{:#?}", node);
        }
    };
    return Ok(definition);
}

fn parse_primitive(api: &Document, node: Node) -> Result<ast::Primitive> {
    let mut tokens = node.into_inner();
    let kind = parse_primitive_kind(tokens.expect_next_token(Rule::Primitive)?)?;
    let format = if let Some(token) = tokens.next() {
        assert!(token.as_rule() == Rule::Format);
        Some(parse_format(token)?)
    } else {
        None
    };
    return Ok(ast::Primitive::new(kind, format, Vec::new()));
}

fn find_schema(api: &Document, identificator: &str) -> Result<Rc<ast::Schema>> {
    let schema = api
        .schemas
        .iter()
        .find(|schema| schema.identifier == identificator);
    if let Some(schema) = schema {
        return Ok(schema.to_owned());
    } else {
        bail!("{identificator} is not a kown type");
    }
}

fn parse_fields(api: &Document, nodes: Nodes) -> Result<Vec<ast::Field>> {
    let mut fields = Vec::<ast::Field>::new();
    for node in nodes {
        let inners = node.into_inner();
        let required = inners.len() == 2;
        for (property, kind) in inners.tuples() {
            let kind = parse_type_definition(api, kind)?;
            fields.push(ast::Field::new(property.as_str().into(), kind, required));
        }
    }
    return Ok(fields);
}

fn parse_format(format: Node) -> Result<ast::Format> {
    assert!(format.as_rule() == Rule::Format);
    let parsed = match format.as_str() {
        "date" => ast::Format::String(ast::StringFormat::Date),
        "date-time" => ast::Format::String(ast::StringFormat::DateTime),
        "password" => ast::Format::String(ast::StringFormat::Password),
        "byte" => ast::Format::String(ast::StringFormat::Byte),
        "binary" => ast::Format::String(ast::StringFormat::Binary),
        "email" => ast::Format::String(ast::StringFormat::Email),
        "uuid" => ast::Format::String(ast::StringFormat::Uuid),
        "uri" => ast::Format::String(ast::StringFormat::Uri),
        "hostname" => ast::Format::String(ast::StringFormat::Hostname),
        "ipv4" => ast::Format::String(ast::StringFormat::Ipv4),
        "ipv6" => ast::Format::String(ast::StringFormat::Ipv6),
        "int32" => ast::Format::Integer(ast::IntegerFormat::Int32),
        "int64" => ast::Format::Integer(ast::IntegerFormat::Int64),
        "float" => ast::Format::Number(ast::NumberFormat::Float),
        "double" => ast::Format::Number(ast::NumberFormat::Double),
        other if other.starts_with("x-") => {
            ast::Format::String(ast::StringFormat::Custom(other.to_string()))
        }
        _ => anyhow::bail!("unsupported format: {format}"),
    };
    return Ok(parsed);
}

fn parse_primitive_kind(node: Node) -> Result<ast::Kind> {
    assert!(
        node.as_rule() == Rule::Primitive,
        "node was a {}",
        node.as_rule()
    );
    match node.as_str() {
        "string" => Ok(ast::Kind::String),
        "number" => Ok(ast::Kind::Number),
        "integer" => Ok(ast::Kind::Integer),
        "boolean" => Ok(ast::Kind::Boolean),
        _ => bail!("{node} was not a Primitive"),
    }
}

impl<'a> Into<ast::HttpMethod> for Node<'a> {
    fn into(self) -> ast::HttpMethod {
        assert!(self.as_rule() == Rule::Method);
        match self.as_str() {
            "GET" => ast::HttpMethod::Get,
            "PUT" => ast::HttpMethod::Put,
            "POST" => ast::HttpMethod::Post,
            "DELETE" => ast::HttpMethod::Delete,
            "PATCH" => ast::HttpMethod::Patch,
            &_ => unreachable!(),
        }
    }
}

fn parse_parameter_annotations(annotations: Node) -> Result<ast::ParameterAnnotations> {
    let mut parameter_type = None;
    let mut constraints = vec![];

    for annotation in annotations.into_inner() {
        let mut tokens = annotation.into_inner();
        let identifier = tokens.expect_next_token(Rule::Identifier)?;
        match identifier.as_str() {
            "Path" => parameter_type = Some(ast::ParameterLocation::Path),
            "Query" => parameter_type = Some(ast::ParameterLocation::Query),
            "Max" => {
                let max = tokens
                    .expect_next_token(Rule::NumberValue)?
                    .as_str()
                    .parse()?;
                constraints.push(ast::Constraint::Maximum(max));
            }
            _ => (),
        }
    }

    let location = parameter_type.unwrap_or(ast::ParameterLocation::Query);
    return Ok(ast::ParameterAnnotations::new(location, constraints));
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};

    use super::*;

    #[test]
    fn empty_file() {
        let src = "";
        let Ok(document) = super::parse(src) else {
            panic!("failed")
        };
        assert!(document.apis.is_empty());
        assert!(document.schemas.is_empty());
    }

    #[inline]
    fn snapshot(name: &str, src: &str) {
        match super::parse(src) {
            Ok(document) => assert_debug_snapshot!(name, document),
            Err(error) => panic!("failed to parse: {}", error),
        }
    }

    #[test]
    fn date_type() {
        snapshot("date_type", "type Date: string#date");
    }

    #[test]
    fn container_schema() {
        snapshot(
            "container_schema",
            "
            type ContainerId: string

            schema Container {
              id: ContainerId
              tare: integer?
              gross: integer?
            }
            ",
        );
    }

    #[test]
    fn booking_document() {
        snapshot(
            "booking_document",
            "
            type ContainerId: string

            schema Container {
              id: ContainerId
              tare: integer?
              gross: integer?
            }

            schema Booking {
              delivery: string#date
              address: string
              containers: [Container]
            }

            schema Problem {
              description: string
            }

            api \"container-api\" \"0.0.1\" {
              path \"/containers\" {
                GET listContainers(@Query @Max(100) limit: integer) {
                  200: Container
                  401: Problem
                }
              }
              path \"/booking/{id}\" {
                GET getBooking(@Path id: string) {
                  200: Booking
                  401: Problem
                }
              }
            }
            ",
        );
    }
}
