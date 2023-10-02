

// struct ApiScript {
//     pub schemas: Vec<Schema>,
// }

// impl ApiScript {
//     fn new() -> Self {
//         return Self {
//             schemas: Vec::new(),
//         };
//     }
// }

// struct Schema {
//     identifier: String,
//     schema_type: TypeDef,
//     format: Option<String>,
// }

// enum TypeDef {
//     Reference(TypeReference),
//     Array(TypeReference),
//     Object(Object),
// }

// type Object = HashMap<String, TypeReference>;

// type TypeReference = String;

// #[derive(Parser)]
// #[grammar = "grammar.pest"]
// struct ApiScriptParser;

// type Node<'i> = pest_consume::Node<'i, Rule, ()>;
// type Result<T> = std::result::Result<T, Error<Rule>>;

// #[parser]
// impl ApiScriptParser {
//     fn ApiScript(node: Node) -> Result<ApiScript> {
//         let mut apiScript = ApiScript::new();
//         for child in node.children() {
//             match child.as_rule() {
//                 Rule::Schema => apiScript.schemas.push(ApiScriptParser::Schema(child)?),
//                 _ => (),
//             }
//         }
//         return Ok(apiScript);
//     }

//     fn Schema(node: Node) -> Result<Schema> {
//         match_nodes!(expr.children();
//             [Rule::IDENT(ident), Rule::Fields(fields)] => ApiScriptParser::ObjectSchema(ident, fields),
//             [Rule::IDENT(ident), Rule::TypeDef(def)] => ApiScriptParser::TypeDefSchema(ident, def)
//         )
//     }

//     fn ObjectSchema(ident: String, Rule::Field: Field) -> Result<Schema> {
//         unimplemented!()
//     }

//     fn TypeDef(node: Node) -> Result<TypeDef> {
//         unimplemented!()
//     }

//     fn EOI(node: Node) {
//         println!("EOF")
//     }
// }
