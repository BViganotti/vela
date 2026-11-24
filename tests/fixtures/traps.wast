(module
  (memory 1)
  (func (export "oob") (result i32)
    i32.const 65536
    i32.load)
  (func (export "div") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.div_s)
  (func (export "unreachable")
    unreachable)
  (func (export "trunc") (result i32)
    f64.const nan
    i32.trunc_f64_s))

(assert_return (invoke "div" (i32.const 7) (i32.const 2)) (i32.const 3))
(assert_trap (invoke "oob") "out of bounds memory access")
(assert_trap (invoke "div" (i32.const 1) (i32.const 0)) "integer divide by zero")
(assert_trap (invoke "div" (i32.const -2147483648) (i32.const -1)) "integer overflow")
(assert_trap (invoke "unreachable") "unreachable")
(assert_trap (invoke "trunc") "invalid conversion to integer")
