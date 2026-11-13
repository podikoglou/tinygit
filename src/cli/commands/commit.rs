use std::env;

use argh::FromArgs;
use chrono::{Local, Utc};
use tinygit::{
    Repo, TinygitError,
    identity::{Email, Identity, Name},
    time::Timestamp,
};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "commit")]
/// Record changes to the repository
pub struct CommitCommand {
    /// description of the commit
    #[argh(option, short = 'm')]
    message: String,
}

impl CommitCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let CommitCommand { message } = self;

        let repo = Repo::open_closest(env::current_dir()?)?;

        // figure out identity
        // TODO: actual identity management
        let identity = Identity::new(
            Name::try_new("Unknown".into())?,
            Email::try_new("your.email@soon.com".into())?,
        );

        // figure out timestamp
        let local_time = Local::now();
        let local_offset = local_time.offset();
        let utc_time = Utc::now().with_timezone(local_offset);

        let timestamp = Timestamp::try_new_rounded_up(utc_time)?;

        match repo.store.commit(
            message,
            (identity.clone(), timestamp.clone()),
            (identity, timestamp),
        ) {
            Err(TinygitError::EmptyTree) => {
                println!("nothing to commit");

                // TODO: error code
                Ok(())
            }
            result => result.map(|_| ()),
        }
    }
}
