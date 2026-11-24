;; Core arithmetic, control flow, globals and memory.
(module
  (memory (export "mem") 1 2)
  (global $g (export "g") i32 (i32.const 42))
  (func $add (export "add") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add)
  (func $fac (export "fac") (param $n i32) (result i32)
    (local $acc i32)
    i32.const 1
    local.set $acc
    block $done
      loop $again
        local.get $n
        i32.eqz
        br_if $done
        local.get $acc
        local.get $n
        i32.mul
        local.set $acc
        local.get $n
        i32.const 1
        i32.sub
        local.set $n
        br $again
      end
    end
    local.get $acc)
  (func (export "store8") (param i32 i32)
    local.get 0
    local.get 1
    i32.store8)
  (func (export "load8") (param i32) (result i32)
    local.get 0
    i32.load8_u)
  (data (i32.const 8) "vela"))

(assert_return (invoke "add" (i32.const 2) (i32.const 3)) (i32.const 5))
(assert_return (invoke "add" (i32.const -1) (i32.const 1)) (i32.const 0))
(assert_return (invoke "fac" (i32.const 5)) (i32.const 120))
(assert_return (invoke "fac" (i32.const 0)) (i32.const 1))
(assert_return (get "g") (i32.const 42))
(assert_return (invoke "load8" (i32.const 8)) (i32.const 118))
(assert_return (invoke "store8" (i32.const 100) (i32.const 7)))
(assert_return (invoke "load8" (i32.const 100)) (i32.const 7))
(assert_return (invoke "load8" (i32.const 10)) (i32.const 108))
