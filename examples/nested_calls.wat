(module
  (func $double (param $x i32) (result i32)
    local.get $x
    i32.const 2
    i32.mul)
  (func (export "quad") (param $x i32) (result i32)
    local.get $x
    call $double
    call $double))
