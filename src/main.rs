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
                    let format = inners
                        .next()
                        .map(|p| p.as_str().to_owned())
                        .or(Option::None);
                    SchemaDefinition::NewType { primitive, format }
                }
                Rule::Fields => todo!(),
                _ => unimplemented!(),
            };
            return Ok(schema_definition);
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
            format: Option<String>,
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
}

mod generator {
    use indexmap::IndexMap;
    use openapiv3::{Components, Info, OpenAPI, Paths, Server};

    use crate::parser::ApiScript;

    pub(crate) fn generate(api_script: &ApiScript) -> OpenAPI {
        return api_script.generate();
    }

    trait Generate<T> {
        fn generate(&self) -> T;
    }

    impl Generate<OpenAPI> for ApiScript {
        fn generate(&self) -> OpenAPI {
            return OpenAPI {
                components: Some(Components {
                    schemas: IndexMap::new(),
                    ..Default::default()
                }),
                openapi: String::from("3.1.0"),
                info: Info {
                    ..Default::default()
                },
                servers: vec![],
                paths: Paths {
                    ..Default::default()
                },
                security: None,
                tags: vec![],
                external_docs: None,
                extensions: IndexMap::new(),
            };
        }
    }
}
