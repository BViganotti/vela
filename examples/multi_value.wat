(module
  (func (export "pair") (param $x i32) (result i32 i64)
    local.get $x
    local.get $x
    i64.extend_i32_u
    i64.const 100
    i64.mul))
