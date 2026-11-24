(module
  (table 1 funcref)
  (func (export "grow") (result i32)
    ref.null func
    i32.const 1
    table.grow 0
    drop
    table.size 0))
