// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tools for checking for, or adding, headers (e.g. licenses, etc) in files.
//!
//! See the [license::spdx] module for more on using licenses as headers.
//!
//! # Examples
//!
//! Checking for a header:
//!
//! ```
//! // Copyright 2023 Google LLC.
//! // SPDX-License-Identifier: Apache-2.0
//! use file_header::*;
//! use std::path::Path;
//!
//! let checker = SingleLineChecker::new("Foo License".to_string(), 10);
//! let header = Header::new(checker, "Foo License\nmore license text".to_string());
//!
//! match check_headers_recursively(
//!     Path::new("/some/dir"),
//!     // check every file -- see `globset` crate for path patterns
//!     |p| true,
//!     header,
//!     // check with 4 threads
//!     4
//! ) {
//!     Ok(fr) => { println!("files without the header: {:?}", fr.no_header_files) }
//!     Err(e) => { println!("got an error: {:?}", e) }
//! }
//!
//! ```
//!

#![deny(missing_docs, unsafe_code)]

use std::{
    fs,
    io::{self, BufRead as _, Write as _},
    iter::FromIterator,
    path, thread,
};

pub mod license;

/// A file header to check for, or add to, files.
#[derive(Clone)]
pub struct Header<C: HeaderChecker> {
    /// A checker to determine if the desired header is already present.
    checker: C,
    /// The header text to add, without comments or other filetype-specific framing.
    header: String,
}

impl<C: HeaderChecker> Header<C> {
    /// Construct a new `Header` with the `checker` used to determine if the header is already
    /// present, and the plain `header` text to add.
    ///
    /// `header` does not need to have applicable comment syntax, etc, as that will be added for
    /// each file type encountered.
    pub fn new(checker: C, header: String) -> Self {
        Self { checker, header }
    }

    /// Return `true` if the file has the desired header, false otherwise.
    pub fn header_present(&self, input: &mut impl io::Read) -> io::Result<bool> {
        self.checker.check(input)
    }

    /// Add the header, with appropriate formatting for the type of file indicated by `p`'s
    /// extension, if the header is not already present.
    /// Returns `true` if the header was added.
    pub fn add_header_if_missing(&self, p: &path::Path) -> Result<bool, AddHeaderError> {
        let err_mapper = |e| AddHeaderError::IoError(p.to_path_buf(), e);
        let contents = fs::read_to_string(p).map_err(err_mapper)?;
        if self
            .header_present(&mut contents.as_bytes())
            .map_err(err_mapper)?
        {
            return Ok(false);
        }
        let mut effective_header = header_delimiters(p)
            .ok_or_else(|| AddHeaderError::UnrecognizedExtension(p.to_path_buf()))
            .map(|d| wrap_header(&self.header, d))?;
        let mut after_header = contents.as_str();
        // check for a magic first line and if present, add the license after the first line
        if let Some((first_line, rest)) = contents.split_once('\n') {
            if MAGIC_FIRST_LINES.iter().any(|l| first_line.contains(l)) {
                let mut first_line = first_line.to_string();
                first_line.push('\n');
                effective_header.insert_str(0, &first_line);
                after_header = rest;
            }
        }
        // write the license
        let mut f = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(p)
            .map_err(err_mapper)?;
        f.write_all(effective_header.as_bytes())
            .map_err(err_mapper)?;
        // newline to separate the header from previous contents
        f.write_all("\n".as_bytes()).map_err(err_mapper)?;
        f.write_all(after_header.as_bytes()).map_err(err_mapper)?;
        Ok(true)
    }

