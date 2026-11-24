//! Compiled-in assets: default module, fixtures and the Studio UI.

pub const DEFAULT_MODULE: &str = r#"
(module
  ;; Vela Studio default workload: a loop that writes memory, updates a
  ;; global, and calls a recursive function.
  (memory (export "memory") 1 4)
  (global $counter (export "counter") (mut i32) (i32.const 0))

  (func $fib (export "fib") (param $n i32) (result i32)
    (if (result i32)
      (i32.lt_s (local.get $n) (i32.const 2))
      (then (local.get $n))
      (else
        (i32.add
          (call $fib (i32.sub (local.get $n) (i32.const 1)))
          (call $fib (i32.sub (local.get $n) (i32.const 2)))))))

  (func (export "main") (param $n i32) (result i32)
    (local $i i32)
    (local $sum i32)
    block $done
      loop $loop
        local.get $i
        local.get $n
        i32.ge_u
        br_if $done
        ;; memory[i * 16] = i
        local.get $i
        i32.const 16
        i32.mul
        local.get $i
        i32.store
        ;; sum += i + (i + 1) * i
        local.get $sum
        local.get $i
        i32.add
        local.get $i
        i32.const 1
        i32.add
        local.get $i
        i32.mul
        i32.add
        local.set $sum
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop
      end
    end
    global.get $counter
    i32.const 1
    i32.add
    global.set $counter
    local.get $sum
    i32.const 5
    call $fib
    i32.add))
"#;

pub const INDEX_HTML: &str = include_str!("../../../assets/index.html");
pub const APP_JS: &str = include_str!("../../../assets/app.js");
pub const STYLE_CSS: &str = include_str!("../../../assets/style.css");

pub const FIXTURES: &[(&str, &str)] = &[
    (
        "basic.wast",
        include_str!("../../../tests/fixtures/basic.wast"),
    ),
    (
        "traps.wast",
        include_str!("../../../tests/fixtures/traps.wast"),
    ),
    (
        "linking.wast",
        include_str!("../../../tests/fixtures/linking.wast"),
    ),
    (
        "invalid.wast",
        include_str!("../../../tests/fixtures/invalid.wast"),
    ),
    (
        "floats.wast",
        include_str!("../../../tests/fixtures/floats.wast"),
    ),
    (
        "tables.wast",
        include_str!("../../../tests/fixtures/tables.wast"),
    ),
];
