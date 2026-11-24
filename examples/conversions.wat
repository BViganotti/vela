(module
  (func (export "convert") (result i64)
    f64.const 3.9
    i32.trunc_f64_s
    i64.extend_i32_u
    i32.const -1
    i64.extend_i32_u
    i64.add))
