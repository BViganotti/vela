(module
  (memory (export "memory") 1)
  (func $init
    i32.const 0
    i32.const 42
    i32.store)
  (start $init)
  (func (export "read") (result i32)
    i32.const 0
    i32.load))
