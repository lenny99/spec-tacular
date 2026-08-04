use crate::{
    ast::{self, Path, API},
    util::ReferenceOr,
};
use indexmap::{indexmap, IndexMap};
use mediatype::MediaTypeBuf;
use openapiv3::{
    Components, Operation, ParameterData, ParameterSchemaOrContent, PathItem, QueryStyle,
    SchemaData, StatusCode,
};

pub(crate) fn generate(doc: &ast::Document) -> Vec<openapiv3::OpenAPI> {
    let mut apis: Vec<openapiv3::OpenAPI> = vec![];
    for api in doc.apis.iter() {
        let open_api = generate_api(doc, api);
        apis.push(open_api);
    }
    return apis;
}

fn generate_schemas(
    doc: &ast::Document,
) -> IndexMap<String, openapiv3::ReferenceOr<openapiv3::Schema>> {
    let mut map: IndexMap<String, ReferenceOrSchema> = indexmap! {};
    for schema in &doc.schemas {
        let (identifier, ref_or_schema): (String, ReferenceOrSchema) = schema.as_ref().into();
        map.insert(identifier, ref_or_schema);
    }
    return map;
}

type ReferenceOrSchema = openapiv3::ReferenceOr<openapiv3::Schema>;

impl Into<(String, ReferenceOrSchema)> for &ast::Schema {
    fn into(self) -> (String, ReferenceOrSchema) {
        return (self.identifier.to_owned(), (&self.definition).into());
    }
}

impl Into<ReferenceOrSchema> for &ReferenceOr<ast::Schema, ast::Definition> {
    fn into(self) -> ReferenceOrSchema {
        match self {
            ReferenceOr::Reference(reference) => {
                let name = &reference.as_ref().identifier;
                let path = format!("#/components/schemas/{name}");
                return ReferenceOrSchema::Reference { reference: path };
            }
            ReferenceOr::Actual(definition) => openapiv3::ReferenceOr::Item(definition.into()),
        }
    }
}

fn generate_api(doc: &ast::Document, api: &ast::API) -> openapiv3::OpenAPI {
    return openapiv3::OpenAPI {
        paths: generate_api_paths(api),
        components: Option::Some(Components {
            schemas: generate_schemas(doc),
            ..Default::default()
        }),
        ..Default::default()
    };
}

fn generate_api_paths(api: &API) -> openapiv3::Paths {
    let mut paths: IndexMap<String, openapiv3::ReferenceOr<PathItem>> = indexmap!();
    for (url, path) in api.paths().iter() {
        let path = generate_path(path);
        paths.insert(url.into(), openapiv3::ReferenceOr::Item(path));
    }

    return openapiv3::Paths {
        paths,
        extensions: indexmap!(),
    };
}

fn generate_path(path: &ast::Path) -> openapiv3::PathItem {
    let get = path
        .endpoints()
        .get(&ast::HttpMethod::Get)
        .map(generate_endpoint);

    return PathItem {
        get,
        ..Default::default()
    };
}

fn generate_endpoint(endpoint: &ast::Endpoint) -> Operation {
    return Operation {
        operation_id: Option::Some(endpoint.operation_id().into()),
        parameters: openapi_parameters(endpoint),
        responses: openapi_responses(endpoint),
        ..Default::default()
    };
}

fn openapi_parameters(
    endpoint: &ast::Endpoint,
) -> Vec<openapiv3::ReferenceOr<openapiv3::Parameter>> {
    let mut result = vec![];

    for parameter in endpoint.parameters() {
        let name = parameter.name();
        let kind = parameter.kind();
        let parameter = match parameter.location() {
            ast::ParameterLocation::Query => query(parameter_data(name, kind.into())),
            ast::ParameterLocation::Path => openapiv3::Parameter::Path {
                parameter_data: openapiv3::ParameterData {
                    required: true,
                    ..parameter_data(parameter.name(), parameter.kind().into())
                },
                style: openapiv3::PathStyle::Simple,
            },
            ast::ParameterLocation::Header => todo!(),
            ast::ParameterLocation::Cookie => todo!(),
        };
        result.push(openapiv3::ReferenceOr::Item(parameter));
    }

    return result;
}

fn openapi_responses(endpoint: &ast::Endpoint) -> openapiv3::Responses {
    let mut responses = indexmap!();
    for (http_code, media_responses) in endpoint.responses() {
        let response = openapiv3::Response {
            content: to_content(media_responses),
            ..Default::default()
        };
        responses.insert(
            StatusCode::Code(*http_code),
            openapiv3::ReferenceOr::Item(response),
        );
    }

    return openapiv3::Responses {
        responses,
        ..Default::default()
    };
}

fn to_content(
    map: &IndexMap<MediaTypeBuf, ReferenceOr<ast::Schema, ast::Definition>>,
) -> IndexMap<String, openapiv3::MediaType> {
    let mut result = indexmap!();
    for (media_type, definition) in map {
        let schema = definition.into();
        result.insert(
            media_type.to_string(),
            openapiv3::MediaType {
                schema: Option::Some(schema),
                ..Default::default()
            },
        );
    }
    return result;
}

impl Into<openapiv3::ParameterSchemaOrContent> for &ReferenceOr<ast::Schema, ast::Definition> {
    fn into(self) -> openapiv3::ParameterSchemaOrContent {
        let schema: ReferenceOrSchema = self.into();
        return openapiv3::ParameterSchemaOrContent::Schema(schema);
    }
}

