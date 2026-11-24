(module
  (func (export "nan32") (result f32) f32.const nan)
  (func (export "nan64") (result f64) f64.const nan)
  (func (export "nearest") (result f64) f64.const 2.5 f64.nearest)
  (func (export "min0") (result f64) f64.const -0.0 f64.const 0.0 f64.min)
  (func (export "copysign") (result f64) f64.const 3.0 f64.const -0.0 f64.copysign))

(assert_return (invoke "nan32") (f32.const nan:canonical))
(assert_return (invoke "nan64") (f64.const nan:canonical))
(assert_return (invoke "nearest") (f64.const 2))
(assert_return (invoke "min0") (f64.const -0))
(assert_return (invoke "copysign") (f64.const -3))
(module
  (func $f (export "f") (result i32) call $f))
(assert_exhaustion (invoke "f") "call stack exhausted")
