//! This module deals with `.gitignore` patterns and directory file exclusion.

use std::path::{Path, PathBuf};

use bstr::{BStr, BString, ByteSlice};
use winnow::{
    ascii::line_ending,
    combinator::{alt, eof, opt, terminated},
    token::take_till,
    ModalResult, Parser,
};

use crate::{ende::Decode, parsing::Stream, TinygitError};

/// A single rule parsed from a `.gitignore` file or built-in defaults.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct IgnorePattern {
    pub prefix: BString,
    pub pattern: BString,
    pub is_negated: bool,
    pub is_dir_only: bool,
    pub is_anchored: bool,
}

impl IgnorePattern {
    /// Checks whether this pattern matches the given path and directory flag.
    #[must_use]
    pub fn matches(&self, path: &BStr, is_dir: bool) -> bool {
        if self.is_dir_only && !is_dir {
            return false;
        }

        let target_path = if self.prefix.is_empty() {
            path
        } else if let Some(rest) = path.strip_prefix(self.prefix.as_bytes()) {
            if let Some(rest) = rest.strip_prefix(b"/") {
                rest.as_bstr()
            } else {
                return false;
            }
        } else {
            return false;
        };

        let target = if self.is_anchored {
            target_path
        } else {
            target_path
                .rsplit_str("/")
                .next()
                .unwrap_or(target_path)
                .as_bstr()
        };

        if self.pattern == "*" || self.pattern == target {
            return true;
        }

        if let Some(suffix) = self.pattern.strip_prefix(b"*") {
            return target.ends_with(suffix);
        }

        if let Some(prefix) = self.pattern.strip_suffix(b"*") {
            return target.starts_with(prefix);
        }

        if let Some(pos) = self.pattern.find_byte(b'*') {
            let (prefix, suffix) = self.pattern.split_at(pos);
            let suffix = &suffix[1..];
            return target.starts_with(prefix)
                && target.ends_with(suffix)
                && target.len() >= prefix.len() + suffix.len();
        }

        false
    }
}

impl Decode for IgnorePattern {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let is_negated = opt("!").map(|opt| opt.is_some()).parse_next(input)?;
        let raw = take_till(0.., (b'\r', b'\n')).parse_next(input)?;
        let raw = raw.trim_end_with(|c| c == ' ' || c == '\t');

        let (raw, is_dir_only) = if let Some(stripped) = raw.strip_suffix(b"/") {
            (stripped, true)
        } else {
            (raw, false)
        };

        let (pattern, is_anchored) = if let Some(stripped) = raw.strip_prefix(b"/") {
            (stripped, true)
        } else {
            (raw, raw.contains(&b'/'))
        };

        Ok(Self {
            prefix: BString::default(),
            pattern: BString::from(pattern),
            is_negated,
            is_dir_only,
            is_anchored,
        })
    }
}

/// An ignore specification for a directory hierarchy.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Ignore {
    pub root: PathBuf,
    pub patterns: Vec<IgnorePattern>,
}

impl Ignore {
    /// Creates an empty [Ignore] with default rules (`.git`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: PathBuf::new(),
            patterns: vec![IgnorePattern {
                prefix: BString::default(),
                pattern: BString::from(".git"),
                is_negated: false,
                is_dir_only: false,
                is_anchored: false,
            }],
        }
    }

    /// Loads the ignore rules for a directory.
    pub fn for_dir(dir: impl AsRef<Path>) -> Result<Self, TinygitError> {
        let dir = dir.as_ref();
        let root = dir
            .ancestors()
            .find(|p| p.join(".git").exists())
            .unwrap_or(dir);

        let mut ignore = Self::new();
        ignore.root = root.to_path_buf();

        let exclude = root.join(".git/info/exclude");
        if exclude.is_file() {
            ignore.load_file(&exclude, BString::default())?;
        }

        let relative = dir.strip_prefix(root).unwrap_or(Path::new(""));
        let mut current = root.to_path_buf();

        let root_gitignore = current.join(".gitignore");
        if root_gitignore.is_file() {
            ignore.load_file(&root_gitignore, BString::default())?;
        }

        let mut prefix = PathBuf::new();
        for component in relative.components() {
            prefix.push(component);
            current.push(component);
            let gitignore = current.join(".gitignore");
            if gitignore.is_file() {
                let prefix_bytes = BString::from(prefix.as_os_str().as_encoded_bytes());
                ignore.load_file(&gitignore, prefix_bytes)?;
            }
        }

        Ok(ignore)
    }

    /// Creates an ignore context for a child directory, inheriting existing rules.
    pub fn child(&self, child_dir: impl AsRef<Path>) -> Result<Self, TinygitError> {
        let child_dir = child_dir.as_ref();
        let gitignore = child_dir.join(".gitignore");
        if !gitignore.is_file() {
            return Ok(self.clone());
        }

        let mut ignore = self.clone();
        let rel = child_dir.strip_prefix(&self.root).unwrap_or(child_dir);
        let prefix = BString::from(rel.as_os_str().as_encoded_bytes());
        ignore.load_file(&gitignore, prefix)?;
        Ok(ignore)
    }

    /// Checks if a path should be ignored.
    pub fn is_ignored(&self, path: impl AsRef<Path>, is_dir: bool) -> bool {
        let path = path.as_ref();
        let path = path.strip_prefix(&self.root).unwrap_or(path);
        let path = path.as_os_str().as_encoded_bytes().as_bstr();
        if path.is_empty() {
            return false;
        }

        // `.git` anywhere in the path hierarchy is always ignored.
        if path.split_str("/").any(|comp| comp == b".git") {
            return true;
        }

        // If an ancestor directory is ignored, all of its contents are ignored.
        let comps: Vec<&[u8]> = path.split_str("/").collect();
        let mut prefix = BString::default();
        for (i, comp) in comps.iter().enumerate().take(comps.len().saturating_sub(1)) {
            if i > 0 {
                prefix.push(b'/');
            }
            prefix.extend_from_slice(comp);
            if self.is_direct_match(prefix.as_bstr(), true) == Some(true) {
                return true;
            }
        }

        self.is_direct_match(path, is_dir).unwrap_or(false)
    }

    fn load_file(&mut self, path: &Path, prefix: BString) -> Result<(), TinygitError> {
        let mut loaded = Self::decode_file(path)?;
        for pattern in &mut loaded.patterns {
            pattern.prefix = prefix.clone();
        }
        self.patterns.extend(loaded.patterns);
        Ok(())
    }

    fn is_direct_match(&self, path: &BStr, is_dir: bool) -> Option<bool> {
        let mut matched = None;
        for pattern in &self.patterns {
            if pattern.matches(path, is_dir) {
                matched = Some(!pattern.is_negated);
            }
        }
        matched
    }
}

