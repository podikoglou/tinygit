use std::env;

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
        let tree_entries = index_entries.into_iter().filter_map(|entry| {
            let key = TreeKey::new(
                FileName::decode_bytes(entry.path_name.as_slice()).ok()?,
                entry.mode.permission(),
            );

            let entry = TreeEntry::new(entry.hash.clone());

            Some((key, entry))
        });

        let new_tree = Tree::new(tree_entries.collect());

        // figure out where to place commit
        let head = match repo.store.head() {
            Ok(head) => Some(head),
            Err(TinygitError::RefNotFound) => None,
            Err(err) => return Err(err),
        };

        // create tree object
        let final_tree: Tree = match &head {
            None => new_tree,
            Some((head_commit, _)) => {
                // get HEAD tree and merge the new tree into it
                let head_tree = repo
                    .store
                    .object(&head_commit.tree)?
                    .try_as_tree()
                    .ok_or(TinygitError::UnexpectedObjectType)?;

                let mut final_tree = head_tree.clone();

                for entry in new_tree.entries {
                    // TODO: this is quite slow! how do we address this?
                    final_tree.entries.retain(|key, _| key.name != entry.0.name);

                    final_tree.entries.insert(entry.0, entry.1);
                }

                final_tree
            }
        };
        let final_tree_object = Object::Tree(final_tree);
        let final_tree_raw = RawObject::try_from(final_tree_object)?;

        // can we remove this clone? perhaps we can have it returned from encode_object?
        let final_tree_hash = final_tree_raw.clone().hash()?;

        match repo.store.encode_object(&final_tree_raw) {
            Err(TinygitError::ObjectExists) => {}
            other => other?,
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
            tree: final_tree_hash,
            parents: head.map(|(_, hash)| vec![hash]).unwrap_or_default(),
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
