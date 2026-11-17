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

        let mut walker = Self {
            db,
            pending: Vec::default(),
        };

        walker.push_children("", tree);

        Ok(walker)
    }

    /// Pushes a tree's entries in reverse order, so `Vec::pop` returns them in
    /// Git's canonical order.
    fn push_children(&mut self, prefix: &str, tree: Tree) {
        for (key, entry) in tree.entries.into_iter().rev() {
            let path = format!("{}{}", prefix, key.as_str());

            self.pending.push(WalkEntry { path, key, entry });
        }
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

            self.push_children(&prefix, tree);
        }

        Ok(Some(entry))
    }
}

/// Reads and parses a tree object from the object database.
fn read_tree(db: &dyn ObjectDb, hash: &ObjectHash) -> Result<Tree, TinygitError> {
    db.raw_object(hash)?
        .into_object(ParserContext::None)?
        .try_as_tree()
        .ok_or(TinygitError::UnexpectedObjectType)
}
