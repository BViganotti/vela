(module
  (func (export "bits") (result i32)
    i32.const 0x0f0f0f0f
    i32.popcnt
    i32.const 0x80000001
    i32.const 1
    i32.rotl
    i32.add))
