use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;



#[test]
fn error_on_conflicting_flags() {
    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "-r", "--no-path", "."])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Cannot use --relative and --no-path together"));
}

#[test]
fn shows_help_with_help_flag() {
    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("cxt"));
}

#[test]
fn shows_version_with_version_flag() {
    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "--version"])
        .assert()
        .success()
        .stdout(predicates::str::contains("cxt"));
}

#[test]
fn error_on_nonexistent_file() {
    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "nonexistent_file.txt"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Path does not exist"));
}

#[test]
fn error_on_nonexistent_directory() {
    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "nonexistent_directory/"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Path does not exist"));
}

#[test]
fn prints_content_to_stdout() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "-p", file_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("Hello, World!"));
}

#[test]
fn prints_content_without_headers() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "-n", "-p", file_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("Hello, World!"))
        .stdout(predicates::str::contains("--- File:").not());
}

#[test]
fn writes_content_to_file() {
    let dir = tempdir().unwrap();
    let input_file = dir.path().join("input.txt");
    let output_file = dir.path().join("output.txt");
    fs::write(&input_file, "Test content").unwrap();

    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "-w", output_file.to_str().unwrap(), input_file.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("Test content"));
}

#[test]
fn handles_wildcard_patterns() {
    let dir = tempdir().unwrap();
    let file1 = dir.path().join("test1.py");
    let file2 = dir.path().join("test2.py");
    let file3 = dir.path().join("test.txt");
    
    fs::write(&file1, "Python file 1").unwrap();
    fs::write(&file2, "Python file 2").unwrap();
    fs::write(&file3, "Text file").unwrap();

    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "-p", &format!("{}/*.py", dir.path().to_str().unwrap())])
        .assert()
        .success()
        .stdout(predicates::str::contains("Python file 1"))
        .stdout(predicates::str::contains("Python file 2"))
        .stdout(predicates::str::contains("Text file").not());
}

#[test]
fn handles_nested_wildcard_patterns() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    
    let file1 = subdir.join("app.cpp");
    let file2 = subdir.join("main.cpp");
    let file3 = subdir.join("helper.h");
    
    fs::write(&file1, "C++ app file").unwrap();
    fs::write(&file2, "C++ main file").unwrap();
    fs::write(&file3, "Header file").unwrap();

    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", "-p", &format!("{}/*/*.cpp", dir.path().to_str().unwrap())])
        .assert()
        .success()
        .stdout(predicates::str::contains("C++ app file"))
        .stdout(predicates::str::contains("C++ main file"))
        .stdout(predicates::str::contains("Header file").not());
}

#[test]
fn handles_no_matching_files() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.txt");
    fs::write(&file, "Test content").unwrap();

    let mut cmd = Command::cargo_bin("cxt").unwrap();
    cmd.args(["--ci", &format!("{}/*.nonexistent", dir.path().to_str().unwrap())])
        .assert()
        .success()
        .stdout(predicates::str::contains("No files found matching the specified patterns"));
}
