package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestCreateSchema(t *testing.T) {
	schemas := []*Schema{
		{
			Identifier: "Int64", Type: &TypeRef{
				Type:   "string",
				Format: "date",
			},
		},
	}
	openApiSchemas := createSchemas(schemas)
	intSchema := openApiSchemas["Int64"]
	assert.Equal(t, intSchema.Ref, "Int64")
	assert.Equal(t, intSchema.Value.Type, "string")
	assert.Equal(t, intSchema.Value.Format, "date")
}
