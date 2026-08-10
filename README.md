# spec-tacular

A small DSL for writing OpenAPI specs by hand, compiled to YAML.

Specs are written in `.api` files using a schema/path-centric syntax and compiled
into OpenAPI 3 documents.

## Usage

Build the compiler:

```sh
cargo build --release
```

Compile an example, printing the resulting YAML to stdout:

```sh
cargo run --release -- compile examples/petstore.api
```

Write the output to a file instead:

```sh
cargo run --release -- compile examples/petstore.api -o petstore.yaml
```

Try the other bundled example with `examples/booking.api`.

For the full CLI options:

```sh
cargo run --release -- compile --help
```
