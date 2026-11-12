//! This test suite tests `tinygit` against `libgit2` which is used as an oracle and considered to
//! be a correct implementation.
//!
//! NOTE: Because of the way the `git2` crate works, it often happens that while tests may be
//! passing, if you would run this with `-- --nocapture`, it would show errors. This should be kept
//! in mind when running this suite.

use git2::{
    Commit,
    ObjectType::{self as Git2ObjectType},
    Odb, Oid, ReferenceType, Signature,
};
use include_dir::{Dir, include_dir};
use std::str::FromStr;
use tinygit::{
    ende::Encode,
    error::ParserContext,
    identity::{self, Email, Identity},
    object::{Object, ObjectType as TinygitObjectType, hash::ObjectHash},
    storage::reference::RefPointer,
};

use crate::common::{unpack_git2_repo, unpack_repo};

mod common;

static REPOS_DIR: Dir = include_dir!("tests/fixtures/repos");

fn oid_to_hash(oid: &Oid) -> ObjectHash {
    ObjectHash::from_str(&oid.to_string()).expect("should be able to parse hash")
}

fn signature_to_identity(signature: Signature) -> Identity {
    Identity {
        name: identity::Name::try_new(signature.name().unwrap().into()).unwrap(),
        email: Email::new(signature.email().unwrap().into()),
    }
}

fn git2_object_type_to_object_type(obj_type: Git2ObjectType) -> Option<TinygitObjectType> {
    match obj_type {
        Git2ObjectType::Any => None,
        Git2ObjectType::Commit => Some(TinygitObjectType::Commit),
        Git2ObjectType::Tree => Some(TinygitObjectType::Tree),
        Git2ObjectType::Blob => Some(TinygitObjectType::Blob),
        Git2ObjectType::Tag => Some(TinygitObjectType::Tag),
    }
}

fn collect_oids(odb: &Odb<'_>) -> Vec<Oid> {
    let mut oids = Vec::new();

    odb.foreach(|oid| {
        oids.push(oid.to_owned());
        true
    })
    .expect("should be able to iterate over OIDs");

    oids
}

#[test]
fn raw_objects_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let oracle_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            assert_eq!(
                tinygit_raw_obj.bytes.len(),
                oracle_obj.data().len(),
                "object lengths should be equal"
            );

            assert_eq!(
                tinygit_raw_obj.bytes,
                oracle_obj.data(),
                "raw object data should be equal"
            );

            let oracle_obj_type = oracle_obj.kind();
            let oracle_obj_type = git2_object_type_to_object_type(oracle_obj_type);

            assert_eq!(
                Some(tinygit_raw_obj.r#type),
                oracle_obj_type,
                "object types should match"
            );
        }
    }
}

#[test]
fn commits_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            // tinygit object
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("should be able to parse the bject");

            let Object::Commit(tinygit_commit) = tinygit_obj else {
                continue;
            };

            // oracle object
            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should be able to find the object");

            let Some(oracle_commit) = oracle_obj.as_commit() else {
                continue;
            };

            assert_eq!(
                tinygit_commit.tree,
                oid_to_hash(&oracle_commit.tree_id()),
                "tree matches"
            );

            assert_eq!(
                tinygit_commit.parents,
                oracle_commit
                    .parent_ids()
                    .map(|oid| oid_to_hash(&oid))
                    .collect::<Vec<_>>(),
                "parents match"
            );

            assert_eq!(
                tinygit_commit.author.0,
                signature_to_identity(oracle_commit.author()),
                "authors match"
            );

            assert_eq!(
                tinygit_commit.committer.0,
                signature_to_identity(oracle_commit.committer()),
                "commiters match"
            );

            assert_eq!(
                tinygit_commit.author.1.timestamp(),
                oracle_commit.time().seconds(),
                "commit times match"
            );

            assert_eq!(
                tinygit_commit.description,
                oracle_commit.message().unwrap(),
                "message matches"
            );

            // TODO: test for headers
        }
    }
}

