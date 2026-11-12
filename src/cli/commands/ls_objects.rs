use std::env;

use argh::FromArgs;
use bstr::ByteSlice;
use itertools::Either;
use strum::IntoDiscriminant;
use tinygit::{Repo, TinygitError, object::hash::HashPrefix, storage::object::ObjectsBucket};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "ls-objects")]
/// List objects in the repository
pub struct LsObjectsCommand {
    /// the buckets to read objets from.
    #[argh(positional)]
    buckets: Vec<HashPrefix>,

    /// whether to make the output pretty and include more information
    #[argh(switch)]
    pretty: bool,

    /// whether to make the output pretty and include more information (second version)
    #[argh(switch)]
    pretty_2: bool,
}

impl LsObjectsCommand {
    pub fn run(self) -> Result<(), TinygitError> {
        let Self {
            buckets,
            pretty,
            pretty_2,
        } = self;
        let repo = Repo::open_closest(env::current_dir()?)?;

        let objects = if buckets.is_empty() {
            Either::Left(repo.store.objects()?)
        } else {
            Either::Right(
                buckets
                    .into_iter()
                    .filter_map(|prefix| repo.store.bucket(prefix).ok())
                    .map(ObjectsBucket::objects)
                    .filter_map(Result::ok)
                    .flatten(),
            )
        };

        match (pretty, pretty_2) {
            (false, false) => {
                // default output, just print object hashes
                for object in objects {
                    println!("{}", object?.hash()?);
                }
            }
            (true, _) => {
                for lazy_object in objects.filter_map(Result::ok) {
                    let object = lazy_object.into_object()?;

                    println!("{} {:?}", lazy_object.hash()?, object.discriminant());
                }
            }
            (_, true) => {
                // pretty output, print object types
                for lazy_object in objects.filter_map(Result::ok) {
                    let object = lazy_object.into_object()?;
                    let hash = lazy_object.hash()?;

                    match object {
                        tinygit::object::Object::Blob(blob) => {
                            println!("{}  blob    {} bytes", hash, blob.0.len());
                        }
                        tinygit::object::Object::Tree(tree) => {
                            println!("{}  tree    {} entries", hash, tree.entries.len());
                        }
                        tinygit::object::Object::Commit(commit) => {
                            println!(
                                "{}  commit  {:?}",
                                hash,
                                commit.description.lines().next().unwrap_or_default()
                            );
                        }
                        tinygit::object::Object::Tag(tag) => {
                            println!("{}  tag  {}", hash, tag.name);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
