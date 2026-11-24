(module
  ;; Iterative Fibonacci; deterministic and bounded.
  (func (export "fib") (param $n i32) (result i32)
    (local $a i32)
    (local $b i32)
    (local $i i32)
    i32.const 0
    local.set $a
    i32.const 1
    local.set $b
    block $done
      loop $loop
        local.get $i
        local.get $n
        i32.ge_u
        br_if $done
        local.get $a
        local.get $b
        i32.add
        local.get $b
        local.set $a
        local.set $b
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop
      end
    end
    local.get $a))