#[test]
fn trees_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            // tinygit object
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("should be able to parse the bject");

            let Object::Tree(tinygit_tree) = tinygit_obj else {
                continue;
            };

            // oracle object
            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should be able to find the object");

            let Some(oracle_tree) = oracle_obj.as_tree() else {
                continue;
            };

            assert_eq!(
                tinygit_tree.entries.len(),
                oracle_tree.len(),
                "tree lengths match"
            );

            for (oracle_entry, tinygit_entry) in oracle_tree.iter().zip(tinygit_tree.entries.iter())
            {
                assert_eq!(
                    tinygit_entry.0.as_str(),
                    oracle_entry.name().unwrap(),
                    "file names match"
                );

                assert_eq!(
                    tinygit_entry.0.mode as i32,
                    oracle_entry.filemode(),
                    "filemode mathces"
                );

                assert_eq!(
                    tinygit_entry.1.object,
                    oid_to_hash(&oracle_entry.id()),
                    "entry object hashes match"
                );
            }
        }
    }
}

#[test]
fn blobs_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            // tinygit object
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("should be able to parse the bject");

            let Object::Blob(tinygit_blob) = tinygit_obj else {
                continue;
            };

            // oracle object
            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should be able to find the object");

            let Some(oracle_blob) = oracle_obj.as_blob() else {
                continue;
            };

            assert_eq!(tinygit_blob.0.len(), oracle_blob.size(), "sizes match");
            assert_eq!(tinygit_blob.0, oracle_blob.content(), "contents match");
        }
    }
}

#[test]
fn tags_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            // tinygit object
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("should be able to parse the bject");

            let Object::Tag(tinygit_tag) = tinygit_obj else {
                continue;
            };

            // oracle object
            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should be able to find the object");

            let Some(oracle_tag) = oracle_obj.as_tag() else {
                continue;
            };

            assert_eq!(
                tinygit_tag.name.as_str(),
                oracle_tag.name().unwrap(),
                "names match"
            );

            assert_eq!(
                tinygit_tag.description.as_str(),
                oracle_tag.message().unwrap().unwrap_or_default(),
                "descriptions match"
            );

            assert_eq!(
                tinygit_tag.tagger.map(|x| x.0),
                oracle_tag.tagger().map(signature_to_identity),
                "taggers match"
            );
        }
    }
}

#[test]
fn commits_reserialization_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("tinygit should parse the object");

            let Object::Commit(tinygit_commit) = tinygit_obj else {
                continue;
            };

            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should find the object");

            let Some(oracle_commit) = oracle_obj.as_commit() else {
                continue;
            };

            let parents: Vec<_> = oracle_commit.parents().collect();
            let parent_refs: Vec<&Commit> = parents.iter().collect();
            let tree = oracle_commit.tree().expect("commit tree");
            let libgit2_buf = oracle_repo
                .commit_create_buffer(
                    &oracle_commit.author(),
                    &oracle_commit.committer(),
                    oracle_commit.message_raw().unwrap(),
                    &tree,
                    &parent_refs,
                )
                .expect("libgit2 should serialize commit");

            let mut tinygit_bytes = Vec::new();
            tinygit_commit
                .encode(&mut tinygit_bytes)
                .expect("tinygit should serialize commit");

            assert_eq!(
                tinygit_bytes.as_slice(),
                &libgit2_buf[..],
                "serialized commit bytes match"
            );
        }
    }
}

#[test]
fn trees_reserialization_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("tinygit should parse the object");

            let Object::Tree(tinygit_tree) = tinygit_obj else {
                continue;
            };

            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should find the object");

            let Some(oracle_tree) = oracle_obj.as_tree() else {
                continue;
            };

            let builder = oracle_repo
                .treebuilder(Some(oracle_tree))
                .expect("treebuilder");
            let new_tree_oid = builder.write().expect("treebuilder write");
            let libgit2_tree_obj = oracle_odb.read(new_tree_oid).expect("read tree");

            let mut tinygit_bytes = Vec::new();
            tinygit_tree
                .encode(&mut tinygit_bytes)
                .expect("tinygit should serialize tree");

            assert_eq!(
                tinygit_bytes,
                libgit2_tree_obj.data(),
                "serialized tree bytes match"
            );
        }
    }
}

