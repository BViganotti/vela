//! Mutation fuzzing for the text parser and `.wast` scripts. Parsing untrusted
//! text must never panic, and malformed scripts must produce a report.

use vela_core::spec::run_wast;
use vela_core::text::parse_wat;
use vela_core::validate::validate_module;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

const ALPHABET: &[u8] = b"()\"$abcXYZ0123456789 _;\\\n.,+-*/";

const BASE: &str = r#"
(module
  (memory 1)
  (global $g (mut i32) (i32.const 3))
  (func $f (export "f") (param $x i32) (result i32)
    (local $y i32)
    local.get $x
    i32.const 2
    i32.mul
    local.set $y
    block (result i32)
      local.get $y
      global.get $g
      i32.add
    end))
"#;

fn mutate(rng: &mut Rng, base: &str) -> String {
    let mut bytes = base.as_bytes().to_vec();
    match rng.below(4) {
        0 => {
            for _ in 0..=rng.below(6) {
                if bytes.is_empty() {
                    break;
                }
                let i = rng.below(bytes.len());
                bytes[i] = ALPHABET[rng.below(ALPHABET.len())];
            }
        }
        1 => {
            let n = rng.below(bytes.len() + 1);
            bytes.truncate(n);
        }
        2 => {
            let at = rng.below(bytes.len() + 1);
            for _ in 0..=rng.below(6) {
                let b = ALPHABET[rng.below(ALPHABET.len())];
                bytes.insert(at.min(bytes.len()), b);
            }
        }
        _ => {
            if !bytes.is_empty() {
                let at = rng.below(bytes.len());
                let end = (at + 1 + rng.below(4)).min(bytes.len());
                bytes.drain(at..end);
            }
        }
    }
    // Replace invalid UTF-8 introduced by byte splicing.
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn thousand_mutated_wat_inputs_never_panic() {
    let mut rng = Rng(0x1234_5678_9abc_def0);
    let mut parsed = 0usize;
    for _ in 0..1000 {
        let src = mutate(&mut rng, BASE);
        if let Ok(module) = parse_wat(&src) {
            parsed += 1;
            let _ = validate_module(&module);
        }
    }
    assert!(parsed > 0, "expected some mutations to remain parseable");
}

#[test]
fn malformed_wast_scripts_report_instead_of_panicking() {
    let fixture = include_str!("../../../tests/fixtures/basic.wast");
    let mut rng = Rng(0x0f0f_1234_5678_9abc);
    for _ in 0..500 {
        let src = mutate(&mut rng, fixture);
        let report = run_wast("mutated.wast", &src);
        // A malformed script may fail assertions, but must always produce a
        // report and never panic.
        assert_eq!(report.total, report.passed + report.failed);
    }
}
