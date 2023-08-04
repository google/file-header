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

use file_header::*;
use std::{fs, path};

#[test]
fn single_line_checker_finds_header_when_present() {
    let input = r#"foo
    some license
    bar"#;
    assert!(test_checker().check(&mut input.as_bytes()).unwrap())
}

#[test]
fn single_line_checker_doesnt_find_header_when_missing() {
    let input = r#"foo
    wrong license
    bar"#;
    assert!(!test_checker().check(&mut input.as_bytes()).unwrap())
}

#[test]
fn adds_header_with_empty_delimiters() {
    let file = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
    fs::write(file.path(), r#"not a license"#).unwrap();
    test_header().add_header_if_missing(file.path()).unwrap();
    assert_eq!(
        "// some license etc etc etc

not a license",
        fs::read_to_string(file.path()).unwrap()
    );
}

#[test]
fn adds_header_with_nonempty_delimiters() {
    let file = tempfile::Builder::new().suffix(".c").tempfile().unwrap();
    fs::write(file.path(), r#"not a license"#).unwrap();
    test_header().add_header_if_missing(file.path()).unwrap();
    assert_eq!(
        "/*
 * some license etc etc etc
 */

not a license",
        fs::read_to_string(file.path()).unwrap()
    );
}

#[test]
fn adds_header_trim_trailing_whitespace() {
    let file = tempfile::Builder::new().suffix(".c").tempfile().unwrap();
    fs::write(file.path(), r#"not a license"#).unwrap();
    test_header_with_blank_lines_and_trailing_whitespace()
        .add_header_if_missing(file.path())
        .unwrap();
    assert_eq!(
        "/*
 * some license
 * line with trailing whitespace.
 *
 * etc
 */

not a license",
        fs::read_to_string(file.path()).unwrap()
    );
}

#[test]
fn doesnt_add_header_when_already_present() {
    let file = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
    let initial_content = r#"
    // some license etc etc etc already present
    not a license"#;
    fs::write(file.path(), initial_content).unwrap();
    test_header().add_header_if_missing(file.path()).unwrap();
    assert_eq!(initial_content, fs::read_to_string(file.path()).unwrap());
}

#[test]
fn adds_header_after_magic_first_line() {
    let file = tempfile::Builder::new().suffix(".xml").tempfile().unwrap();
    fs::write(
        file.path(),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<root />
"#,
    )
    .unwrap();
    test_header().add_header_if_missing(file.path()).unwrap();
    assert_eq!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!--
 some license etc etc etc
-->

<root />
"#,
        fs::read_to_string(file.path()).unwrap()
    );
}

#[test]
fn check_recursively() {
    let header = test_header();
    let mut root = path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("resources/test/example_check");
    let results = check_headers_recursively(&root, |_p| true, header, 4).unwrap();
    assert_eq!(
        vec![path::PathBuf::from("no_header.rs")],
        results
            .no_header_files
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().to_path_buf())
            .collect::<Vec<_>>()
    );
}

#[test]
fn add_recursively() {
    let header = test_header();

    let root = tempfile::tempdir().unwrap();

    let mut no_header = root.path().to_path_buf();
    no_header.push("no_header.rs");
    fs::write(&no_header, "// no header\n").unwrap();

    let mut with_header = root.path().to_path_buf();
    with_header.push("with_header.rs");
    let mut contents = "some license etc etc etc".to_string();
    contents.push_str("\n// has a header\n");
    fs::write(&with_header, &contents).unwrap();

    assert_eq!(
        vec![path::PathBuf::from("no_header.rs")],
        add_headers_recursively(root.path(), |_| true, header)
            .map(|paths| paths
                .iter()
                .map(|p| p.strip_prefix(&root).unwrap().to_path_buf())
                .collect::<Vec<_>>())
            .unwrap()
    );
}

fn test_checker() -> SingleLineChecker {
    SingleLineChecker::new("some license".to_string(), 100)
}

fn test_header() -> Header<SingleLineChecker> {
    Header::new(test_checker(), r#"some license etc etc etc"#.to_string())
}
fn test_header_with_blank_lines_and_trailing_whitespace() -> Header<SingleLineChecker> {
    Header::new(
        test_checker(),
        "some license\nline with trailing whitespace.  \n\netc".to_string(),
    )
}
