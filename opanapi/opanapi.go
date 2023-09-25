package opanapi

import (
	participle "github.com/alecthomas/participle/v2/"
	"github.com/alecthomas/participle/v2/lexer"
)

type OpanApi struct {
	Schemas []*Schema `@@*`
}

type Schema struct {
	Identifier string   `"schema" @Ident`
	Type       *TypeRef `"=" ( @@ |`
	Fields     []*Field `"{" @@* "}")`
}

type Field struct {
	Key  string   `@Ident`
	Type *TypeRef `":" @@`
}

type TypeRef struct {
	Array    *TypeRef   `("[" @@ "]"`
	Type     SchemaType `| @Ident )`
	Nullable bool       `( @"?" )?`
	Format   string     `("#" @Ident)?`
}

type SchemaType = string

const (
	SchemaString = "string"
	SchemaNumber = "number"
	SchemaBool   = "boolean"
)

var (
	opanApiLexer = lexer.MustSimple([]lexer.SimpleRule{
		{Name: "Comment", Pattern: `(//)[^\n]*\n?`},
		{Name: "Ident", Pattern: `[a-zA-Z]\w*`},
		{Name: "Number", Pattern: `(?:\d*\.)?\d+`},
		{Name: "Punct", Pattern: `[-[!@#$%^&*()+_={}\|:;"'<,>.?/]|]`},
		{Name: "Whitespace", Pattern: `[ \t\n\r]+`},
	})
	Parser = participle.MustBuild[OpanApi](
		participle.Lexer(opanApiLexer),
		participle.Elide("Comment", "Whitespace"),
		participle.UseLookahead(2),
	)
)
