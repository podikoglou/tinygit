# tinygit
Implementation of (some of) Git in Rust.

I started working on this project when I was inspired by what
[Cursor was doing with Git](https://cursor.com/blog/git-at-any-scale) and got
to learn about how neat Git really is.

This project is in the form of a library + a CLI with *some* of the original
Git commands (and some that I came up with). Concretely:
```
One of the following subcommands must be present:
    help
    hash-object
    ls-buckets
    ls-objects
    parse-object
    parse-pack
    unpack-objects
    log
    index-pack
    add
    commit
```


## AI Disclaimer
~90% of the code is human-written.
