// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::LazyLock;

use camino::Utf8PathBuf;
use regex::Regex;

#[derive(Debug, Clone, Hash, Default)]
pub enum BuildInput {
    Single(String),
    Multiple(Vec<String>),
    Glob(Glob),
    Inputs(Vec<BuildInput>),
    #[default]
    Empty,
}

impl AsRef<BuildInput> for BuildInput {
    fn as_ref(&self) -> &BuildInput {
        self
    }
}

impl From<String> for BuildInput {
    fn from(v: String) -> Self {
        BuildInput::Single(v)
    }
}

impl From<&str> for BuildInput {
    fn from(v: &str) -> Self {
        BuildInput::Single(v.to_owned())
    }
}

impl From<Vec<String>> for BuildInput {
    fn from(v: Vec<String>) -> Self {
        BuildInput::Multiple(v)
    }
}

impl From<Glob> for BuildInput {
    fn from(v: Glob) -> Self {
        BuildInput::Glob(v)
    }
}

impl From<&BuildInput> for BuildInput {
    fn from(v: &BuildInput) -> Self {
        BuildInput::Inputs(vec![v.clone()])
    }
}

impl From<&[BuildInput]> for BuildInput {
    fn from(v: &[BuildInput]) -> Self {
        BuildInput::Inputs(v.to_vec())
    }
}

impl From<Vec<BuildInput>> for BuildInput {
    fn from(v: Vec<BuildInput>) -> Self {
        BuildInput::Inputs(v)
    }
}

impl From<Utf8PathBuf> for BuildInput {
    fn from(v: Utf8PathBuf) -> Self {
        BuildInput::Single(v.into_string())
    }
}

impl BuildInput {
    pub fn add_to_vec(
        &self,
        vec: &mut Vec<String>,
        exisiting_outputs: &HashMap<String, Vec<String>>,
    ) {
        let mut resolve_and_add = |value: &str| {
            if let Some(stripped) = value.strip_prefix(':') {
                let files = exisiting_outputs.get(stripped).unwrap_or_else(|| {
                    println!("{:?}", &exisiting_outputs);
                    panic!("input referenced {value}, but rule missing/not processed");
                });
                for file in files {
                    vec.push(file.into())
                }
            } else {
                vec.push(value.into());
            }
        };

        match self {
            BuildInput::Single(s) => resolve_and_add(s),
            BuildInput::Multiple(v) => {
                for item in v {
                    resolve_and_add(item);
                }
            }
            BuildInput::Glob(glob) => {
                for path in glob.resolve() {
                    vec.push(path.into_string());
                }
            }
            BuildInput::Inputs(inputs) => {
                for input in inputs {
                    input.add_to_vec(vec, exisiting_outputs)
                }
            }
            BuildInput::Empty => {}
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub struct Glob {
    pub include: String,
    pub exclude: Option<String>,
}

static CACHED_FILES: LazyLock<Vec<Utf8PathBuf>> = LazyLock::new(cache_files);

/// Walking the source tree once instead of for each glob yields ~4x speed
/// improvements.
///
/// Uses the `ignore` crate so `.gitignore` is respected: build outputs (`out/`,
/// `node_modules/`, the Android subtree's generated `build/` dirs, git
/// worktrees, ...) are skipped automatically. A short explicit list covers
/// non-source dirs git does NOT ignore: `.agents` (agent-tooling docs) and
/// `reference/` (untracked reference clones, e.g. Anki-Android). Symlinks and
/// directories are dropped; only regular files are returned.
fn cache_files() -> Vec<Utf8PathBuf> {
    fn is_excluded(entry: &ignore::DirEntry) -> bool {
        // matched at any depth (e.g. also android/app/.agents); none of these
        // names are ever real project source.
        matches!(
            entry.file_name().to_str(),
            Some(".git" | ".claude" | ".agents" | "reference")
        )
    }

    let mut files: Vec<Utf8PathBuf> = ignore::WalkBuilder::new(".")
        // keep tracked dotfiles (.dprint.json, .github, .rustfmt.toml, ...);
        // git-ignored ones are still dropped by the gitignore filter below.
        .hidden(false)
        // reproducible regardless of the user's global/parent gitignores.
        .git_global(false)
        .parents(false)
        .filter_entry(|e| !is_excluded(e))
        .build()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            // regular files only (skips directories and symlinks)
            if !entry.file_type()?.is_file() {
                return None;
            }
            let path = entry.path().strip_prefix("./").unwrap_or(entry.path());
            Utf8PathBuf::from_path_buf(path.to_owned()).ok()
        })
        .collect();
    // deterministic ordering for reproducible build.ninja output
    files.sort();
    files
}

impl Glob {
    pub fn resolve(&self) -> impl Iterator<Item = Utf8PathBuf> {
        let include = globset::GlobBuilder::new(&self.include)
            .literal_separator(true)
            .build()
            .unwrap()
            .compile_matcher();
        let exclude = self.exclude.as_ref().map(|glob| {
            globset::GlobBuilder::new(glob)
                .literal_separator(true)
                .build()
                .unwrap()
                .compile_matcher()
        });
        CACHED_FILES.iter().filter_map(move |path| {
            if include.is_match(path) {
                let excluded = exclude
                    .as_ref()
                    .map(|exclude| exclude.is_match(path))
                    .unwrap_or_default();
                if !excluded {
                    return Some(path.to_owned());
                }
            }
            None
        })
    }
}

pub fn space_separated<I>(iter: I) -> String
where
    I: IntoIterator,
    I::Item: Display,
{
    itertools::join(iter, " ")
}

/// Join target inputs with a space. Any whitespace characters in the inputs are
/// escaped as `$ `
pub fn join_inputs<I>(iter: I) -> String
where
    I: IntoIterator,
    I::Item: Display,
{
    static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s").unwrap());
    let iter = iter.into_iter().map(|input| {
        let input = input.to_string();
        WHITESPACE_RE.replace_all(input.trim(), "$$$0").to_string()
    });
    itertools::join(iter, " ")
}
