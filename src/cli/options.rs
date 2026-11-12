use argh::FromArgs;
use tinygit::TinygitError;

use crate::cli::commands::{
    add::AddCommand, commit::CommitCommand, hash_object::HashObjectCommand,
    index_pack::IndexPackCommand, log::LogCommand, ls_buckets::LsBucketsCommand,
    ls_objects::LsObjectsCommand, parse_object::ParseObjectCommand, parse_pack::ParsePackCommand,
    unpack_objects::UnpackObjectsCommand,
};

/// The stupid implementation of the stupid content tracker.
#[derive(FromArgs, PartialEq, Debug)]
pub struct Args {
    #[argh(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum Subcommand {
    HashObject(HashObjectCommand),
    LsBuckets(LsBucketsCommand),
    LsObjects(LsObjectsCommand),
    ParseObject(ParseObjectCommand),
    ParsePack(ParsePackCommand),
    UnpackObjects(UnpackObjectsCommand),
    Log(LogCommand),
    IndexPack(IndexPackCommand),
    AddCommand(AddCommand),
    CommitCommand(CommitCommand),
}

impl Args {
    pub fn run(self) -> Result<(), TinygitError> {
        match self.subcommand {
            Subcommand::HashObject(cmd) => cmd.run(),
            Subcommand::LsBuckets(cmd) => cmd.run(),
            Subcommand::LsObjects(cmd) => cmd.run(),
            Subcommand::ParseObject(cmd) => cmd.run(),
            Subcommand::ParsePack(cmd) => cmd.run(),
            Subcommand::UnpackObjects(cmd) => cmd.run(),
            Subcommand::Log(cmd) => cmd.run(),
            Subcommand::IndexPack(cmd) => cmd.run(),
            Subcommand::AddCommand(cmd) => cmd.run(),
            Subcommand::CommitCommand(cmd) => cmd.run(),
        }
    }
}
