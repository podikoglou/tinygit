# Fixtures

## Objects
### Creating an object fixture
- Grab object from wherever (could be a regression, so, from a real repo, or could be synthetically constructed)
- Place into `tests/fixtures/objects`, with a file name of `<object-type>-<index>.obj`
- Run `tests/fixtures/scripts/hashes.sh`

## Repositories
- `repo-1`: Repo with 1 commit, of a single file. Contains three objects: one blob, one tree, one commit.
- `repo-2`: Repo with 414 commits of one text file (containing three numbers) each
- `repo-3`: Repo with 4 commits of one text file each, ending with a merge commit that has two parents
- `repo-4`: Repo with 4 commits, and a tag
- `repo-5`: Repo with 2 commits, with the objects related to the first packed, and the others not
- `repo-6`: Repo based on repo-5, with 4 commits, packed with `git gc` so that it contains delta-compressed objects
- `repo-7`: Repo with 2 commits of a file >64KB, packed with `git gc` so that it contains a delta-compressed object copying 64KB (size 0 in copy instruction header)
- `repo-8`: Repo with a commit containing a subdirectory and files sharing prefixes (`foo-bar`, `foo.c`, `foo/bar.rs`)
- `repo-9`: Repo with random commits, branches and tags. Comes with a a script (inside it) for generating similar repos.
- `repo-10`: Repo with an index containing extension blocks (such as cached tree `TREE`)

### Creating a repository fixture
- Create a repository somewhere, initialize it as you like
- `tar -cvf ../repo-<n>.tar`

It is crucical that this is ran inside the repository, so that the repository's
directly is not included in the tar!
