//! `.wast` assertion variants and honest failure reporting.

use vela_core::spec::{run_wast, Command, ModuleSource};

fn run(src: &str) -> vela_core::spec::WastReport {
    run_wast("case.wast", src)
}

#[test]
fn nan_assertions_accept_canonical_and_arithmetic() {
    let src = r#"
    (module
      (func (export "n32") (result f32) f32.const nan)
      (func (export "n64") (result f64) f64.const nan))
    (assert_return (invoke "n32") (f32.const nan:canonical))
    (assert_return (invoke "n32") (f32.const nan:arithmetic))
    (assert_return (invoke "n64") (f64.const nan:canonical))
    (assert_return_arithmetic_nan (invoke "n64"))
    (assert_return_canonical_nan (invoke "n32"))
    "#;
    let report = run(src);
    assert!(report.ok(), "{report}");
}

#[test]
fn exact_float_values_are_compared() {
    let src = r#"
    (module (func (export "pi") (result f64) f64.const 3.141592653589793))
    (assert_return (invoke "pi") (f64.const 3.141592653589793))
    "#;
    assert!(run(src).ok());
    let bad = r#"
    (module (func (export "x") (result f64) f64.const 1.5))
    (assert_return (invoke "x") (f64.const 1.5000001))
    "#;
    let report = run(bad);
    assert!(!report.ok());
    assert_eq!(report.failed, 1);
}

#[test]
fn invalid_and_malformed_are_distinguished() {
    let invalid = r#"
    (assert_invalid (module (func (result i32) i64.const 0)) "type mismatch")
    "#;
    assert!(run(invalid).ok());
    let malformed = r#"
    (assert_malformed (module quote "(module (func") "unclosed")
    "#;
    assert!(run(malformed).ok());
    // A well-formed module must not satisfy assert_malformed.
    let wrong = r#"
    (assert_malformed (module (func)) "not malformed")
    "#;
    assert!(!run(wrong).ok());
}

#[test]
fn trap_assertions_require_a_trap() {
    let src = r#"
    (module
      (func (export "ok") (result i32) i32.const 1)
      (func (export "bad") (result i32) unreachable))
    (assert_trap (invoke "bad") "unreachable")
    "#;
    assert!(run(src).ok());
    let wrong = r#"
    (module (func (export "ok") (result i32) i32.const 1))
    (assert_trap (invoke "ok") "unreachable")
    "#;
    assert!(!run(wrong).ok());
}

#[test]
fn get_and_register_work() {
    let src = r#"
    (module $m
      (global (export "g") i32 (i32.const 7))
      (func (export "f") (result i32) global.get 0))
    (register "lib" $m)
    (assert_return (get "g") (i32.const 7))
    (module
      (import "lib" "f" (func $f (result i32)))
      (func (export "call") (result i32) call $f))
    (assert_return (invoke "call") (i32.const 7))
    "#;
    let report = run(src);
    assert!(report.ok(), "{report}");
}

#[test]
fn report_display_lists_failures_with_lines() {
    let report = run("(module)\n(assert_return (invoke \"missing\") (i32.const 0))\n");
    assert!(!report.ok());
    assert_eq!(report.failed, 1);
    let text = report.to_string();
    assert!(text.contains("1/2"), "{text}");
    assert!(text.contains("line 2"), "{text}");
}

#[test]
fn script_parser_supports_binary_modules() {
    let bytes = b"\0asm\x01\0\0\0";
    let mut literal = String::from("(module binary \"");
    for b in bytes {
        literal.push_str(&format!("\\{b:02x}"));
    }
    literal.push_str("\")\n");
    let commands = vela_core::spec::runner::parse_script(&literal).unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0].1 {
        Command::Module {
            source: ModuleSource::Binary(b),
            ..
        } => assert_eq!(b, bytes),
        other => panic!("unexpected command {other:?}"),
    }
}
