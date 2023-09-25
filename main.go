package main

import (
	"fmt"
	"lenny99/opan_api/opanapi"
	"os"

	"github.com/alecthomas/kong"
	"github.com/alecthomas/repr"

	"gopkg.in/yaml.v3"
)

var cli struct {
	EBNF  bool     `help"Dump EBNF."`
	Files []string `arg:"" optional:"" type:"existingfile" help:"OpanApi files to parse."`
}

func main() {
	ctx := kong.Parse(&cli)
	if cli.EBNF {
		fmt.Println(opanapi.Parser.String())
		ctx.Exit(0)
	}
	for _, file := range cli.Files {
		fileHandle, err := os.Open(file)
		ctx.FatalIfErrorf(err)
		defer fileHandle.Close()
		ast, err := opanapi.Parser.Parse(file, fileHandle)
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
