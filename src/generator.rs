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

fn format_or_else(basic: &Basic) -> openapiv3::VariantOrUnknownOrEmpty<openapiv3::StringFormat> {
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
