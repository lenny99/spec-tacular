use anyhow::Result;
use clap::Parser;
use itertools::Itertools;
use openapiv3::OpenAPI;
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

extern crate pest;
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate derive_getters;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    match args.command {
        cli::Commands::Compile { file, output } => {
            let apis = compile(file.as_path())?;
            if let Some(output) = output {
                match apis.len().cmp(&2) {
                    Ordering::Less | Ordering::Equal => write_to_file(&output, &apis)?,
                    Ordering::Greater => write_to_files(apis, output)?,
                }
            } else {
                let content = combine_apis(&apis)?;
                println!("{content}");
            }
        }
    }
    return Ok(());
}

fn write_to_files(apis: Vec<OpenAPI>, output: std::path::PathBuf) -> Result<(), anyhow::Error> {
    for api in apis {
        let ending = format!("/{}", api.info.title);
        let path = output.join(Path::new(ending.as_str()));
        std::fs::write(path, serde_yaml::to_string(&api)?)?;
    }
    return Ok(());
}

fn write_to_file(output: &std::path::PathBuf, apis: &Vec<OpenAPI>) -> Result<(), anyhow::Error> {
    if (output.is_file() || !output.exists()) {
        let content = combine_apis(apis)?;
        std::fs::write(output, content)?;
    }
    return Ok(());
}

fn combine_apis(apis: &Vec<OpenAPI>) -> Result<String> {
    let mut content = String::new();
    for api in apis {
        content += "---\n";
        content += serde_yaml::to_string(api)?.as_str();
    }
    return Ok(content);
}

fn compile(path: &std::path::Path) -> Result<Vec<OpenAPI>> {
    let input = fs::read_to_string(path)?;
    let api_script = parser::parse(&input)?;
    let apis = generator::generate(&api_script);
    return Ok(apis);
}

mod cli {
    use std::path::PathBuf;

    use clap::{Parser, Subcommand};
    use pest::pratt_parser::Op;

    #[derive(Parser)]
    #[command(arg_required_else_help(true))]
    pub struct Args {
        #[command(subcommand)]
        pub command: Commands,
    }

    #[derive(Subcommand, Clone)]
    pub enum Commands {
        Compile {
            file: PathBuf,
            #[arg(short, long)]
            output: Option<PathBuf>,
        },
    }
}

mod util;

