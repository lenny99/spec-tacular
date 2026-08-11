use indexmap::IndexMap;
use mediatype::MediaTypeBuf;

use crate::util::ReferenceOr;
use std::rc::Rc;

#[derive(Debug)]
pub struct Document {
    pub schemas: Vec<Rc<Schema>>,
    pub apis: Vec<API>,
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub identifier: String,
    pub definition: ReferenceOr<Schema, Definition>,
}

impl Schema {
    pub(crate) fn new(identifier: String, definition: ReferenceOr<Schema, Definition>) -> Schema {
        return Schema {
            identifier,
            definition,
        };
    }
}

#[derive(Debug, Clone)]
pub enum Definition {
    Primitive(Primitive),
    Array(Rc<Schema>),
    Object(Vec<Field>),
}

impl Definition {
    pub fn constrained_by(self: &Self, constraints: &Vec<Constraint>) -> Self {
        match self {
            Definition::Primitive(basic) => {
                Definition::Primitive(basic.constrained_by(constraints))
            }
            Definition::Array(_element) => todo!(),
            Definition::Object(_fields) => todo!(),
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Primitive {
    kind: Kind,
    format: Option<Format>,
    constraints: Vec<Constraint>,
}

impl Primitive {
    pub(crate) fn new(kind: Kind, format: Option<Format>, constraints: Vec<Constraint>) -> Self {
        Self {
            kind,
            format,
            constraints,
        }
    }

    fn constrained_by(self: &Self, constraints: &Vec<Constraint>) -> Self {
        let mut cloned = self.clone();
        cloned.constraints.clone_from(constraints);
        return cloned;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Constraint {
    Maximum(usize),
    Minimum(usize),
}

#[derive(Debug, Clone, Getters)]
pub struct Field {
    name: String,
    definition: ReferenceOr<Schema, Definition>,
    required: bool,
}

impl Field {
    pub fn new(name: String, definition: ReferenceOr<Schema, Definition>, required: bool) -> Self {
        Field {
            name,
            definition,
            required,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Kind {
    String,
    Number,
    Integer,
    Boolean,
}

#[derive(Debug, Clone)]
pub enum StringFormat {
    Date,
    DateTime,
    Password,
    Byte,
    Binary,
    Email,
    Uuid,
    Uri,
    Hostname,
    Ipv4,
    Ipv6,
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum IntegerFormat {
    Int32,
    Int64,
}

#[derive(Debug, Clone)]
pub enum NumberFormat {
    Float,
    Double,
}

#[derive(Debug, Clone)]
pub enum Format {
    String(StringFormat),
    Integer(IntegerFormat),
    Number(NumberFormat),
}

#[derive(Debug, Clone, Getters)]
pub struct API {
    name: String,
    version: String,
    paths: IndexMap<String, Path>,
}
impl API {
    pub(crate) fn new(name: String, version: String, paths: IndexMap<String, Path>) -> Self {
        Self {
            name,
            version,
            paths,
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Path {
    endpoints: IndexMap<HttpMethod, Endpoint>,
}

impl Path {
    pub fn new(endpoints: IndexMap<HttpMethod, Endpoint>) -> Self {
        Path { endpoints }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Endpoint {
    operation_id: String,
    parameters: Vec<Parameter>,
    responses: IndexMap<u16, Responses>,
}

impl Endpoint {
    pub(crate) fn new(
        operation_id: String,
        parameters: Vec<Parameter>,
        responses: IndexMap<u16, IndexMap<MediaTypeBuf, ReferenceOr<Schema, Definition>>>,
    ) -> Self {
        Self {
            operation_id,
            parameters,
            responses,
        }
    }
}

#[derive(Debug, Clone, Getters)]
pub struct Parameter {
    name: String,
    kind: ReferenceOr<Schema, Definition>,
    location: ParameterLocation,
    constraints: Vec<Constraint>,
}

impl Parameter {
    pub fn new<S>(
        identifier: S,
        kind: ReferenceOr<Schema, Definition>,
        annotations: ParameterAnnotations,
    ) -> Parameter
    where
        S: AsRef<str>,
    {
        Parameter {
            name: identifier.as_ref().to_string(),
            location: annotations.parameter_type,
            constraints: annotations.constraints,
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParameterLocation {
    Query,
    Path,
    Header,
    Cookie,
}

pub type Responses = IndexMap<MediaTypeBuf, ReferenceOr<Schema, Definition>>;
pub type HttpCode = u16;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Patch,
    Delete,
}

#[derive(Debug, Clone)]
pub struct ParameterAnnotations {
    parameter_type: ParameterLocation,
    constraints: Vec<Constraint>,
}

impl ParameterAnnotations {
    pub(crate) fn new(location: ParameterLocation, constraints: Vec<Constraint>) -> Self {
        Self {
            parameter_type: location,
            constraints: constraints,
        }
    }
}

#[cfg(test)]
pub mod testing {
    use std::thread::scope;

    use super::*;

    pub enum DefinitionBuilder {
        Primitive {
            kind: Kind,
            format: Option<Format>,
            constraints: Vec<Constraint>,
        },
    }

    impl DefinitionBuilder {
        fn kind(kind: Kind) -> Self {
            Self::Primitive {
                kind,
                format: None,
                constraints: Vec::default(),
            }
        }
    }

    pub fn number() -> DefinitionBuilder {
        return DefinitionBuilder::kind(Kind::Number);
    }

    pub fn string() -> DefinitionBuilder {
        return DefinitionBuilder::kind(Kind::String);
    }

    pub fn bool() -> DefinitionBuilder {
        return DefinitionBuilder::kind(Kind::Boolean);
    }

    pub fn integer() -> DefinitionBuilder {
        return DefinitionBuilder::kind(Kind::Integer);
    }

    impl DefinitionBuilder {
        pub fn max(mut self, max: usize) -> Self {
            if let DefinitionBuilder::Primitive { constraints, .. } = &mut self {
                constraints.push(Constraint::Maximum(max));
            }
            self
        }

        pub fn format<F>(mut self, representation: F) -> Self
        where
            F: Into<Format>,
        {
            if let DefinitionBuilder::Primitive { format, .. } = &mut self {
                *format = Some(representation.into());
            }
            self
        }

        pub fn custom(mut self, name: &str) -> Self {
            if let DefinitionBuilder::Primitive { format, .. } = &mut self {
                *format = Some(Format::String(StringFormat::Custom(name.to_string())));
            }
            self
        }
    }

    impl From<&str> for Format {
        fn from(representation: &str) -> Self {
            match representation {
                "int32" => Format::Integer(IntegerFormat::Int32),
                "int64" => Format::Integer(IntegerFormat::Int64),
                "float" => Format::Number(NumberFormat::Float),
                "double" => Format::Number(NumberFormat::Double),
                "date" => Format::String(StringFormat::Date),
                "date-time" => Format::String(StringFormat::DateTime),
                "password" => Format::String(StringFormat::Password),
                "byte" => Format::String(StringFormat::Byte),
                "binary" => Format::String(StringFormat::Binary),
                "email" => Format::String(StringFormat::Email),
                "uuid" => Format::String(StringFormat::Uuid),
                "uri" => Format::String(StringFormat::Uri),
                "hostname" => Format::String(StringFormat::Hostname),
                "ipv4" => Format::String(StringFormat::Ipv4),
                "ipv6" => Format::String(StringFormat::Ipv6),
                other => Format::String(StringFormat::Custom(other.to_string())),
            }
        }
    }

    impl Into<Definition> for DefinitionBuilder {
        fn into(self) -> Definition {
            match self {
                DefinitionBuilder::Primitive {
                    kind,
                    format,
                    constraints,
                } => Definition::Primitive(Primitive {
                    kind,
                    format,
                    constraints,
                }),
            }
        }
    }

    #[derive(Default)]
    pub struct DocumentBuilder {
        schemas: Vec<Rc<Schema>>,
        apis: Vec<API>,
    }

    impl<'builder> DocumentBuilder {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn build(self) -> Document {
            Document {
                schemas: self.schemas,
                apis: self.apis,
            }
        }

        pub fn schema(&'builder mut self, name: &str) -> SchemaBuilder {
            SchemaBuilder::new(name)
        }

        pub fn kind(&'builder mut self, name: &str) -> KindBuilder<'builder> {
            KindBuilder::new(self, name)
        }

        fn register_definition(&mut self, name: String, defnition: Definition) {
            let schema = Schema::new(name, ReferenceOr::Actual(defnition));
            self.schemas.push(Rc::new(schema));
        }

        pub fn api(&mut self, name: &str, version: &str) {
            self.apis.push(API::new(
                name.to_owned(),
                version.to_owned(),
                IndexMap::default(),
            ));
        }

        pub fn register_schema(&mut self, warehouse: SchemaBuilder) {
            let schema = warehouse.build();
            self.schemas.push(Rc::new(schema));
        }
    }

    pub struct KindBuilder<'builder> {
        document: &'builder mut DocumentBuilder,
        pub name: String,
        pub kind: Option<Definition>,
    }

    impl<'builder> KindBuilder<'builder> {
        pub fn new(document: &'builder mut DocumentBuilder, name: &str) -> Self {
            Self {
                name: name.to_owned(),
                document: document,
                kind: None,
            }
        }

        pub fn definition<D: Into<Definition>>(mut self, definition: D) -> Self {
            self.kind = Some(definition.into());
            return self;
        }

        pub fn register(self) {
            let Self {
                document,
                name,
                kind,
            } = self;
            let definition =
                kind.expect("Could not register empty definition. Please define the type.");
            document.register_definition(name, definition);
        }
    }

    pub struct SchemaBuilder {
        name: Option<String>,
        object: ObjectBuilder,
    }

    impl SchemaBuilder {
        pub fn new(name: &str) -> Self {
            Self {
                name: Some(name.to_owned()),
                object: ObjectBuilder::new(),
            }
        }

        pub fn build(self) -> Schema {
            Schema {
                identifier: self.name.expect("schema name empty. Please provide a name"),
                definition: ReferenceOr::Actual(self.object.build()),
            }
        }

        pub fn field<ConfigFn>(&mut self, name: &str, configure: ConfigFn)
        where
            ConfigFn: FnOnce(FieldBuilder) -> FieldBuilder,
        {
            self.object.field(name, configure);
        }
    }

    pub struct FieldBuilder {
        name: String,
        kind: Option<ReferenceOr<Schema, Definition>>,
        required: bool,
    }

    impl FieldBuilder {
        pub fn new(name: &str) -> Self {
            return Self {
                name: name.to_owned(),
                kind: None,
                required: false,
            };
        }

        fn build(self) -> Field {
            let definition = self
                .kind
                .expect("Could not build field. Finish the definition.");
            return Field {
                name: self.name,
                definition: definition,
                required: self.required,
            };
        }

        pub fn refer(mut self, schema: Rc<Schema>) -> FieldBuilder {
            self.kind = Some(ReferenceOr::Reference(schema));
            self
        }

        pub fn is<D: Into<Definition>>(mut self, definition: D) -> FieldBuilder {
            self.kind = Some(ReferenceOr::Actual(definition.into()));
            self
        }

        pub fn required(mut self, required: bool) -> FieldBuilder {
            self.required = required;
            self
        }

        pub(crate) fn schema<ConfigFn>(mut self, configure: ConfigFn) -> FieldBuilder
        where
            ConfigFn: FnOnce(&mut ObjectBuilder),
        {
            let mut sb = ObjectBuilder::new();
            configure(&mut sb);
            self.kind = Some(ReferenceOr::Actual(sb.build()));
            self
        }
    }

    pub struct ObjectBuilder {
        fields: IndexMap<String, Field>,
    }

    impl ObjectBuilder {
        pub fn new() -> Self {
            Self {
                fields: IndexMap::new(),
            }
        }

        pub fn build(self) -> Definition {
            let fields = self.fields.values().cloned().collect::<Vec<_>>();
            return Definition::Object(fields);
        }

        pub fn field<ConfigFn>(&mut self, name: &str, configure: ConfigFn)
        where
            ConfigFn: FnOnce(FieldBuilder) -> FieldBuilder,
        {
            let mut fb = FieldBuilder::new(name);
            fb = configure(fb);
            let built = fb.build();
            let name = built.name.clone();
            self.fields.insert(name, built);
        }
    }
}
