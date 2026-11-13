use std::{
    fs::{self},
    io::{self, Read},
    path::PathBuf,
};

use argh::FromArgs;
use tinygit::{
    TinygitError,
    ende::{Decode, Encode},
    storage::object::loose::CompressedLooseObject,
};
use tinygit::error::ParserContext;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "parse-object")]
/// Reads an object from a file or stdin and parses it
pub struct ParseObjectCommand {
    /// read the object from the standard input
    #[argh(switch)]
    stdin: bool,

    /// write the output in binary loose object format
    #[argh(switch, short = 'b')]
    binary: bool,

    #[argh(positional)]
    files: Vec<String>,
}

impl ParseObjectCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let Self {
            stdin,
            files,
            binary,
        } = self;

        // if stdin, deal with this first
        let stdin_data: Option<Vec<u8>> = if stdin {
            let stdin = io::stdin();
            let mut buf = Vec::new();

            let mut lock = stdin.lock();

            lock.read_to_end(&mut buf)?;

            Some(buf)
        } else {
            None
        };

        let objects = stdin_data
            .map(|data| (data, ParserContext::None))
            .map(Ok)
            .into_iter()
            .chain(
                files
                    .into_iter()
                    .map(PathBuf::from)
                    .map(|path| fs::read(&path).map(|bytes| (bytes, ParserContext::File(path)))),
            )
            .map(|contents| match contents {
                Ok((bytes, context)) => {
                    CompressedLooseObject::decode_bytes_with_context(bytes.as_slice(), context)
                }
                Err(err) => Err(TinygitError::from(err)),
            });

        for object in objects {
            match object {
                Ok(object) => match binary {
                    true => {
                        let mut stdout = io::stdout();

                        object.encode(&mut stdout)?;
                    }
                    false => println!("{object:#?}"),
                },
                Err(err) => eprintln!("{err}"),
            }
        }

        Ok(())
    }
}