impl Decode for Ignore {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let mut patterns = Vec::new();

        while !input.is_empty() {
            let line = terminated(take_till(0.., (b'\r', b'\n')), alt((line_ending, eof)))
                .parse_next(input)?;

            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(b"#") {
                continue;
            }

            let mut line_stream = Stream::new(trimmed);
            patterns.push(IgnorePattern::parse(&mut line_stream)?);
        }

        Ok(Self {
            root: PathBuf::new(),
            patterns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_rules() {
        let input = b"*.log\n/build/\n!important.txt\n# comment\n   \n";
        let ignore = Ignore::decode_bytes(input).unwrap();
        assert_eq!(ignore.patterns.len(), 3);

        assert_eq!(ignore.patterns[0].pattern, "*.log");
        assert!(!ignore.patterns[0].is_negated);
        assert!(!ignore.patterns[0].is_dir_only);
        assert!(!ignore.patterns[0].is_anchored);

        assert_eq!(ignore.patterns[1].pattern, "build");
        assert!(!ignore.patterns[1].is_negated);
        assert!(ignore.patterns[1].is_dir_only);
        assert!(ignore.patterns[1].is_anchored);

        assert_eq!(ignore.patterns[2].pattern, "important.txt");
        assert!(ignore.patterns[2].is_negated);
        assert!(!ignore.patterns[2].is_dir_only);
        assert!(!ignore.patterns[2].is_anchored);
    }

    #[test]
    fn matches_wildcards_and_negation() {
        let input = b"*.log\n!important.log\n";
        let mut ignore = Ignore::new();
        let loaded = Ignore::decode_bytes(input).unwrap();
        ignore.patterns.extend(loaded.patterns);

        assert!(ignore.is_ignored("debug.log", false));
        assert!(ignore.is_ignored("nested/debug.log", false));
        assert!(!ignore.is_ignored("important.log", false));
        assert!(!ignore.is_ignored("debug.txt", false));
    }

    #[test]
    fn matches_directories() {
        let input = b"target/\n";
        let mut ignore = Ignore::new();
        let loaded = Ignore::decode_bytes(input).unwrap();
        ignore.patterns.extend(loaded.patterns);

        assert!(ignore.is_ignored("target", true));
        assert!(ignore.is_ignored("sub/target", true));
        assert!(!ignore.is_ignored("target", false));
        assert!(ignore.is_ignored("target/file.txt", false));
    }

    #[test]
    fn ignores_git_by_default() {
        let ignore = Ignore::new();

        assert!(ignore.is_ignored(".git", true));
        assert!(ignore.is_ignored(".git", false));
        assert!(ignore.is_ignored(".git/HEAD", false));
        assert!(ignore.is_ignored(".git/objects/pack", true));
        assert!(ignore.is_ignored("sub/.git", true));
        assert!(ignore.is_ignored("sub/.git/config", false));
        assert!(!ignore.is_ignored(".gitignore", false));
        assert!(!ignore.is_ignored("src/main.rs", false));
    }
}
