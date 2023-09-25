package transformer

import (
	"lenny99/opan_api/ast"

	"github.com/getkin/kin-openapi/openapi3"
)

func Compile(ast *ast.OpanApi) openapi3.T { // TODO Opend-API?
	schemas := createSchemas(ast.Schemas)
	return openapi3.T{
		OpenAPI: "{NAME}",
		Info: &openapi3.Info{
			Title:   "{TITLE}",
			Version: "{VERSION}",
		},
		Paths: openapi3.Paths{},
		Components: &openapi3.Components{
			Schemas: schemas,
		},
	}
}

func createSchemas(a_schemas []*ast.Schema) openapi3.Schemas {
	result := make(map[string]*openapi3.SchemaRef, len(a_schemas))

	for _, schema := range a_schemas {

		openapiSchema := openapi3.NewSchema()
		openapiSchema.Type = schema.Type.Type
		if schema.Type.Format != "" {
			openapiSchema.Format = schema.Type.Format
		}

		ref := openapi3.NewSchemaRef(schema.Identifier, openapiSchema)
		result[schema.Identifier] = ref
	}
	return result
}
