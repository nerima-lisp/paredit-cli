use std::path::Path;

use anyhow::Result;

use super::super::shared::{read_input_and_dialect, write_file_with_rollback};
use super::args::RemoveDefinitionArgs;
use super::render::print_remove_definition_plan;
use crate::application::usecase::remove_definition::{
    DefinitionSourcePort, LoadedDefinitionSource, RemoveDefinitionRequest,
    remove_definition as execute_remove_definition,
};
use crate::presentation::cli::DialectArg;

pub(in crate::presentation::cli) fn remove_definition(args: RemoveDefinitionArgs) -> Result<()> {
    let output = args.output;
    let request = RemoveDefinitionRequest {
        file: args.file,
        path: args.path,
        write: args.write,
    };
    let mut source = CliDefinitionSource {
        dialect: args.dialect,
    };
    let plan = execute_remove_definition(&mut source, request)?;
    print_remove_definition_plan(&plan, output)
}

struct CliDefinitionSource {
    dialect: Option<DialectArg>,
}

impl DefinitionSourcePort for CliDefinitionSource {
    fn load(&mut self, file: &Path) -> Result<LoadedDefinitionSource> {
        let (input, dialect) =
            read_input_and_dialect(Some(file.to_path_buf()), self.dialect.take())?;
        Ok(LoadedDefinitionSource {
            text: input.text,
            dialect,
        })
    }

    fn write(&mut self, file: &Path, content: &str) -> Result<()> {
        write_file_with_rollback(file.to_path_buf(), content.to_owned())
    }
}
