//! Deterministic mutation fuzzing for the binary decoder, validator and
//! instantiation path. 1,000 mutated modules must fail cleanly, never panic.

use vela_core::binary::{decode_module, encode_module};
use vela_core::runtime::{instantiate, Registry, Store};
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
    fn byte(&mut self) -> u8 {
        (self.next() & 0xff) as u8
    }
}

fn seed_module() -> Vec<u8> {
    let wat = r#"
    (module
      (type (func (param i32) (result i32)))
      (memory 1 2)
      (table 1 funcref)
      (global (mut i32) (i32.const 7))
      (func $f (export "f") (param $x i32) (result i32)
        (local $y i32)
        local.get $x
        local.get $x
        i32.mul
        local.set $y
        block (result i32)
          local.get $y
          i32.const 3
          i32.add
        end)
      (elem (i32.const 0) $f)
      (data (i32.const 0) "seed"))
    "#;
    let module = parse_wat(wat).expect("seed parses");
    encode_module(&module)
}

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    match rng.below(5) {
        0 => {
            // Bit flips.
            for _ in 0..=rng.below(8) {
                if out.is_empty() {
                    break;
                }
                let i = rng.below(out.len());
                out[i] ^= 1 << rng.below(8);
            }
        }
        1 => {
            // Truncation.
            let n = rng.below(out.len() + 1);
            out.truncate(n);
        }
        2 => {
            // Insertion.
            let at = rng.below(out.len() + 1);
            let n = 1 + rng.below(8);
            for _ in 0..n {
                let b = rng.byte();
                out.insert(at.min(out.len()), b);
            }
        }
        3 => {
            // Deletion.
            if !out.is_empty() {
                let at = rng.below(out.len());
                let n = 1 + rng.below(4);
                let end = (at + n).min(out.len());
                out.drain(at..end);
            }
        }
        _ => {
            // Wholesale random bytes with a valid header sometimes.
            let n = rng.below(256);
            out = (0..n).map(|_| rng.byte()).collect();
            if rng.below(2) == 0 {
                let mut headed = b"\0asm\x01\0\0\0".to_vec();
                headed.extend_from_slice(&out);
                out = headed;
            }
        }
    }
    out
}

#[test]
fn thousand_mutated_modules_never_panic() {
    let base = seed_module();
    let mut rng = Rng(0x5eed_1234_9876_abcd);
    let mut decoded = 0usize;
    let mut validated = 0usize;
    let mut instantiated = 0usize;
    for _ in 0..1000 {
        let bytes = mutate(&mut rng, &base);
        let module = match decode_module(&bytes) {
            Ok(m) => {
                decoded += 1;
                m
            }
            Err(_) => continue,
        };
        if validate_module(&module).is_err() {
            continue;
        }
        validated += 1;
        // Avoid spending unbounded time on fuzzed start functions.
        let instrs: usize = module.funcs.iter().map(|f| f.body.body.len()).sum();
        if module.start.is_none() && instrs < 5_000 {
            let mut store = Store::new();
            match instantiate(&mut store, module, &Registry::new()) {
                Ok(_) => instantiated += 1,
                Err(_) => {}
            }
        }
    }
    assert!(decoded > 0, "expected at least one mutated module to decode");
    assert!(validated > 0, "expected at least one mutated module to validate");
    assert!(instantiated > 0, "expected at least one mutated module to instantiate");
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0xdead_beef_0bad_f00d);
    for _ in 0..1000 {
        let n = rng.below(512);
        let bytes: Vec<u8> = (0..n).map(|_| rng.byte()).collect();
        let _ = decode_module(&bytes);
    }
}