#[test]
fn blobs_reserialization_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("tinygit should parse the object");

            let Object::Blob(tinygit_blob) = tinygit_obj else {
                continue;
            };

            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should find the object");

            let Some(oracle_blob) = oracle_obj.as_blob() else {
                continue;
            };

            let new_blob_oid = oracle_repo.blob(oracle_blob.content()).expect("write blob");
            let libgit2_blob_obj = oracle_odb.read(new_blob_oid).expect("read blob");

            let mut tinygit_bytes = Vec::new();
            tinygit_blob
                .encode(&mut tinygit_bytes)
                .expect("tinygit should serialize blob");

            assert_eq!(
                tinygit_bytes,
                libgit2_blob_obj.data(),
                "serialized blob bytes match"
            );
        }
    }
}

#[test]
fn tags_reserialization_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let oracle_odb = oracle_repo.odb().expect("should be able to open odb");
        let tinygit_store = tinygit_repo.store;

        for oid in collect_oids(&oracle_odb) {
            let hash = oid_to_hash(&oid);

            let tinygit_raw_obj = tinygit_store
                .raw_object(&hash)
                .expect("tinygit should load the object");

            let tinygit_obj = tinygit_raw_obj
                .into_object(ParserContext::None)
                .expect("tinygit should parse the object");

            let Object::Tag(tinygit_tag) = tinygit_obj else {
                continue;
            };

            let oracle_odb_obj = oracle_odb
                .read(oid)
                .expect("libgit2 should load the object");

            let oracle_obj = oracle_repo
                .find_object(oid, Some(oracle_odb_obj.kind()))
                .expect("libgit2 should find the object");

            let Some(oracle_tag) = oracle_obj.as_tag() else {
                continue;
            };

            let target = oracle_tag.target().expect("tag target");
            let tagger = oracle_tag.tagger().expect("tagger");
            let new_tag_oid = oracle_repo
                .tag_annotation_create(
                    oracle_tag.name().unwrap(),
                    &target,
                    &tagger,
                    oracle_tag.message().unwrap().unwrap_or_default(),
                )
                .expect("tag_annotation_create");
            let libgit2_tag_obj = oracle_odb.read(new_tag_oid).expect("read tag");

            let mut tinygit_bytes = Vec::new();
            tinygit_tag
                .encode(&mut tinygit_bytes)
                .expect("tinygit should serialize tag");

            assert_eq!(
                tinygit_bytes,
                libgit2_tag_obj.data(),
                "serialized tag bytes match"
            );
        }
    }
}

#[test]
fn refs_match() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let tinygit_store = tinygit_repo.store;
        let oracle_refs = oracle_repo
            .references()
            .expect("libgit2 should list references");

        for oracle_ref in oracle_refs {
            let oracle_ref = oracle_ref.expect("valid reference");
            let ref_name = oracle_ref.name().expect("utf-8 reference name");

            let tinygit_ref = tinygit_store
                .reference(&ref_name.to_string())
                .expect("tinygit should load the reference");

            assert_eq!(tinygit_ref.name, ref_name, "reference names match");

            match oracle_ref.kind() {
                Some(ReferenceType::Direct) => {
                    let oracle_target = oracle_ref.target().expect("direct ref should have target");
                    assert_eq!(
                        tinygit_ref.pointer,
                        RefPointer::Hash(oid_to_hash(&oracle_target)),
                        "direct ref targets match"
                    );
                }
                Some(ReferenceType::Symbolic) => {
                    let oracle_target = oracle_ref
                        .symbolic_target()
                        .expect("valid symbolic target")
                        .expect("symbolic ref should have target");
                    assert_eq!(
                        tinygit_ref.pointer,
                        RefPointer::Symbolic(oracle_target.to_string()),
                        "symbolic ref targets match"
                    );
                }
                None => unreachable!(),
            }

            let oracle_resolved = oracle_ref
                .resolve()
                .expect("libgit2 should resolve reference");
            let oracle_resolved_oid = oracle_resolved
                .target()
                .expect("resolved ref should have target");

            let tinygit_resolved_hash = tinygit_store
                .resolve_reference(&tinygit_ref.pointer)
                .expect("tinygit should resolve reference");

            assert_eq!(
                tinygit_resolved_hash,
                oid_to_hash(&oracle_resolved_oid),
                "resolved reference hashes match"
            );
        }
    }
}

