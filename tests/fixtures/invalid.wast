(assert_invalid
  (module (func (result i32) i64.const 0))
  "type mismatch")
(assert_invalid
  (module (func (result i32) i32.const 0 i64.eqz))
  "type mismatch")
(assert_invalid
  (module (func (local i32) i64.const 0 local.set 0))
  "type mismatch")
(assert_malformed
  (module quote "(module (func")
  "unexpected end")
(assert_malformed
  (module binary "\00asm\01\00\00\00\01")
  "unexpected end")
