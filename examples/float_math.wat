(module
  (func (export "math") (result f64)
    f64.const 81
    f64.sqrt
    f64.const 2.5
    f64.nearest
    f64.add))
