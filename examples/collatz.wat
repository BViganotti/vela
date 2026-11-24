(module
  (func (export "steps") (param $n i32) (result i32)
    (local $steps i32)
    block $done
      loop $loop
        local.get $n
        i32.const 1
        i32.eq
        br_if $done
        ;; n = if n odd then 3n+1 else n/2
        local.get $n
        i32.const 1
        i32.and
        if
          local.get $n
          i32.const 3
          i32.mul
          i32.const 1
          i32.add
          local.set $n
        else
          local.get $n
          i32.const 1
          i32.shr_u
          local.set $n
        end
        local.get $steps
        i32.const 1
        i32.add
        local.set $steps
        br $loop
      end
    end
    local.get $steps))
