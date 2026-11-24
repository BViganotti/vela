(module
  (func (export "choose") (param $a i32) (param $b i32) (param $cond i32) (result i32)
    local.get $a
    local.get $b
    local.get $cond
    select))