    /// Delete the header, with appropriate formatting for the type of file indicated by `p`'s
    /// extension, if the header is already present.
    /// Returns `true` if the header was deleted.
    pub fn delete_header_if_present(&self, p: &path::Path) -> Result<bool, DeleteHeaderError> {
        let err_mapper = |e| DeleteHeaderError::IoError(p.to_path_buf(), e);
        let contents = fs::read_to_string(p).map_err(err_mapper)?;
        if !self
            .header_present(&mut contents.as_bytes())
            .map_err(err_mapper)?
        {
            return Ok(false);
        }
        let mut effective_header = header_delimiters(p)
            .ok_or_else(|| DeleteHeaderError::UnrecognizedExtension(p.to_path_buf()))
            .map(|d| wrap_header(&self.header, d))?;
        // include the newline separator appended by add_header_if_missing()
        effective_header.push('\n');

        // the checker is conservative: it may look for only a substring of the license, but
        // deletion will only have an effect if the entire wrapped header is present.
        if !contents.contains(&effective_header) {
            return Ok(false);
        }

        // remove the first copy of the header to avoid touching the license text in a string
        // literal, etc.
        let remainder = contents.replacen(&effective_header, "", 1);
        // write the remainder
        let mut f = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(p)
            .map_err(err_mapper)?;
        f.write_all(remainder.as_bytes()).map_err(err_mapper)?;
        Ok(true)
    }
}

/// Errors that can occur when adding a header
#[derive(Debug, thiserror::Error)]
pub enum AddHeaderError {
    /// IO error while adding the header to the path
    #[error("I/O error at {0:?}: {1}")]
    IoError(path::PathBuf, io::Error),
    /// The file at the path had an unrecognized extension
    #[error("Unknown file extension: {0:?}")]
    UnrecognizedExtension(path::PathBuf),
}

/// Errors that can occur when deleting a header
#[derive(Debug, thiserror::Error)]
pub enum DeleteHeaderError {
    /// IO error while deleting the header from the path
    #[error("I/O error at {0:?}: {1}")]
    IoError(path::PathBuf, io::Error),
    /// The file at the path had an unrecognized extension
    #[error("Unknown file extension: {0:?}")]
    UnrecognizedExtension(path::PathBuf),
}

/// Checks for headers in files, like licenses or author attribution.
///
/// This is intended to be used via [`Header`], not called directly.
pub trait HeaderChecker: Send + Clone {
    /// Return `true` if the file has the desired header, `false` otherwise.
    fn check(&self, file: &mut impl io::Read) -> io::Result<bool>;
}

/// Checks for a pattern in the first several lines of each file.
#[derive(Clone)]
pub struct SingleLineChecker {
    /// Pattern to do a substring match on in each of the first `max_lines` lines of the file
    pattern: String,
    /// Number of lines to search through
    max_lines: usize,
}

impl SingleLineChecker {
    /// Construct a `SingleLineChecker` that looks for `pattern` in the first `max_lines` of a file.
    pub fn new(pattern: String, max_lines: usize) -> Self {
        Self { pattern, max_lines }
    }
}

