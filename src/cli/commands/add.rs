use argh::FromArgs;
use tinygit::TinygitError;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "add")]
/// Add file contents to the index
pub struct AddCommand {}

impl AddCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        Ok(())
    }
}
