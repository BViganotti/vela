(module
  (memory (export "memory") 1)
  (func (export "fill") (result i32)
    i32.const 32
    i32.const 171
    i32.const 4
    memory.fill
    i32.const 35
    i32.load8_u))
