use anyhow::Result;
use serde_yaml;
use std::fs;

extern crate pest;
#[macro_use]
extern crate pest_derive;

fn main() -> Result<()> {
    let input = fs::read_to_string("./examples/booking.api")?;
    let api_script = parser::parse(&input)?;
    let open_api = generator::generate(&api_script);
    let open_api_str = serde_yaml::to_string(&open_api)?;
    println!("{}", open_api_str);
    return Ok(());
}

mod parser {
    use anyhow::Result;
    use itertools::{Either, Itertools};
    use pest::{
        iterators::{Pair, Pairs},
        Parser,
    };
    use std::{rc::Rc, vec};

    pub fn parse(input: &str) -> Result<ApiScript> {
        let mut parse_tree = ApiScript::parse(Rule::ApiScript, input)?;
        let root = parse_tree.next().unwrap();
        let api_script = ApiScript::from(root)?;
        return Ok(api_script);
    }

    #[derive(Debug)]
    struct ParseError {
        message: String,
    }

    impl std::error::Error for ParseError {}
    impl std::fmt::Display for ParseError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.message)
        }
    }

    type Node<'a> = Pair<'a, Rule>;
    type Nodes<'a> = Pairs<'a, Rule>;

    #[derive(Parser, Debug)]
    #[grammar = "grammar.pest"]
    pub struct ApiScript {
        pub schemas: Vec<Schema>,
    }

    impl ApiScript {
        fn from(tree: Node) -> Result<Self> {
            let mut api_script = ApiScript { schemas: vec![] };
            for pair in tree.into_inner() {
                match pair.as_rule() {
                    Rule::Schema => api_script.schemas.push(api_script.schema(pair)?),
                    _ => (),
                }
            }
            return Ok(api_script);
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
            let schema = self
                .schemas
                .iter()
                .find(|schema| schema.identificator == node.as_str());
            if let Some(schema) = schema {
                return Ok(Either::Left(schema));
            }
            let primitive = ApiScript::primitive(node)?;
            return Ok(Either::Right(primitive));
        }

        fn fields(&self, nodes: Nodes) -> Result<Vec<Field>> {
            let mut fields = Vec::<Field>::new();
            for node in nodes {
                for (property, kind) in node.into_inner().tuples() {
                    fields.push(Field {
                        name: property.as_str().into(),
                        definition: self.type_def(kind)?,
                    })
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
                _ => Err(ParseError {
                    message: format!("{} is not a primitive", primitive.as_str()),
                }),
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
        Reference {
            name: String,
        },
    }

    #[derive(Debug, Clone)]
    pub struct Field {
        pub name: String,
        pub definition: SchemaDefinition,
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
}

mod generator {
    use crate::parser::{ApiScript, Field, Format, Primitive, Schema, SchemaDefinition};
    use indexmap::IndexMap;
    use openapiv3::ReferenceOr;

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
                SchemaDefinition::Object { fields } => openapiv3::Schema {
                    schema_data: openapiv3::SchemaData {
                        ..Default::default()
                    },
                    schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::Object(
                        openapiv3::ObjectType {
                            properties: into_properties(fields),
                            ..Default::default()
                        },
                    )),
                },
                _ => todo!(),
            };
            return (self.identificator.to_owned(), schema);
        }
    }

    fn into_properties(
        fields: &Vec<Field>,
    ) -> IndexMap<String, openapiv3::ReferenceOr<Box<openapiv3::Schema>>> {
        let mut map = IndexMap::new();
        for field in fields {
            let kind: ReferenceOrSchemaKind = (&field.definition).into();
            match kind {
                ReferenceOr::Item(kind) => {
                    let schema = openapiv3::Schema {
                        schema_data: Default::default(),
                        schema_kind: kind,
                    };
                    map.insert(field.name.clone(), ReferenceOr::Item(Box::new(schema)));
                }
                ReferenceOr::Reference { reference } => {
                    map.insert(field.name.clone(), ReferenceOr::Reference { reference });
                }
            }
        }
        return map;
    }

    type ReferenceOrSchemaKind = openapiv3::ReferenceOr<openapiv3::SchemaKind>;

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
