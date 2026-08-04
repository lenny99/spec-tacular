use anyhow::{Error, Result};
use clap::Parser;
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};
use openapiv3::OpenAPI;
use std::fs;
use std::path::Path;
use std::{cmp::Ordering, path::PathBuf};
use util::ParseError;

extern crate pest;
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate derive_getters;

mod ast;
mod error;
mod generator;
mod parser;
mod util;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    match args.command {
        cli::Commands::Compile { file, output } => {
            let src = fs::read_to_string(file)?;
            match compile(&src) {
                Err(error) => handle_error(&src, error)?,
                Ok(spec) => write_output(spec, output)?,
            };
        }
    }
    Ok(())
}

fn compile(src: &str) -> Result<Vec<openapiv3::OpenAPI>> {
    let spec = parser::parse(&src)?;
    return Ok(generator::generate(&spec));
}

fn handle_error(src: &str, error: Error) -> Result<()> {
    if let Some(error) = error.downcast_ref::<ParseError>() {
        handle_parse_error(src, error);
        return Ok(());
    } else {
        return Err(error);
    }
}

fn handle_parse_error(src: &str, error: &ParseError) {
    let mut writer = StandardStream::stderr(ColorChoice::Always);
    let config = codespan_reporting::term::Config::default();

    let mut files = SimpleFiles::new();
    let id = files.add("main", src);
    if let Some(span) = error.span {
        let diag = Diagnostic::error()
            .with_message(&error.message)
            .with_labels(vec![
                Label::primary(id, span.0..span.1).with_message(&error.message)
            ]);
        term::emit(&mut writer, &config, &files, &diag)
            .expect("Can't even print errors, skill issue");
    } else {
        term::emit(
            &mut writer,
            &config,
            &files,
            &Diagnostic::error().with_message(&error.message),
        )
        .expect("Could not even print error");
    }
}

fn write_output(apis: Vec<OpenAPI>, output: Option<PathBuf>) -> Result<(), anyhow::Error> {
    if let Some(output) = output {
        match apis.len().cmp(&2) {
            Ordering::Less | Ordering::Equal => write_to_file(&output, &apis)?,
            Ordering::Greater => write_to_files(apis, output)?,
        }
    } else {
        let content = combine_apis(&apis)?;
        println!("{content}");
    }
    return Ok(());
}

fn write_to_files(apis: Vec<OpenAPI>, output: std::path::PathBuf) -> Result<(), anyhow::Error> {
    for api in apis {
        let ending = format!("{}", api.info.title);
        let path = output.join(Path::new(ending.as_str()));
        std::fs::write(path, serde_yaml::to_string(&api)?)?;
    }
    return Ok(());
}

fn write_to_file(output: &std::path::PathBuf, apis: &Vec<OpenAPI>) -> Result<(), anyhow::Error> {
    if output.is_file() || !output.exists() {
        let content = combine_apis(apis)?;
        std::fs::write(output, content)?;
    }
    return Ok(());
}

fn combine_apis(apis: &Vec<OpenAPI>) -> Result<String> {
    let mut content = String::new();
    for api in apis {
        content += "---\n";
        content += serde_yaml::to_string(api)?.as_str();
    }
    return Ok(content);
}

mod cli {
    use clap::{Parser, Subcommand};
    use std::path::PathBuf;

    #[derive(Parser)]
    #[command(arg_required_else_help(true))]
    pub struct Args {
        #[command(subcommand)]
        pub command: Commands,
    }

    #[derive(Subcommand, Clone)]
    pub enum Commands {
        Compile {
            file: PathBuf,
            #[arg(short, long)]
            output: Option<PathBuf>,
        },
    }
}