#[test]
fn head_matches() {
    for repo_dir in REPOS_DIR.files() {
        let (tinygit_repo, _dir) = unpack_repo(repo_dir.contents());
        let (oracle_repo, _dir) = unpack_git2_repo(repo_dir.contents());

        let tinygit_store = tinygit_repo.store;

        let (tinygit_commit, tinygit_head_hash) =
            tinygit_store.head().expect("tinygit should resolve HEAD");

        let oracle_head = oracle_repo.head().expect("libgit2 should find HEAD");
        let oracle_commit = oracle_head
            .peel_to_commit()
            .expect("libgit2 should peel HEAD to commit");

        assert_eq!(
            tinygit_head_hash,
            oid_to_hash(&oracle_commit.id()),
            "HEAD commit hashes match"
        );

        assert_eq!(
            tinygit_commit.tree,
            oid_to_hash(&oracle_commit.tree_id()),
            "HEAD commit tree matches"
        );

        assert_eq!(
            tinygit_commit.parents,
            oracle_commit
                .parent_ids()
                .map(|oid| oid_to_hash(&oid))
                .collect::<Vec<_>>(),
            "HEAD commit parents match"
        );

        assert_eq!(
            tinygit_commit.author.0,
            signature_to_identity(oracle_commit.author()),
            "HEAD commit authors match"
        );

        assert_eq!(
            tinygit_commit.committer.0,
            signature_to_identity(oracle_commit.committer()),
            "HEAD commit committers match"
        );

        assert_eq!(
            tinygit_commit.author.1.timestamp(),
            oracle_commit.time().seconds(),
            "HEAD commit times match"
        );

        assert_eq!(
            tinygit_commit.description,
            oracle_commit.message().unwrap(),
            "HEAD commit messages match"
        );
    }
}

mod fuzz {
    //! This is a differential fuzz suite that uses bolero for generating random sequences of
    //! operations and runs them using both `libgit2` and `tinygit` and compares the results.

