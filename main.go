package main

import (
	"fmt"
	"os"

	"github.com/alecthomas/kong"
	"github.com/alecthomas/repr"
	"gopkg.in/yaml.v3"

	"github.com/alecthomas/participle/v2"
	"github.com/alecthomas/participle/v2/lexer"

	"github.com/getkin/kin-openapi/openapi3"
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
	Array    *TypeRef `("[" @@ "]"`
	Type     string   `| @Ident )`
	Nullable bool     `( @"?" )?`
	Format   string   `("#" @Ident)?`
}

var (
	opanApiLexer = lexer.MustSimple([]lexer.SimpleRule{
		{Name: "Comment", Pattern: `(//)[^\n]*\n?`},
		{Name: "Ident", Pattern: `[a-zA-Z]\w*`},
		{Name: "Number", Pattern: `(?:\d*\.)?\d+`},
		{Name: "Punct", Pattern: `[-[!@#$%^&*()+_={}\|:;"'<,>.?/]|]`},
		{Name: "Whitespace", Pattern: `[ \t\n\r]+`},
	})
	parser = participle.MustBuild[OpanApi](
		participle.Lexer(opanApiLexer),
		participle.Elide("Comment", "Whitespace"),
		participle.UseLookahead(2),
	)
)

var cli struct {
	EBNF  bool     `help"Dump EBNF."`
	Files []string `arg:"" optional:"" type:"existingfile" help:"OpanApi files to parse."`
}

func main() {
	ctx := kong.Parse(&cli)
	if cli.EBNF {
		fmt.Println(parser.String())
		ctx.Exit(0)
	}
	for _, file := range cli.Files {
		fileHandle, err := os.Open(file)
		ctx.FatalIfErrorf(err)
		defer fileHandle.Close()
		ast, err := parser.Parse(file, fileHandle)
		repr.Println(ast)
		ctx.FatalIfErrorf(err)
		openapi := ast.Compile()
		repr.Println(openapi)
		bytes, err := yaml.Marshal(openapi)
		ctx.FatalIfErrorf(err)
		err = os.WriteFile("api.yaml", bytes, 0644)
		ctx.FatalIfErrorf(err)
	}
}

func (ast *OpanApi) Compile() openapi3.T { // TODO Opend-API?
	schemas := createSchemas(ast.Schemas)
	return openapi3.T{
		OpenAPI: "missing",
		Info: &openapi3.Info{
			Title:   "missing",
			Version: "0.0.1",
		},
		Paths: openapi3.Paths{},
		Components: &openapi3.Components{
			Schemas: schemas,
		},
	}
}

func createSchemas(a_schemas []*Schema) openapi3.Schemas {
	result := make(map[string]*openapi3.SchemaRef, len(a_schemas))

	for _, schema := range a_schemas {

		switch _type := schema.Type.Type; _type {
		case "string":
			openapiSchema := openapi3.NewSchema()
			openapiSchema.Type = _type
			if schema.Type.Format != "" {
				openapiSchema.Format = schema.Type.Format
			}

			ref := openapi3.NewSchemaRef(schema.Identifier, openapiSchema)
			result[schema.Identifier] = ref
		}
	}
	return result
}


