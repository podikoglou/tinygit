use std::{collections::BTreeMap, env};

use argh::FromArgs;
use chrono::{Local, Utc};
use tinygit::{
    Repo, TinygitError,
    ende::Decode,
    fs::FileName,
    identity::{Email, Identity, Name},
    object::{
        Object,
        commit::Commit,
        tree::{Tree, TreeEntry, TreeKey},
    },
    storage::{object::RawObject, reference::RefPointer},
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

        let repo = Repo::open(env::current_dir()?)?;

        // open index
        let index = repo.store.index()?;
        let index_entries = index.entries().collect::<Vec<_>>();

        if index_entries.is_empty() {
            println!("nothing to commit");

            // TODO: error code
            return Ok(());
        }

        // convert index entries to tree entries (TreeKey, TreeEntry)
        let tree_entries = index_entries
            .into_iter()
            .map(|entry| {
                let key = TreeKey::new(
                    FileName::decode_bytes(entry.path_name.as_slice())?,
                    entry.mode.permission(),
                );

                let entry = TreeEntry::new(entry.hash.clone());

                Ok((key, entry))
            })
            .collect::<Result<BTreeMap<_, _>, TinygitError>>()?;

        let tree = Tree::new(tree_entries);

        let tree_object = Object::Tree(tree);
        let tree_raw = RawObject::try_from(tree_object)?;

        // can we remove this clone? perhaps we can have it returned from encode_object?
        let tree_hash = tree_raw.clone().hash()?;

        // create tree object
        match repo.store.encode_object(&tree_raw) {
            Err(TinygitError::ObjectExists) => {}
            other => other?,
        };

        // figure out where to place commit
        let parents = match repo.store.head() {
            Ok(head) => vec![head.1],
            Err(TinygitError::RefNotFound) => vec![],
            Err(err) => return Err(err),
        };

        // figure out identity
        // TODO: actual identity management
        let identity = Identity::new(
            Name::try_new("Unknown".into())?,
            Email::new("your.email@soon.com".into()),
        );

        // figure out timestamp
        let local_time = Local::now();
        let local_offset = local_time.offset();
        let utc_time = Utc::now().with_timezone(local_offset);

        let timestamp = Timestamp::try_new_rounded_up(utc_time)?;

        // create commit
        let commit = Commit {
            tree: tree_hash,
            parents,
            author: (identity.clone(), timestamp.clone()),
            committer: (identity, timestamp),
            extra: vec![],
            description: message.into(),
        };

        let commit_object = Object::Commit(Box::new(commit));
        let commit_object_raw = RawObject::try_from(commit_object)?;
        let commit_object_hash = commit_object_raw.clone().hash()?;

        repo.store.encode_object(&commit_object_raw)?;

        // update HEAD
        match &repo.store.reference(&"HEAD".into())?.pointer {
            RefPointer::Symbolic(r#ref) => repo
                .store
                .set_reference(r#ref, RefPointer::Hash(commit_object_hash))?,

            RefPointer::Hash(_) => repo
                .store
                .set_reference(&"HEAD".into(), RefPointer::Hash(commit_object_hash))?,
        };

        Ok(())
    }
}
