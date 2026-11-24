//! Text diagnostics: positions and clear rejection of malformed input.

use vela_core::text::parse_wat;
use vela_core::validate::validate_module;

fn err(src: &str) -> vela_core::error::TextError {
    parse_wat(src).expect_err("expected a text error")
}

#[test]
fn unterminated_string_reports_position() {
    let e = err("(module\n  (data \"abc))");
    assert!(e.line >= 2, "{e}");
    assert!(e.message.contains("unterminated string"), "{e}");
}

#[test]
fn unbalanced_parentheses_report_positions() {
    let e = err("(module\n (func\n  i32.const 1");
    assert!(e.message.contains("unclosed"), "{e}");
    let e = err("(module))");
    assert!(e.message.contains("unexpected ')'"), "{e}");
}

#[test]
fn unknown_instruction_names_the_opcode() {
    let e = err("(module (func i32.frobnicate))");
    assert!(e.message.contains("i32.frobnicate"), "{e}");
}

#[test]
fn unknown_local_and_label_are_rejected_at_parse_time() {
    let e = err("(module (func local.get $nope))");
    assert!(e.message.contains("$nope"), "{e}");
    let e = err("(module (func block br $gone end))");
    assert!(e.message.contains("$gone"), "{e}");
}

#[test]
fn imports_must_precede_definitions() {
    let e = err("(module (func) (import \"a\" \"b\" (func)))");
    assert!(e.message.contains("import after"), "{e}");
}

#[test]
fn multiple_top_level_modules_are_rejected() {
    let e = err("(module) (module)");
    assert!(e.message.contains("exactly one module"), "{e}");
}

#[test]
fn unknown_value_types_and_limits_are_rejected() {
    assert!(parse_wat("(module (func (param i128)))").is_err());
    assert!(parse_wat("(module (memory 1 0))").is_err());
    assert!(parse_wat("(module (table 2 1 funcref))").is_err());
}

#[test]
fn duplicate_export_names_fail_validation_not_parsing() {
    let m = parse_wat("(module (func (export \"x\")) (export \"x\" (func 0)))").unwrap();
    let e = validate_module(&m).expect_err("duplicate exports");
    assert!(e.message.contains("duplicate export"), "{e}");
}
