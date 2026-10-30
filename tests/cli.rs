use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn displays_help_without_network_access() {
    Command::cargo_bin("extractrs")
        .expect("binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--batch"))
        .stdout(predicate::str::contains("--metadata-only"))
        .stdout(predicate::str::contains("--chunk-size"))
        .stdout(predicate::str::contains("--schema"));
}

#[test]
fn requires_a_url_or_batch_file() {
    Command::cargo_bin("extractrs")
        .expect("binary should build")
        .assert()
        .failure()
        .stderr(predicate::str::contains("provide at least one URL"));
}

#[test]
fn rejects_overlap_that_is_not_smaller_than_chunk_size_before_fetching() {
    Command::cargo_bin("extractrs")
        .expect("binary should build")
        .args([
            "https://example.com",
            "--chunk-size",
            "10",
            "--overlap",
            "10",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--overlap must be smaller than --chunk-size",
        ));
}

#[test]
fn rejects_invalid_schema_before_fetching() {
    Command::cargo_bin("extractrs")
        .expect("binary should build")
        .args(["https://example.com", "--schema", r#"{"price":"currency"}"#])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported type 'currency'"));
}
