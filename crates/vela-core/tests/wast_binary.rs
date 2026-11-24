//! `.wast` modules supplied as binary or quoted text.

use vela_core::binary::encode_module;
use vela_core::spec::run_wast;
use vela_core::text::parse_wat;

fn escaped_module_bytes() -> String {
    let module = parse_wat(
        r#"(module
             (memory 1)
             (func (export "f") (param i32) (result i32)
               local.get 0 global.get 0 drop i32.const 2 i32.mul)
             (global i32 (i32.const 0))
             (data (i32.const 0) "bin"))"#,
    )
    .unwrap();
    let bytes = encode_module(&module);
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}

#[test]
fn binary_module_in_a_script_runs() {
    let src = format!(
        "(module binary \"{}\")\n(assert_return (invoke \"f\" (i32.const 21)) (i32.const 42))\n",
        escaped_module_bytes()
    );
    let report = run_wast("binary.wast", &src);
    assert!(report.ok(), "{report}");
    assert_eq!(report.passed, 2);
}

#[test]
fn quoted_module_in_a_script_runs() {
    let src = r#"
    (module quote "(module (func (export \"f\") (result i32) i32.const 7))")
    (assert_return (invoke "f") (i32.const 7))
    "#;
    let report = run_wast("quote.wast", src);
    assert!(report.ok(), "{report}");
}

#[test]
fn malformed_binary_is_malformed_not_invalid() {
    let src = r#"
    (assert_malformed (module binary "\00asm\01\00\00") "short header")
    (assert_malformed (module binary "\00asm\01\00\00\00\ff") "bad section")
    "#;
    let report = run_wast("malformed.wast", src);
    assert!(report.ok(), "{report}");
    assert_eq!(report.passed, 2);
}

#[test]
fn binary_missing_import_is_unlinkable() {
    // Encode a module importing env.mem and assert it cannot link.
    let text = r#"(module (import "env" "mem" (memory 1)))"#;
    let module = parse_wat(text).unwrap();
    let bytes = encode_module(&module);
    let escaped: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
    let src = format!(
        "(assert_unlinkable (module binary \"{escaped}\") \"unknown import\")\n"
    );
    let report = run_wast("unlinkable.wast", &src);
    assert!(report.ok(), "{report}");
}