    use bolero::{TypeGenerator, ValueGenerator, check, one_value_of, produce};
    use git2::Repository;
    use nutype::nutype;
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::Path,
    };
    use strum::EnumDiscriminants;
    use tempfile::TempDir;
    use tinygit::{
        Repo,
        storage::index::{Index, IndexEntry},
    };

    #[derive(Debug, Clone, TypeGenerator)]
    struct Name(#[generator(produce::<String>().with().len(1..=255).chars('A'..='z'))] String);

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Modification {
        Insert { offset: usize, bytes: Vec<u8> },
        Delete { offset: usize, size: usize },
    }

    impl Modification {
        fn apply(&self, contents: &mut Vec<u8>) {
            match self {
                Modification::Insert { offset, bytes } => {
                    assert!(*offset <= contents.len(), "Insert offset out of bounds");
                    contents.splice(*offset..*offset, bytes.iter().copied());
                }
                Modification::Delete { offset, size } => {
                    assert!(
                        *offset + *size <= contents.len(),
                        "Delete range out of bounds"
                    );
                    contents.drain(*offset..*offset + *size);
                }
            }
        }
    }

    #[derive(Debug, Clone, EnumDiscriminants)]
    #[strum_discriminants(derive(TypeGenerator))]
    enum Op {
        Create(Name, Vec<u8>),
        Modify(Name, Modification),
        GitAdd(Vec<Name>),
        GitRm(Vec<Name>),
    }

    #[nutype(derive(Debug, Clone))]
    struct Sequence(Vec<Op>);

    impl TypeGenerator for Sequence {
        fn generate<D: bolero::Driver>(driver: &mut D) -> Option<Self> {
            let mut seq: Vec<Op> = Vec::default();

            // working tree: file path -> file contents
            let mut working: BTreeMap<String, Vec<u8>> = BTreeMap::default();

            // tracked files
            let mut index: BTreeSet<String> = BTreeSet::default();

            let n = (0..32usize).generate(driver)?;

            for _ in 0..n {
                let mut kinds = Vec::default();

                kinds.push(OpDiscriminants::Create);

                // modifying or staging is valid for any file that exists in the working tree.
                if !working.is_empty() {
                    kinds.push(OpDiscriminants::Modify);
                    kinds.push(OpDiscriminants::GitAdd);
                }

                // `git rm` is valid only for files tracked by the index.
                if !index.is_empty() {
                    kinds.push(OpDiscriminants::GitRm);
                }

                let kind = one_value_of(&kinds[..]).generate(driver)?;

                let op = match kind {
                    OpDiscriminants::Create => {
                        let name = Name::generate(driver)?;
                        let contents = <Vec<u8> as TypeGenerator>::generate(driver)?;

                        working.insert(name.0.clone(), contents.clone());

                        Op::Create(name, contents)
                    }
                    OpDiscriminants::Modify => {
                        let names: Vec<String> = working.keys().cloned().collect();
                        let name = Name(one_value_of(&names[..]).generate(driver)?);
                        let contents = working
                            .get_mut(&name.0)
                            .expect("file must exist in working tree");

                        let is_insert = if contents.is_empty() {
                            true
                        } else {
                            <bool as TypeGenerator>::generate(driver)?
                        };

                        let delta = if is_insert {
                            let offset = (0..=contents.len()).generate(driver)?;
                            let bytes = <Vec<u8> as TypeGenerator>::generate(driver)?;

                            Modification::Insert { offset, bytes }
                        } else {
                            let offset = (0..contents.len()).generate(driver)?;
                            let max_size = contents.len() - offset;
                            let size = (0..=max_size).generate(driver)?;

                            Modification::Delete { offset, size }
                        };

                        delta.apply(contents);

                        Op::Modify(name, delta)
                    }
                    OpDiscriminants::GitAdd => {
                        let names: Vec<String> = working.keys().cloned().collect();
                        let name = Name(one_value_of(&names[..]).generate(driver)?);

                        index.insert(name.0.clone());

                        Op::GitAdd(vec![name])
                    }
                    OpDiscriminants::GitRm => {
                        let names: Vec<String> = index.iter().cloned().collect();
                        let name = Name(one_value_of(&names[..]).generate(driver)?);

                        assert!(
                            working.remove(&name.0).is_some(),
                            "name should exist in the working tree"
                        );
                        assert!(index.remove(&name.0), "name should exist in the index");

                        Op::GitRm(vec![name])
                    }
                };

                seq.push(op);
            }

            Some(Self::new(seq))
        }
    }

    /// All the properties that libgit2 reports for an index entry.
    #[derive(Debug, PartialEq, Eq)]
    struct Entry {
        path: String,
        mode: u32,
        hash: String,
        ctime_seconds: u32,
        ctime_ns_fractions: u32,
        mtime_seconds: u32,
        mtime_ns_fractions: u32,
        dev: u32,
        ino: u32,
        uid: u32,
        gid: u32,
        file_size: u32,
        flags: u16,
    }

    impl From<&IndexEntry> for Entry {
        fn from(entry: &IndexEntry) -> Self {
            let mode = (u32::from(u8::from(entry.mode.object_type())) << 12)
                | u32::from(entry.mode.permission());
            let flags = (u16::from(entry.flags.assume_valid()) << 15)
                | (u16::from(entry.flags.extended()) << 14)
                | (u16::from(entry.flags.stage()) << 12)
                | entry.flags.name_length();

            Self {
                path: entry.path_name.to_string(),
                mode,
                hash: entry.hash.to_string(),
                ctime_seconds: entry.ctime_seconds,
                ctime_ns_fractions: entry.ctime_ns_fractions,
                mtime_seconds: entry.mtime_seconds,
                mtime_ns_fractions: entry.mtime_ns_fractions,
                dev: entry.dev,
                ino: entry.ino,
                uid: entry.uid,
                gid: entry.gid,
                file_size: entry.file_size,
                flags,
            }
        }
    }

    impl From<git2::IndexEntry> for Entry {
        fn from(entry: git2::IndexEntry) -> Self {
            Self {
                path: String::from_utf8_lossy(&entry.path).into_owned(),
                mode: entry.mode,
                hash: entry.id.to_string(),
                ctime_seconds: entry.ctime.seconds() as u32,
                ctime_ns_fractions: entry.ctime.nanoseconds(),
                mtime_seconds: entry.mtime.seconds() as u32,
                mtime_ns_fractions: entry.mtime.nanoseconds(),
                dev: entry.dev,
                ino: entry.ino,
                uid: entry.uid,
                gid: entry.gid,
                file_size: entry.file_size,
                flags: entry.flags,
            }
        }
    }

    fn get_libgit2_index_entries(repo: &Repository) -> Vec<Entry> {
        let mut index = repo.index().expect("should be able to get repo index");
        let _ = index.read(false);
        index.iter().map(Entry::from).collect()
    }

    /// Ask libgit2 what the index contains.
    fn expected_index_entries(dir: &Path) -> Vec<Entry> {
        let repo = Repository::open(dir).expect("should be able to open git repo");
        get_libgit2_index_entries(&repo)
    }

    fn run_op(repo: &Repository, op: &Op) {
        let path = repo.workdir().expect("repo must have a workdir");
        match op {
            Op::Create(name, contents) => {
                fs::write(path.join(&name.0), contents).expect("should be able to write file");
            }
            Op::Modify(name, delta) => {
                let file_path = path.join(&name.0);
                let mut contents =
                    fs::read(&file_path).expect("should be able to read file to modify");
                delta.apply(&mut contents);
                fs::write(&file_path, contents).expect("should be able to write modified file");
            }
            Op::GitAdd(names) => {
                let mut index = repo.index().expect("should be able to get repo index");
                for name in names {
                    index
                        .add_path(Path::new(&name.0))
                        .expect("libgit2 should be able to add file");
                }
                index
                    .write()
                    .expect("libgit2 should be able to write index");
            }
            Op::GitRm(names) => {
                let mut index = repo.index().expect("should be able to get repo index");
                for name in names {
                    let file_path = path.join(&name.0);
                    if file_path.exists() {
                        let _ = fs::remove_file(&file_path);
                    }
                    index
                        .remove_path(Path::new(&name.0))
                        .expect("libgit2 should be able to remove file");
                }
                index
                    .write()
                    .expect("libgit2 should be able to write index");
            }
        }
    }

    /// Creates a repository with libgit2, and applies the generated operations.
    fn run_ops(path: &Path, ops: &Sequence) {
        let repo = Repository::init(path).expect("should be able to init git repo");

        for op in ops.clone().into_inner() {
            run_op(&repo, &op);
        }
    }

    fn run_op_tinygit(repo: &Repo, libgit2_repo: &Repository, op: &Op) {
        let path = &repo.path;
        match op {
            Op::Create(name, contents) => {
                fs::write(path.join(&name.0), contents).expect("should be able to write file");
            }
            Op::Modify(name, delta) => {
                let file_path = path.join(&name.0);
                let mut contents =
                    fs::read(&file_path).expect("should be able to read file to modify");
                delta.apply(&mut contents);
                fs::write(&file_path, contents).expect("should be able to write modified file");
            }
            Op::GitAdd(names) => {
                for name in names {
                    repo.stage(path.join(&name.0))
                        .expect("tinygit stage shouldn't fail");
                }
            }
            Op::GitRm(names) => {
                let mut index = libgit2_repo
                    .index()
                    .expect("should be able to get repo index");
                for name in names {
                    let file_path = path.join(&name.0);
                    if file_path.exists() {
                        let _ = fs::remove_file(&file_path);
                    }
                    index
                        .remove_path(Path::new(&name.0))
                        .expect("libgit2 should be able to remove file");
                }
                index
                    .write()
                    .expect("libgit2 should be able to write index");
            }
        }
    }

    #[test]
    fn fuzz_index_parsing() {
        check!().with_type::<Sequence>().for_each(|ops| {
            let dir = TempDir::new().unwrap();
            let path = dir.path();

            run_ops(path, ops);

            let mut expected = expected_index_entries(path);
            expected.sort_by(|left, right| left.path.cmp(&right.path));

            let index_path = path.join(".git/index");

            if !index_path.exists() {
                assert!(expected.is_empty());
                return;
            }

            // open index and assert it was properly parsed
            let index = Index::open(index_path).expect("should be able to parse index");

            // assert entries correctness
            let mut parsed: Vec<Entry> = index.entries().map(Entry::from).collect();
            parsed.sort_by(|left, right| left.path.cmp(&right.path));

            assert_eq!(parsed, expected);
        });
    }

    /// This test parses Git's index with tinygit, writes it back, and asserts that the result is
    /// byte-for-byte identical to Git's index.
    #[test]
    fn fuzz_index_roundtrip() {
        check!().with_type::<Sequence>().for_each(|ops| {
            let dir = TempDir::new().unwrap();
            let path = dir.path();

            run_ops(path, ops);

            let index_path = path.join(".git/index");

            if !index_path.exists() {
                return;
            }

            // read Git's index before parsing, because `Index::write` truncates the file in place.
            let original = fs::read(&index_path).expect("should be able to read Git's index");

            let index = Index::open(index_path.clone()).expect("Git's index should parse");

            index
                .try_write()
                .expect("tinygit should re-encode the index");

            let rewritten = fs::read(&index_path).expect("should be able to read tinygit's index");

            assert_eq!(
                original, rewritten,
                "tinygit's index should be byte-identical to Git's"
            );
        });
    }

    /// Tests random sequences of git operations by running them once in a directory using git, and
    /// running them in another directory using tinygit. Asserts the contents of the index are the same
    /// (minus the things that can't be the same, like the timestamps and inodes)
    #[test]
    fn fuzz_index_differential_add() {
        check!().with_type::<Sequence>().for_each(|ops| {
        let git_dir = TempDir::new().unwrap();
        let tinygit_dir = TempDir::new().unwrap();

        let git_repo = Repository::init(git_dir.path()).expect("libgit2 should be able to init git repo");
        let tinygit_libgit2 = Repository::init(tinygit_dir.path()).expect("libgit2 should be able to init tinygit repo");
        let tinygit_repo = Repo::open(tinygit_dir.path()).expect("should be able to open tinygit repo");

        let indexed = |entries: Vec<Entry>| -> Vec<Entry> {
            let mut entries = entries;
            entries.sort_by(|left, right| left.path.cmp(&right.path));
            entries
        };

        let content_view = |entries: Vec<Entry>| -> Vec<(String, u32, String, u16)> {
            entries
                .into_iter()
                .map(|entry| (entry.path, entry.mode, entry.hash, entry.flags))
                .collect()
        };

        for op in ops.clone().into_inner() {
            run_op(&git_repo, &op);
            run_op_tinygit(&tinygit_repo, &tinygit_libgit2, &op);

            let tinygit_index_path = tinygit_dir.path().join(".git/index");

            if tinygit_index_path.exists() {
                let tinygit_view = indexed(
                    Index::open(tinygit_index_path)
                        .expect("tinygit's index should parse")
                        .entries()
                        .map(Entry::from)
                        .collect(),
                );
                let ls_files_view = indexed(get_libgit2_index_entries(&tinygit_libgit2));
                assert_eq!(
                    tinygit_view, ls_files_view,
                    "tinygit internal index parse must match git ls-files on tinygit dir after op: {:?}",
                    op
                );

                let git_view = indexed(get_libgit2_index_entries(&git_repo));
                assert_eq!(
                    content_view(git_view),
                    content_view(tinygit_view),
                    "git index and tinygit index content must match after op: {:?}",
                    op
                );
            } else {
                let git_view = indexed(get_libgit2_index_entries(&git_repo));
                assert!(
                    git_view.is_empty(),
                    "tinygit index does not exist, but git index is not empty after op: {:?}",
                    op
                );
            }
        }
    });
    }
}
