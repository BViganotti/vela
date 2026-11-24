(module
  (type $un (func (result i32)))
  (table 2 funcref)
  (func $one (result i32) i32.const 1)
  (func $two (result i32) i32.const 2)
  (elem (i32.const 0) $one $two)
  (func (export "pick") (param i32) (result i32)
    local.get 0
    call_indirect (type $un))
  (func (export "size") (result i32)
    table.size 0)
  (func (export "grow") (result i32)
    ref.null func
    i32.const 1
    table.grow 0))

(assert_return (invoke "pick" (i32.const 0)) (i32.const 1))
(assert_return (invoke "pick" (i32.const 1)) (i32.const 2))
(assert_return (invoke "size") (i32.const 2))
(assert_return (invoke "grow") (i32.const 2))
(assert_trap (invoke "pick" (i32.const 5)) "undefined element")
