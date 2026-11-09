mod common;

#[test]
fn index_read_with_extensions() {
    let (repo, _dir) = include_repo!("fixtures/repos/repo-10.tar");

    let index = repo.store.index().expect("should be able to parse index");

    assert_eq!(index.entries().count(), 1);
    let entry = index.entries().next().expect("should have one entry");
    assert_eq!(entry.path_name, "mmfmff");
}