mod parser {
    use crate::util::*;
    use anyhow::Result;
    use indexmap::{indexmap, IndexMap};
    use itertools::{Either, Itertools};
    use mediatype::{
        names::{APPLICATION, JSON},
        MediaTypeBuf,
    };
    use pest::Parser;
    use std::{ptr::null, rc::Rc, vec};

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
                    _ => (),
                }
            }
            return Ok(api_script);
        }

        fn api(&self, api: Node) -> Result<Api> {
            let mut nodes = api.into_inner();
            let identifier = nodes.next().unwrap().as_str();
            let version = nodes.next().unwrap().as_str();
            let path_nodes = nodes.next().unwrap();

            let mut paths: IndexMap<String, Path> = indexmap!();
            for path_node in path_nodes.into_inner() {
                let (url, path) = self.path(path_node)?;
                paths.insert(url, path);
            }

            return Ok(Api {
                name: identifier.into(),
                version: version.into(),
                paths: paths,
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

            let _parameters = inners.expect_next_token(Rule::Parameters)?; // TODO parameters

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
                    parameters: vec![],
                    responses: responses,
                },
            ));
        }

        fn response(&self, response: Node) -> Result<(HttpCode, Responses)> {
            assert!(response.as_rule() == Rule::Response);
            let mut inners = response.into_inner();

            let http_code: u16 = inners
                .expect_next_token(Rule::HTTP_CODE)?
                .as_str()
                .parse()?;
            let type_def = self.type_def(inners.expect_next_token(Rule::TypeDef)?)?;

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
                identificator: identificator.to_owned(),
                schema_definition,
            });
        }

        fn schema_definition(&self, schema_definition: Node) -> Result<SchemaDefinition> {
            let inner = schema_definition.into_inner().next().unwrap();
            let schema_definition = match inner.as_rule() {
                Rule::TypeDef => self.type_def(inner)?,
                Rule::Fields => {
                    let inners = inner.into_inner();
                    let fields = self.fields(inners)?;
                    SchemaDefinition::Object { fields: fields }
                }
                _ => unimplemented!(),
            };
            return Ok(schema_definition);
        }

        fn type_def(&self, node: Node) -> Result<SchemaDefinition> {
            let mut inners = node.into_inner().into_iter();
            let name = inners.next().unwrap();
            let schema_or_primitve = self.find_schema_or_parse_as_primitve(name)?;
            let definition = match schema_or_primitve {
                Either::Left(schema) => SchemaDefinition::Reference {
                    name: schema.identificator.clone(),
                },
                Either::Right(primitive) => {
                    let format = inners.next().map(ApiScript::format);
                    SchemaDefinition::NewType { primitive, format }
                }
            };
            return Ok(definition);
        }

        fn find_schema_or_parse_as_primitve(
            &self,
            node: Node,
        ) -> Result<Either<&Schema, Primitive>> {
            let schema = self.find_schema(node.as_str());
            if let Some(schema) = schema {
                return Ok(Either::Left(schema));
            }
            let primitive = ApiScript::primitive(node)?;
            return Ok(Either::Right(primitive));
        }

        fn find_schema(&self, identificator: &str) -> Option<&Schema> {
            self.schemas
                .iter()
                .find(|schema| schema.identificator == identificator)
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

        fn primitive(primitive: Node) -> Result<Primitive, ParseError> {
            match primitive.as_str() {
                "string" => Ok(Primitive::String),
                "number" => Ok(Primitive::Number),
                "integer" => Ok(Primitive::Integer),
                "boolean" => Ok(Primitive::Boolean),
                _ => Err(ParseError::mismatch(Rule::Primitive, primitive.as_str())),
            }
        }
    }

    #[derive(Debug)]
    pub struct Schema {
        pub identificator: String,
        pub schema_definition: SchemaDefinition,
    }

    #[derive(Debug, Clone)]
    pub enum SchemaDefinition {
        NewType {
            primitive: Primitive,
            format: Option<Format>,
        },
        Array {
            schema_ref: Rc<Schema>,
        },
        Object {
            fields: Vec<Field>,
        },
        // TODO move into ReferenceOr type like openapiv3
        Reference {
            name: String,
        },
    }

    #[derive(Debug, Clone, Getters)]
    pub struct Field {
        name: String,
        definition: SchemaDefinition,
        required: bool,
    }

    impl Field {
        fn new(name: String, definition: SchemaDefinition, required: bool) -> Self {
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

    #[derive(Debug, Clone)]
    pub struct Parameter {
        name: String,
        kind: SchemaDefinition,
    }

    pub type Responses = IndexMap<MediaTypeBuf, SchemaDefinition>;
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
}

mod generator {
    use crate::parser::{
        Api, ApiScript, Endpoint, Field, Format, HttpMethod, Path, Primitive, Schema,
        SchemaDefinition,
    };
    use indexmap::{indexmap, IndexMap};
    use mediatype::MediaTypeBuf;
    use openapiv3::{Components, Operation, PathItem, ReferenceOr, StatusCode};

    pub(crate) fn generate(api_script: &ApiScript) -> Vec<openapiv3::OpenAPI> {
        return api_script.generate();
    }

    impl ApiScript {
        fn generate(&self) -> Vec<openapiv3::OpenAPI> {
            let mut apis: Vec<openapiv3::OpenAPI> = vec![];
            for api in self.apis.iter() {
                let open_api = api.generate(&self);
                apis.push(open_api);
            }
            return apis;
        }

        fn generate_schemas(&self) -> IndexMap<String, ReferenceOr<openapiv3::Schema>> {
            return self.schemas.iter().map(Schema::generate).fold(
                IndexMap::new(),
                |mut map, (identificator, schema)| {
                    map.insert(identificator, openapiv3::ReferenceOr::Item(schema));
                    return map;
                },
            );
        }
    }

    impl Api {
        fn generate(&self, api: &ApiScript) -> openapiv3::OpenAPI {
            return openapiv3::OpenAPI {
                paths: self.generate_paths(),
                components: Option::Some(Components {
                    schemas: api.generate_schemas(),
                    ..Default::default()
                }),
                ..Default::default()
            };
        }

        fn generate_paths(&self) -> openapiv3::Paths {
            let mut paths: IndexMap<String, ReferenceOr<PathItem>> = indexmap!();
            for (url, path) in self.paths().iter() {
                let path = path.generate();
                paths.insert(url.into(), ReferenceOr::Item(path));
            }

            return openapiv3::Paths {
                paths: paths,
                extensions: indexmap!(),
            };
        }
    }

    impl Path {
        fn generate(&self) -> openapiv3::PathItem {
            let get = self
                .endpoints()
                .get(&HttpMethod::Get)
                .map(Endpoint::generate);

            return PathItem {
                get: get,
                ..Default::default()
            };
        }
    }

    impl Endpoint {
        fn generate(&self) -> Operation {
            return Operation {
                operation_id: Option::Some(self.operation_id().into()),
                responses: self.openapi_responses(),
                ..Default::default()
            };
        }

        fn openapi_responses(&self) -> openapiv3::Responses {
            let mut responses = indexmap!();
            for (http_code, media_responses) in self.responses() {
                let response = openapiv3::Response {
                    content: Endpoint::to_content(media_responses),
                    ..Default::default()
                };
                responses.insert(StatusCode::Code(*http_code), ReferenceOr::Item(response));
            }

            return openapiv3::Responses {
                responses,
                ..Default::default()
            };
        }

        fn to_content(
            map: &IndexMap<MediaTypeBuf, SchemaDefinition>,
        ) -> IndexMap<String, openapiv3::MediaType> {
            let mut result = indexmap!();
            for (media_type, definition) in map {
                let reference_or_schema = definition.into();
                result.insert(
                    media_type.to_string(),
                    openapiv3::MediaType {
                        schema: Option::Some(reference_or_schema),
                        ..Default::default()
                    },
                );
            }
            return result;
        }
    }

    impl Into<openapiv3::Responses> for Endpoint {
        fn into(self) -> openapiv3::Responses {
            let mut responses = indexmap!();
            for (http_code, media_responses) in self.responses() {
                let response = openapiv3::Response {
                    content: Endpoint::to_content(media_responses),
                    ..Default::default()
                };
                responses.insert(StatusCode::Code(*http_code), ReferenceOr::Item(response));
            }

            return openapiv3::Responses {
                responses,
                ..Default::default()
            };
        }
    }

    impl Schema {
        fn generate(&self) -> (String, openapiv3::Schema) {
            let schema = (&self.schema_definition).into();
            return (self.identificator.to_owned(), schema);
        }
    }

    type ReferenceOrSchemaKind = openapiv3::ReferenceOr<openapiv3::SchemaKind>;

    impl Into<openapiv3::Schema> for &SchemaDefinition {
        fn into(self) -> openapiv3::Schema {
            match &self {
                SchemaDefinition::NewType { primitive, format } => {
                    return openapiv3::Schema {
                        schema_kind: primitive.as_schema_type(format.clone()), // ?
                        schema_data: openapiv3::SchemaData::default(),
                    };
                }
                SchemaDefinition::Object { fields } => {
                    return openapiv3::Schema {
                        schema_data: openapiv3::SchemaData {
                            ..Default::default()
                        },
                        schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::Object(
                            openapiv3::ObjectType {
                                properties: SchemaDefinition::properties(fields),
                                required: SchemaDefinition::required_properties(fields),
                                ..Default::default()
                            },
                        )),
                    }
                }
                _ => todo!(),
            };
        }
    }

    impl SchemaDefinition {
        fn properties(
            fields: &Vec<Field>,
        ) -> IndexMap<String, openapiv3::ReferenceOr<Box<openapiv3::Schema>>> {
            let mut map = IndexMap::new();
            for field in fields {
                let kind: ReferenceOrSchemaKind = field.definition().into();
                match kind {
                    ReferenceOr::Item(kind) => {
                        let schema = openapiv3::Schema {
                            schema_data: Default::default(),
                            schema_kind: kind,
                        };
                        map.insert(field.name().clone(), ReferenceOr::Item(Box::new(schema)));
                    }
                    ReferenceOr::Reference { reference } => {
                        map.insert(field.name().clone(), ReferenceOr::Reference { reference });
                    }
                }
            }
            return map;
        }

        fn required_properties(feilds: &Vec<Field>) -> Vec<String> {
            feilds
                .iter()
                .filter(|f| *f.required())
                .map(|f| f.name().clone())
                .collect()
        }
    }

    impl Into<ReferenceOrSchemaKind> for &SchemaDefinition {
        fn into(self) -> ReferenceOrSchemaKind {
            match self {
                SchemaDefinition::NewType { primitive, format } => {
                    let schema_kind = primitive.as_schema_type(format.clone());
                    return ReferenceOrSchemaKind::Item(schema_kind);
                }
                SchemaDefinition::Reference { name } => {
                    let path = format!("#/components/schemas/{}", name);
                    return ReferenceOrSchemaKind::Reference { reference: path };
                }
                _ => todo!(),
            }
        }
    }

    impl Into<ReferenceOr<openapiv3::Schema>> for &SchemaDefinition {
        fn into(self) -> ReferenceOr<openapiv3::Schema> {
            if let SchemaDefinition::Reference { name } = self {
                return ReferenceOr::Reference {
                    reference: format!("#/components/schemas/{name}"),
                };
            }

            let schema = self.into();
            return ReferenceOr::Item(schema);
        }
    }

    impl Primitive {
        fn as_schema_type(&self, format: Option<Format>) -> openapiv3::SchemaKind {
            let openapi_format: openapiv3::VariantOrUnknownOrEmpty<openapiv3::StringFormat> =
                format.map_or(openapiv3::VariantOrUnknownOrEmpty::Empty, Format::as_format);
            let openapi_type = match self {
                Primitive::String => openapiv3::Type::String(openapiv3::StringType {
                    format: openapi_format,
                    ..Default::default()
                }),
                Primitive::Number => openapiv3::Type::Number(openapiv3::NumberType::default()),
                Primitive::Integer => openapiv3::Type::Integer(openapiv3::IntegerType::default()),
                Primitive::Boolean => openapiv3::Type::Boolean {},
            };
            return openapiv3::SchemaKind::Type(openapi_type);
        }
    }

    impl Format {
        fn as_format(self) -> openapiv3::VariantOrUnknownOrEmpty<openapiv3::StringFormat> {
            match self {
                Format::Date => {
                    openapiv3::VariantOrUnknownOrEmpty::Item(openapiv3::StringFormat::Date)
                }
                Format::DateTime => todo!(),
                Format::Password => todo!(),
                Format::Byte => todo!(),
                Format::Binary => todo!(),
                Format::Custom(_) => todo!(),
            }
        }
    }

    impl Into<openapiv3::SchemaKind> for &Primitive {
        fn into(self) -> openapiv3::SchemaKind {
            let openapi_type = match self {
                Primitive::String => openapiv3::Type::String(openapiv3::StringType::default()),
                Primitive::Number => openapiv3::Type::Number(openapiv3::NumberType::default()),
                Primitive::Integer => openapiv3::Type::Integer(openapiv3::IntegerType::default()),
                Primitive::Boolean => openapiv3::Type::Boolean {},
            };
            return openapiv3::SchemaKind::Type(openapi_type);
        }
    }
}