impl Into<openapiv3::Responses> for ast::Endpoint {
    fn into(self) -> openapiv3::Responses {
        let mut responses = indexmap!();
        for (http_code, media_responses) in self.responses() {
            let response = openapiv3::Response {
                content: to_content(media_responses),
                ..Default::default()
            };
            responses.insert(
                StatusCode::Code(*http_code),
                openapiv3::ReferenceOr::Item(response),
            );
        }

        return openapiv3::Responses {
            responses,
            ..Default::default()
        };
    }
}

impl Into<openapiv3::Schema> for ast::Definition {
    fn into(self) -> openapiv3::Schema {
        return (&self).into();
    }
}

impl Into<openapiv3::Schema> for &ast::Definition {
    fn into(self) -> openapiv3::Schema {
        match &self {
            ast::Definition::Primitive(basic) => openapiv3::Schema {
                schema_kind: basic.into(),
                schema_data: openapiv3::SchemaData::default(),
            },
            ast::Definition::Object(fields) => openapiv3::Schema {
                schema_data: openapiv3::SchemaData {
                    ..Default::default()
                },
                schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::Object(
                    openapiv3::ObjectType {
                        properties: ast::Definition::properties(fields),
                        required: ast::Definition::required_properties(fields),
                        ..Default::default()
                    },
                )),
            },
            ast::Definition::Array(array) => {
                let path = format!("#/components/schemas/{}", array.identifier);
                let reference = BoxedSchemaReference::Reference { reference: path };
                let array_type = openapiv3::ArrayType {
                    items: Option::Some(reference),
                    max_items: None,
                    min_items: None,
                    unique_items: false,
                };
                let schema_kind = openapiv3::Type::Array(array_type);
                openapiv3::Schema {
                    schema_kind: openapiv3::SchemaKind::Type(schema_kind),
                    schema_data: SchemaData::default(),
                }
            }
        }
    }
}

impl ast::Definition {
    fn properties(
        fields: &Vec<ast::Field>,
    ) -> IndexMap<String, openapiv3::ReferenceOr<Box<openapiv3::Schema>>> {
        let mut map = IndexMap::new();
        for field in fields {
            let definition = field.definition();
            match definition {
                ReferenceOr::Reference(_reference) => {
                    //let kind = BoxedSchemaReference::Reference(reference);
                    //map.insert(field.name().to_owned(), kind);
                }
                ReferenceOr::Actual(definition) => {
                    let schema: openapiv3::Schema = definition.into();
                    map.insert(
                        field.name().to_owned(),
                        BoxedSchemaReference::Item(Box::new(schema)),
                    );
                }
            }
        }
        return map;
    }

    fn required_properties(fields: &Vec<ast::Field>) -> Vec<String> {
        fields
            .iter()
            .filter(|f| *f.required())
            .map(|f| f.name().clone())
            .collect()
    }
}

type BoxedSchemaReference = openapiv3::ReferenceOr<Box<openapiv3::Schema>>;

impl Into<openapiv3::SchemaKind> for &ast::Primitive {
    fn into(self) -> openapiv3::SchemaKind {
        let mut kind = match self.kind() {
            ast::Kind::String => openapiv3::Type::String(openapiv3::StringType {
                format: format_or_else(self),
                ..Default::default()
            }),
            ast::Kind::Number => openapiv3::Type::Number(openapiv3::NumberType::default()),
            ast::Kind::Integer => openapiv3::Type::Integer(openapiv3::IntegerType::default()),
            ast::Kind::Boolean => openapiv3::Type::Boolean {},
        };

        apply_constraint(&mut kind, self.constraints().as_slice());

        return openapiv3::SchemaKind::Type(kind);
    }
}

fn format_or_else(
    basic: &ast::Primitive,
) -> openapiv3::VariantOrUnknownOrEmpty<openapiv3::StringFormat> {
    if let Some(format) = basic.format() {
        return openapiv3::VariantOrUnknownOrEmpty::Item(format.into());
    }
    return openapiv3::VariantOrUnknownOrEmpty::Empty;
}

fn apply_constraint(kind: &mut openapiv3::Type, constraints: &[ast::Constraint]) {
    for constraint in constraints {
        match constraint {
            ast::Constraint::Maximum(max) => set_max(kind, *max),
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

impl Into<openapiv3::StringFormat> for &ast::Format {
    fn into(self) -> openapiv3::StringFormat {
        match self {
            ast::Format::Date => openapiv3::StringFormat::Date,
            ast::Format::DateTime => openapiv3::StringFormat::DateTime,
            ast::Format::Password => openapiv3::StringFormat::Password,
            ast::Format::Byte => openapiv3::StringFormat::Byte,
            ast::Format::Binary => openapiv3::StringFormat::Binary,
            ast::Format::Custom(_) => todo!(),
        }
    }
}

fn query(data: ParameterData) -> openapiv3::Parameter {
    return openapiv3::Parameter::Query {
        parameter_data: data,
        allow_reserved: false,
        style: QueryStyle::Form,
        allow_empty_value: None,
    };
}

fn parameter_data<Str: Into<String>>(
    name: Str,
    format: ParameterSchemaOrContent,
) -> openapiv3::ParameterData {
    return ParameterData {
        name: name.into(),
        format: format,
        example: None,
        examples: indexmap! {},
        explode: None,
        required: false,
        deprecated: None,
        extensions: indexmap! {},
        description: None,
    };
}
