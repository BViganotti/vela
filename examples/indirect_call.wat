(module
  (type $un (func (result i32)))
  (table 2 funcref)
  (func $one (result i32) i32.const 1)
  (func $two (result i32) i32.const 2)
  (elem (i32.const 0) $one $two)
  (func (export "pick") (param $i i32) (result i32)
    local.get $i
    call_indirect (type $un)))
