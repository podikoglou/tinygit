use std::{
    collections::{BinaryHeap, HashSet},
    env,
    io::{self, IsTerminal, Write},
    process::Command,
};

use argh::FromArgs;
use bstr::ByteSlice;
use itertools::Either;
use owo_colors::{OwoColorize as _, Stream};
use tinygit::{
    Repo, TinygitError,
    object::{commit::Commit, hash::ObjectHash},
};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "log")]
/// Show commit logs
pub struct LogCommand {}

/// Newtype over [Commit] used in the context of the `log` command's priority queue.
///
/// Comparison between [QueueEntry] values depends solely on the commit time.
///
/// NOTE: This could perhaps be moved somewhere else later or we may be able to achieve the same
/// thing differently.
struct QueueEntry(Commit, ObjectHash);

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.committer.1 == other.0.committer.1
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.committer.1.cmp(&other.0.committer.1)
    }
}

impl From<(Commit, ObjectHash)> for QueueEntry {
    fn from(val: (Commit, ObjectHash)) -> Self {
        QueueEntry(val.0, val.1)
    }
}

impl LogCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let stdout = io::stdout();
        let mut out = if io::stdout().is_terminal() {
            let pipe = io::pipe()?;
            let child = Command::new("less")
                .stdin(pipe.0)
                .env("LESS", "FRX") // this makes less colorful, I am *not* sure why.
                .spawn()?;

            Either::Left((pipe.1, child))
        } else {
            Either::Right(stdout.lock())
        };

        owo_colors::set_override(out.is_left());

        let w: &mut dyn Write = match &mut out {
            Either::Left((w, _)) => w,
            Either::Right(w) => w,
        };

        let repo = Repo::open_closest(env::current_dir()?)?;

        let mut queue: BinaryHeap<QueueEntry> = BinaryHeap::default();
        let mut visited: HashSet<ObjectHash> = HashSet::default();

        let head = repo.store.head()?;

        queue.push(head.into());

        while !queue.is_empty() {
            // we just ensured it's not empty, so we can unwrap.
            let QueueEntry(commit, hash) = queue.pop().unwrap();

            for parent_hash in &commit.parents {
                if visited.contains(parent_hash) {
                    continue;
                }

                let parent = repo.store.object(parent_hash)?;
                let parent = *parent.try_as_commit().ok_or(TinygitError::UnexpectedType)?;

                queue.push((parent, parent_hash.clone()).into());
                visited.insert(parent_hash.clone());
            }

            if let Err(err) = write_commit(w, &commit, &hash) {
                if err.kind() == io::ErrorKind::BrokenPipe {
                    break;
                }
                return Err(err.into());
            }
        }

        if let Either::Left((writer, mut child)) = out {
            drop(writer);
            child.wait()?;
        }

        Ok(())
    }
}

fn write_commit(
    w: &mut (impl Write + ?Sized),
    Commit {
        author,
        description,
        ..
    }: &Commit,
    hash: &ObjectHash,
) -> io::Result<()> {
    writeln!(
        w,
        "{}",
        format!("commit {hash}").if_supports_color(Stream::Stdout, |x| x.yellow())
    )?;

    writeln!(w, "Author: {}", author.0)?;
    writeln!(w, "Date:   {}", author.1.format("%a %b %d %T %Y %z"))?;
    writeln!(w)?;

    for line in description.lines() {
        writeln!(w, "    {}", line.as_bstr())?;
    }

    writeln!(w)?;

    Ok(())
}
