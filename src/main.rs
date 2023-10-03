use anyhow::Result;
use serde_yaml;
use std::fs;

extern crate pest;
#[macro_use]
extern crate pest_derive;

fn main() -> Result<()> {
    let input = fs::read_to_string("./examples/booking.api")?;
    let api_script = parser::parse(&input)?;
    dbg!(&api_script);
    let open_api = generator::generate(&api_script);
    let open_api_str = serde_yaml::to_string(&open_api)?;
    println!("{}", open_api_str);
    return Ok(());
}

mod parser {
    use std::rc::Rc;

    use anyhow::Result;
    use pest::{iterators::Pair, Parser};

    pub fn parse(input: &str) -> Result<ApiScript> {
        let mut parse_tree = ApiScript::parse(Rule::ApiScript, input)?;
        let root = parse_tree.next().unwrap();
        let ast = ApiScript::api_script(root)?;
        return Ok(ast);
    }

    type Node<'a> = Pair<'a, Rule>;

    #[derive(Parser, Debug)]
    #[grammar = "grammar.pest"]
    pub struct ApiScript {
        pub schemas: Vec<Schema>,
    }

    impl ApiScript {
        fn api_script(api_script: Node) -> Result<ApiScript> {
            let mut schemas: Vec<Schema> = vec![];

            for pair in api_script.into_inner() {
                match pair.as_rule() {
                    Rule::Schema => schemas.push(ApiScript::schema(pair)?),
                    _ => (),
                }
            }

            return Ok(ApiScript { schemas });
        }

        fn schema(schema: Node) -> Result<Schema> {
            let mut inners = schema.into_inner().into_iter();
            let identificator = inners.next().unwrap().as_str();
            let schema_definition = ApiScript::schema_definition(inners.next().unwrap())?;
            return Ok(Schema {
                identificator: identificator.to_owned(),
                schema_definition,
            });
        }

        fn schema_definition(schema_definition: Node) -> Result<SchemaDefinition> {
            let inner = schema_definition.into_inner().next().unwrap();
            let schema_definition = match inner.as_rule() {
                Rule::TypeDef => {
                    let mut inners = inner.into_inner().into_iter();
                    let primitive = ApiScript::primitive(inners.next().unwrap());
                    let format = inners.next().map(ApiScript::format);
                    SchemaDefinition::NewType { primitive, format }
                }
                Rule::Fields => todo!(),
                _ => unimplemented!(),
            };
            return Ok(schema_definition);
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

        fn primitive(primitive: Node) -> Primitive {
            match primitive.as_str() {
                "string" => Primitive::String,
                "number" => Primitive::Number,
                "integer" => Primitive::Integer,
                "boolean" => Primitive::Boolean,
                _ => unreachable!(),
            }
        }
    }

    #[derive(Debug)]
    pub struct Schema {
        pub identificator: String,
        pub schema_definition: SchemaDefinition,
    }

    #[derive(Debug)]
    pub enum SchemaDefinition {
        NewType {
            primitive: Primitive,
            format: Option<Format>,
        },
        Array {
            schemaRef: Rc<Schema>,
        },
        Object {
            fields: Vec<Field>,
        },
    }

    #[derive(Debug)]
    pub struct Field {
        pub name: String,
        pub definition: SchemaDefinition,
    }

    #[derive(Debug)]
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
}

mod generator {
    use crate::parser::{ApiScript, Format, Primitive, Schema, SchemaDefinition};
    use indexmap::IndexMap;
    use openapiv3;

    pub(crate) fn generate(api_script: &ApiScript) -> openapiv3::OpenAPI {
        return api_script.generate();
    }

    trait Generate<T> {
        fn generate(&self) -> T;
    }

    impl Generate<openapiv3::OpenAPI> for ApiScript {
        fn generate(&self) -> openapiv3::OpenAPI {
            return openapiv3::OpenAPI {
                components: Some(openapiv3::Components {
                    schemas: self.schemas.iter().map(Schema::generate).fold(
                        IndexMap::new(),
                        |mut map, (identificator, schema)| {
                            map.insert(identificator, openapiv3::ReferenceOr::Item(schema));
                            return map;
                        },
                    ),
                    ..Default::default()
                }),
                openapi: String::from("3.1.0"),
                info: openapiv3::Info {
                    ..Default::default()
                },
                servers: vec![],
                paths: openapiv3::Paths {
                    ..Default::default()
                },
                security: None,
                tags: vec![],
                external_docs: None,
                extensions: IndexMap::new(),
            };
        }
    }

    impl Schema {
        fn generate(&self) -> (String, openapiv3::Schema) {
            let schema = match &self.schema_definition {
                SchemaDefinition::NewType { primitive, format } => openapiv3::Schema {
                    schema_kind: primitive.as_schema_type(format.clone()), // ?
                    schema_data: openapiv3::SchemaData::default(),
                },
                _ => todo!(),
            };
            return (self.identificator.to_owned(), schema);
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
