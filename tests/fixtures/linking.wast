(module $lib
  (func (export "seven") (result i32) i32.const 7)
  (global (export "answer") i32 (i32.const 42)))
(register "lib" $lib)
(module
  (import "lib" "seven" (func $seven (result i32)))
  (import "lib" "answer" (global $answer i32))
  (func (export "run") (result i32)
    call $seven
    global.get $answer
    i32.add))
(assert_return (invoke "run") (i32.const 49))
(assert_unlinkable
  (module (import "missing" "f" (func)))
  "unknown import")
