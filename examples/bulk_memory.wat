(module
  (memory (export "memory") 1)
  (data "ABCD")
  (func (export "bulk") (result i32)
    i32.const 10
    i32.const 0
    i32.const 4
    memory.init 0
    i32.const 20
    i32.const 10
    i32.const 4
    memory.copy
    i32.const 30
    i32.const 122
    i32.const 4
    memory.fill
    data.drop 0
    ;; (30 == 'z') + (23 == 'D')
    i32.const 30
    i32.load8_u
    i32.const 23
    i32.load8_u
    i32.add))
