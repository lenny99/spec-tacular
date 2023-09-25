package transformer

import (
	"lenny99/opan_api/ast"
	"testing"

	"github.com/alecthomas/assert/v2"
)

func TestCreateSchema(t *testing.T) {
	schemas := []*ast.Schema{
		{
			Identifier: "Int64",
			Type: &ast.TypeRef{
				Type:   ast.SchemaNumber,
				Format: "int64",
			},
		},
		{
			Identifier: "Date",
			Type: &ast.TypeRef{
				Type:   ast.SchemaBool,
				Format: "Date",
			},
		},
	}

	openApiSchemas := createSchemas(schemas)

	intSchema := openApiSchemas["Int64"]
	assert.Equal(t, intSchema.Ref, "Int64")
	assert.Equal(t, intSchema.Value.Type, "number")
	assert.Equal(t, intSchema.Value.Format, "int64")

	dateSchema := openApiSchemas["Date"]
	assert.Equal(t, dateSchema.Ref, "Date")
	assert.Equal(t, dateSchema.Value.Type, Bool)
	assert.Equal(t, dateSchema.Value.Format, "Date")
}
