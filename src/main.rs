use anyhow::Result;
use clap::Parser;
use openapiv3::OpenAPI;
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

extern crate pest;
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate derive_getters;

mod generator;
mod parser;
mod util;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    match args.command {
        cli::Commands::Compile { file, output } => {
            let apis = compile(file.as_path())?;
            if let Some(output) = output {
                match apis.len().cmp(&2) {
                    Ordering::Less | Ordering::Equal => write_to_file(&output, &apis)?,
                    Ordering::Greater => write_to_files(apis, output)?,
                }
            } else {
                let content = combine_apis(&apis)?;
                println!("{content}");
            }
        }
    }
    return Ok(());
}

fn write_to_files(apis: Vec<OpenAPI>, output: std::path::PathBuf) -> Result<(), anyhow::Error> {
    for api in apis {
        let ending = format!("/{}", api.info.title);
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

fn compile(path: &std::path::Path) -> Result<Vec<OpenAPI>> {
    let input = fs::read_to_string(path)?;
    let api_script = crate::parser::parse(&input)?;
    let apis = crate::generator::generate(&api_script);
    return Ok(apis);
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
