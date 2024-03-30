use anyhow::Result;
use clap::Parser;
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
    if output.is_file() || !output.exists() {
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
    println!("{:#?}", api_script);
    let apis = generator::generate(&api_script);
    return Ok(apis);
}

mod cli {
    use clap::{Parser, Subcommand};
    use std::path::PathBuf;

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
}

mod generator {
    use crate::parser::{
        Api, ApiScript, Basic, Constraint, Definition, Endpoint, Field, Format, HttpMethod,
        ParameterType, Path, Primitive, Schema,
    };
    use anyhow::bail;
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
                parameters: self.openapi_parameters(),
                responses: self.openapi_responses(),
                ..Default::default()
            };
        }

        fn openapi_parameters(&self) -> Vec<ReferenceOr<openapiv3::Parameter>> {
            let mut result = vec![];

            for parameter in self.parameters() {
                let parameter = match parameter.parameter_type() {
                    ParameterType::Query => openapiv3::Parameter::Query {
                        parameter_data: openapiv3::ParameterData {
                            name: parameter.name().to_string(),
                            required: false,
                            format: parameter.kind().into(),
                            example: None,
                            examples: indexmap! {},
                            explode: None,
                            extensions: indexmap! {},
                            description: None,
                            deprecated: None,
                        },
                        allow_reserved: false,
                        style: openapiv3::QueryStyle::Form,
                        allow_empty_value: None,
                    },
                    ParameterType::Path => openapiv3::Parameter::Path {
                        parameter_data: openapiv3::ParameterData {
                            name: parameter.name().to_string(),
                            description: None,
                            required: true,
                            deprecated: None,
                            format: parameter.kind().into(),
                            example: None,
                            examples: indexmap! {},
                            explode: None,
                            extensions: indexmap! {},
                        },
                        style: openapiv3::PathStyle::Simple,
                    },
                    ParameterType::Header => todo!(),
                    ParameterType::Cookie => todo!(),
                };
                result.push(ReferenceOr::Item(parameter));
            }

            return result;
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
            map: &IndexMap<MediaTypeBuf, Definition>,
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

    impl Into<openapiv3::ParameterSchemaOrContent> for &Definition {
        fn into(self) -> openapiv3::ParameterSchemaOrContent {
            let schema = openapiv3::ReferenceOr::Item(self.into());
            return openapiv3::ParameterSchemaOrContent::Schema(schema);
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
            return (self.identifier.to_owned(), schema);
        }
    }

    type ReferenceOrSchemaKind = openapiv3::ReferenceOr<openapiv3::SchemaKind>;

    impl Into<openapiv3::Schema> for &Definition {
        fn into(self) -> openapiv3::Schema {
            match &self {
                Definition::Primitive(basic) => {
                    return openapiv3::Schema {
                        schema_kind: basic.into(),
                        schema_data: openapiv3::SchemaData::default(),
                    };
                }
                Definition::Object { fields } => {
                    return openapiv3::Schema {
                        schema_data: openapiv3::SchemaData {
                            ..Default::default()
                        },
                        schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::Object(
                            openapiv3::ObjectType {
                                properties: Definition::properties(fields),
                                required: Definition::required_properties(fields),
                                ..Default::default()
                            },
                        )),
                    }
                }
                _ => todo!(),
            };
        }
    }

    impl Definition {
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

        fn required_properties(fields: &Vec<Field>) -> Vec<String> {
            fields
                .iter()
                .filter(|f| *f.required())
                .map(|f| f.name().clone())
                .collect()
        }
    }

    impl Into<ReferenceOrSchemaKind> for &Definition {
        fn into(self) -> ReferenceOrSchemaKind {
            match self {
                Definition::Primitive(basic) => ReferenceOrSchemaKind::Item(basic.into()),
                Definition::Reference { name } => {
                    let path = format!("#/components/schemas/{}", name);
                    return ReferenceOrSchemaKind::Reference { reference: path };
                }
                Definition::Array { schema } => {
                    if let Definition::Reference { name } = schema.as_ref() {
                        let path = format!("#/components/schemas/{name}");
                        let reference =
                            ReferenceOr::<Box<openapiv3::Schema>>::Reference { reference: path };
                        let array_type = openapiv3::ArrayType {
                            items: Option::Some(reference),
                            max_items: None,
                            min_items: None,
                            unique_items: false,
                        };
                        let schema_kind = openapiv3::Type::Array(array_type);
                        let schema_type = openapiv3::SchemaKind::Type(schema_kind);
                        return ReferenceOrSchemaKind::Item(schema_type);
                    }
                    return ReferenceOrSchemaKind::Reference {
                        reference: String::from("FOO"),
                    };
                }
                _ => todo!(),
            }
        }
    }

    impl Into<ReferenceOr<openapiv3::Schema>> for &Definition {
        fn into(self) -> ReferenceOr<openapiv3::Schema> {
            if let Definition::Reference { name } = self {
                return ReferenceOr::Reference {
                    reference: format!("#/components/schemas/{name}"),
                };
            }

            let schema = self.into();
            return ReferenceOr::Item(schema);
        }
    }

    impl Into<openapiv3::SchemaKind> for &Basic {
        fn into(self) -> openapiv3::SchemaKind {
            let mut kind = match self.privitive() {
                Primitive::String => openapiv3::Type::String(openapiv3::StringType {
                    format: format_or_else(self),
                    ..Default::default()
                }),
                Primitive::Number => openapiv3::Type::Number(openapiv3::NumberType::default()),
                Primitive::Integer => openapiv3::Type::Integer(openapiv3::IntegerType::default()),
                Primitive::Boolean => openapiv3::Type::Boolean {},
            };

            apply_constraint(&mut kind, self.constraints().as_slice());

            return openapiv3::SchemaKind::Type(kind);
        }
    }

    fn format_or_else(
        basic: &Basic,
    ) -> openapiv3::VariantOrUnknownOrEmpty<openapiv3::StringFormat> {
        if let Some(format) = basic.format() {
            return openapiv3::VariantOrUnknownOrEmpty::Item(format.into());
        }
        return openapiv3::VariantOrUnknownOrEmpty::Empty;
    }

    fn apply_constraint(kind: &mut openapiv3::Type, constraints: &[Constraint]) {
        for constraint in constraints {
            match constraint {
                Constraint::Maximum(max) => set_max(kind, *max),
                _ => (),
            }
        }
    }

    fn set_max(kind: &mut openapiv3::Type, max: usize) {
        match kind {
            openapiv3::Type::String(str) => str.max_length = Some(max),
            openapiv3::Type::Number(num) => num.maximum = Some(max as f64),
            openapiv3::Type::Integer(int) => int.maximum = Some(max as i64),
            _ => (),
        }
    }

    impl Into<openapiv3::StringFormat> for &Format {
        fn into(self) -> openapiv3::StringFormat {
            match self {
                Format::Date => openapiv3::StringFormat::Date,
                Format::DateTime => openapiv3::StringFormat::DateTime,
                Format::Password => openapiv3::StringFormat::Password,
                Format::Byte => openapiv3::StringFormat::Byte,
                Format::Binary => openapiv3::StringFormat::Binary,
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
