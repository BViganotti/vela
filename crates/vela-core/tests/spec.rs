//! Run local `.wast` fixtures through the assertion runner.

use std::path::Path;
use vela_core::spec::run_wast;

fn run_file(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    let src = std::fs::read_to_string(&path).expect("fixture exists");
    let report = run_wast(name, &src);
    assert!(
        report.ok(),
        "fixture {name} failed:\n{}",
        report
            .failures
            .iter()
            .map(|f| format!("line {}: {}", f.line, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(report.total > 0, "fixture {name} has no assertions");
}

#[test]
fn basic_fixture() {
    run_file("basic.wast");
}

#[test]
fn traps_fixture() {
    run_file("traps.wast");
}

#[test]
fn linking_fixture() {
    run_file("linking.wast");
}

#[test]
fn invalid_fixture() {
    run_file("invalid.wast");
}

#[test]
fn floats_fixture() {
    run_file("floats.wast");
}

#[test]
fn tables_fixture() {
    run_file("tables.wast");
}