impl HeaderChecker for SingleLineChecker {
    fn check(&self, input: &mut impl io::Read) -> io::Result<bool> {
        let mut reader = io::BufReader::new(input);
        let mut lines_read = 0;
        // reuse buffer to minimize allocation
        let mut line = String::new();
        // only read the first bit of the file
        while lines_read < self.max_lines {
            line.clear();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                // EOF
                return Ok(false);
            }
            lines_read += 1;
            if line.contains(&self.pattern) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Reasons why a file may not have a header
#[derive(Copy, Clone)]
enum CheckStatus {
    /// The header was not found in the file
    HeaderNotFound,
    /// A file appears to be binary
    BinaryFile,
}

/// The output of checking a single file
#[derive(Clone)]
struct FileResult {
    path: path::PathBuf,
    status: CheckStatus,
}

/// Aggregated results for recursively checking a directory tree of files.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct FileResults {
    /// Paths that did not have a header
    pub no_header_files: Vec<path::PathBuf>,
    /// Paths that appeared to be binary, not UTF-8 text
    pub binary_files: Vec<path::PathBuf>,
}

impl FileResults {
    /// Returns `true` if any files scanned did not have a header
    pub fn has_failure(&self) -> bool {
        !self.no_header_files.is_empty() || !self.binary_files.is_empty()
    }
}

impl FromIterator<FileResult> for FileResults {
    fn from_iter<I>(iter: I) -> FileResults
    where
        I: IntoIterator<Item = FileResult>,
    {
        let mut results = FileResults::default();
        for result in iter {
            match result.status {
                CheckStatus::HeaderNotFound => results.no_header_files.push(result.path),
                CheckStatus::BinaryFile => results.binary_files.push(result.path),
            }
        }
        results
    }
}

/// Recursively check for `header` in every file in `root` that matches `path_predicate`.
///
/// Checking the discovered files is parallelized across `num_threads` threads.
///
/// [`globset`](https://crates.io/crates/globset) is a useful crate for ignoring unwanted files in
/// `path_predicate`.
///
/// Returns a [`FileResults`] object containing the paths without headers detected, and the paths
/// which were not UTF-8 text.
pub fn check_headers_recursively(
    root: &path::Path,
    path_predicate: impl Fn(&path::Path) -> bool,
    header: Header<impl HeaderChecker + 'static>,
    num_threads: usize,
) -> Result<FileResults, CheckHeadersRecursivelyError> {
    let (path_tx, path_rx) = crossbeam::channel::unbounded::<path::PathBuf>();
    let (result_tx, result_rx) = crossbeam::channel::unbounded();
    // spawn a few threads to handle files in parallel
    let handles = (0..num_threads)
        .map(|_| {
            let path_rx = path_rx.clone();
            let result_tx = result_tx.clone();
            let header = header.clone();
            thread::spawn(move || {
                for p in path_rx {
                    match fs::File::open(&p).and_then(|mut f| header.header_present(&mut f)) {
                        Ok(header_present) => {
                            if header_present {
                                // no op
                            } else {
                                let res = FileResult {
                                    path: p,
                                    status: CheckStatus::HeaderNotFound,
                                };
                                result_tx.send(Ok(res)).unwrap();
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                            let res = FileResult {
                                path: p,
                                status: CheckStatus::BinaryFile,
                            };
                            result_tx.send(Ok(res)).unwrap();
                        }
                        Err(e) => result_tx
                            .send(Err(CheckHeadersRecursivelyError::IoError(p, e)))
                            .unwrap(),
                    }
                }
                // no more files
            })
        })
        .collect::<Vec<thread::JoinHandle<()>>>();
    // make sure result channel closes when threads complete
    drop(result_tx);
    find_files(root, path_predicate, path_tx)?;
    let res: FileResults = result_rx.into_iter().collect::<Result<_, _>>()?;
    for h in handles {
        h.join().unwrap();
    }
    Ok(res)
}

/// Errors that can occur when checking for headers recursively
#[derive(Debug, thiserror::Error)]
pub enum CheckHeadersRecursivelyError {
    /// An I/O error occurred while checking the path
    #[error("I/O error at {0:?}: {1}")]
    IoError(path::PathBuf, io::Error),
    /// `walkdir` could not navigate the directory structure
    #[error("Walkdir error: {0}")]
    WalkdirError(#[from] walkdir::Error),
}

/// Add the provided `header` to any file in `root` that matches `path_predicate` and that doesn't
/// already have a header as determined by `checker`.
///
/// Returns a list of paths that had headers added.
pub fn add_headers_recursively(
    root: &path::Path,
    path_predicate: impl Fn(&path::Path) -> bool,
    header: Header<impl HeaderChecker>,
) -> Result<Vec<path::PathBuf>, AddHeadersRecursivelyError> {
    // likely no need for threading since adding headers is only done occasionally
    recursive_optional_operation(root, path_predicate, |p| {
        header.add_header_if_missing(p).map_err(|e| e.into())
    })
}

/// Errors that can occur when adding a header recursively
#[derive(Debug, thiserror::Error)]
pub enum AddHeadersRecursivelyError {
    /// An I/O error occurred while adding the header to the path
    #[error("I/O error at {0:?}: {1}")]
    IoError(path::PathBuf, io::Error),
    /// `walkdir` could not navigate the directory structure
    #[error("Walkdir error: {0}")]
    WalkdirError(#[from] walkdir::Error),
    /// A file with an unrecognized extension was encountered at the path
    #[error("Unknown file extension: {0:?}")]
    UnrecognizedExtension(path::PathBuf),
}

impl From<AddHeaderError> for AddHeadersRecursivelyError {
    fn from(value: AddHeaderError) -> Self {
        match value {
            AddHeaderError::IoError(p, e) => Self::IoError(p, e),
            AddHeaderError::UnrecognizedExtension(p) => Self::UnrecognizedExtension(p),
        }
    }
}

/// Delete the provided `header` from any file in `root` that matches `path_predicate` and that
/// already has a header as determined by `header`'s checker.
///
/// Returns a list of paths that had headers removed.
pub fn delete_headers_recursively(
    root: &path::Path,
    path_predicate: impl Fn(&path::Path) -> bool,
    header: Header<impl HeaderChecker>,
) -> Result<Vec<path::PathBuf>, DeleteHeadersRecursivelyError> {
    recursive_optional_operation(root, path_predicate, |p| {
        header.delete_header_if_present(p).map_err(|e| e.into())
    })
}

/// Errors that can occur when adding a header recursively
#[derive(Debug, thiserror::Error)]
pub enum DeleteHeadersRecursivelyError {
    /// An I/O error occurred while removing the header from the path
    #[error("I/O error at {0:?}: {1}")]
    IoError(path::PathBuf, io::Error),
    /// `walkdir` could not navigate the directory structure
    #[error("Walkdir error: {0}")]
    WalkdirError(#[from] walkdir::Error),
    /// A file with an unrecognized extension was encountered at the path
    #[error("Unknown file extension: {0:?}")]
    UnrecognizedExtension(path::PathBuf),
}

impl From<DeleteHeaderError> for DeleteHeadersRecursivelyError {
    fn from(value: DeleteHeaderError) -> Self {
        match value {
            DeleteHeaderError::IoError(p, e) => Self::IoError(p, e),
            DeleteHeaderError::UnrecognizedExtension(p) => Self::UnrecognizedExtension(p),
        }
    }
}

/// Find all files starting from `root` that do not match the globs in `ignore`, publishing the
/// resulting paths into `dest`.
fn find_files(
    root: &path::Path,
    path_predicate: impl Fn(&path::Path) -> bool,
    dest: crossbeam::channel::Sender<path::PathBuf>,
) -> Result<(), walkdir::Error> {
    for r in walkdir::WalkDir::new(root).into_iter() {
        let entry = r?;
        if entry.path().is_dir() || !path_predicate(entry.path()) {
            continue;
        }
        dest.send(entry.into_path()).unwrap()
    }
    Ok(())
}

/// Prepare a header for inclusion in a particular file syntax by wrapping it with
/// comment characters as per the provided `delim`.
///
/// Trailing whitespace will be removed to avoid linters disliking the resulting text.
fn wrap_header(orig_header: &str, delim: HeaderDelimiters) -> String {
    let mut out = String::new();
    if !delim.first_line.is_empty() {
        out.push_str(delim.first_line);
        out.push('\n');
    }
    // assumes header uses \n
    for line in orig_header.split('\n') {
        out.push_str(delim.content_line_prefix);
        out.push_str(line);
        // Remove any trailing whitespaces (excluding newlines) from `content_line_prefix + line`.
        // For example, if `content_line_prefix` is `// ` and `line` is empty, the resulting string
        // should be truncated to `//`.
        out.truncate(out.trim_end_matches([' ', '\t']).len());
        out.push('\n');
    }
    if !delim.last_line.is_empty() {
        out.push_str(delim.last_line);
        out.push('\n');
    }
    out
}

/// Returns the header prefix line, content line prefix, and suffix line for the extension of the
/// provided path, or `None` if the extension is not recognized.
fn header_delimiters(p: &path::Path) -> Option<HeaderDelimiters> {
    match p
        .extension()
        // if the extension isn't UTF-8, oh well
        .and_then(|os_str| os_str.to_str())
        .unwrap_or("")
    {
        "c" | "h" | "gv" | "java" | "scala" | "kt" | "kts" => Some(("/*", " * ", " */")),
        "js" | "mjs" | "cjs" | "jsx" | "tsx" | "css" | "scss" | "sass" | "ts" => {
            Some(("/**", " * ", " */"))
        }
        "cc" | "cpp" | "cs" | "go" | "hcl" | "hh" | "hpp" | "m" | "mm" | "proto" | "rs"
        | "swift" | "dart" | "groovy" | "v" | "sv" => Some(("", "// ", "")),
        "py" | "sh" | "yaml" | "yml" | "dockerfile" | "rb" | "gemfile" | "tcl" | "tf" | "bzl"
        | "pl" | "pp" | "build" => Some(("", "# ", "")),
        "el" | "lisp" => Some(("", ";; ", "")),
        "erl" => Some(("", "% ", "")),
        "hs" | "lua" | "sql" | "sdl" => Some(("", "-- ", "")),
        "html" | "xml" | "vue" | "wxi" | "wxl" | "wxs" => Some(("<!--", " ", "-->")),
        "php" => Some(("", "// ", "")),
        "ml" | "mli" | "mll" | "mly" => Some(("(**", "   ", "*)")),
        // also handle whole filenames if extensions didn't match
        _ => match p
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("")
        {
            "Dockerfile" => Some(("", "# ", "")),
            _ => None,
        },
    }
    .map(
        |(first_line, content_line_prefix, last_line)| HeaderDelimiters {
            first_line,
            content_line_prefix,
            last_line,
        },
    )
}

/// Delimiters to use around and inside a header for a particular file syntax.
#[derive(Clone, Copy)]
struct HeaderDelimiters {
    /// Line to prepend before the header
    first_line: &'static str,
    /// Prefix before each line of the header itself
    content_line_prefix: &'static str,
    /// Line to append after the header
    last_line: &'static str,
}

/// Magic first lines that we need to check for before adding the license text to a file
const MAGIC_FIRST_LINES: [&str; 8] = [
    "#!",                       // shell script
    "<?xml",                    // XML declaratioon
    "<!doctype",                // HTML doctype
    "# encoding:",              // Ruby encoding
    "# frozen_string_literal:", // Ruby interpreter instruction
    "<?php",                    // PHP opening tag
    "# escape", // Dockerfile directive https://docs.docker.com/engine/reference/builder/#parser-directives
    "# syntax", // Dockerfile directive https://docs.docker.com/engine/reference/builder/#parser-directives
];

/// Apply `operation` to each discovered path in `root` that passes `path_predicate`.
///
/// Return the paths for which `operation` took action, as indicated by `operation` returning
/// `true`.
fn recursive_optional_operation<E>(
    root: &path::Path,
    path_predicate: impl Fn(&path::Path) -> bool,
    operation: impl Fn(&path::Path) -> Result<bool, E>,
) -> Result<Vec<path::PathBuf>, E>
where
    E: From<walkdir::Error>,
{
    let (path_tx, path_rx) = crossbeam::channel::unbounded::<path::PathBuf>();
    find_files(root, path_predicate, path_tx)?;
    path_rx
        .into_iter()
        // keep the paths for which the operation took action, and the errors
        .filter_map(|p| match operation(&p) {
            Ok(operation_applied) => {
                if operation_applied {
                    Some(Ok(p))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<Vec<_>, _>>()
}
