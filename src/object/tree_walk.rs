use fallible_iterator::FallibleIterator;

use crate::{
    TinygitError,
    error::ParserContext,
    object::{
        hash::ObjectHash,
        tree::{Tree, TreeEntry, TreeKey},
    },
    storage::object::ObjectDb,
};

/// One entry produced by a [`TreeWalker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    /// Full path from the walked root, with `/` separators.
    pub path: String,

    pub key: TreeKey,

    pub entry: TreeEntry,
}

/// A [FallibleIterator] over the entries of a tree and its subtrees.
///
/// Entries are yielded in pre-order: a directory comes before its children.
pub struct TreeWalker<'a> {
    db: &'a dyn ObjectDb,
    pending: Vec<WalkEntry>,
}

impl<'a> TreeWalker<'a> {
    /// Starts a walk at the tree `root`.
    pub fn new(db: &'a dyn ObjectDb, root: &ObjectHash) -> Result<Self, TinygitError> {
        let tree = read_tree(db, root)?;
        let mut pending = Vec::new();

        push_children(&mut pending, "", tree);

        Ok(Self { db, pending })
    }
}

impl FallibleIterator for TreeWalker<'_> {
    type Item = WalkEntry;
    type Error = TinygitError;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        let Some(entry) = self.pending.pop() else {
            return Ok(None);
        };

        if entry.key.is_tree() {
            let tree = read_tree(self.db, &entry.entry.object)?;

            let mut prefix = entry.path.clone();
            prefix.push('/');

            push_children(&mut self.pending, &prefix, tree);
        }

        Ok(Some(entry))
    }
}

/// Pushes a tree's entries in reverse order, so `Vec::pop` returns them in
/// Git's canonical order.
fn push_children(pending: &mut Vec<WalkEntry>, prefix: &str, tree: Tree) {
    for (key, entry) in tree.entries.into_iter().rev() {
        let mut path = String::with_capacity(prefix.len() + key.as_str().len());
        path.push_str(prefix);
        path.push_str(key.as_str());

        pending.push(WalkEntry { path, key, entry });
    }
}

/// Reads and parses a tree object from the object database.
fn read_tree(db: &dyn ObjectDb, hash: &ObjectHash) -> Result<Tree, TinygitError> {
    db.raw_object(hash)?
        .into_object(ParserContext::None)?
        .try_as_tree()
        .ok_or(TinygitError::UnexpectedObjectType)
}
