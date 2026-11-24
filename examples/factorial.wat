(module
  (func (export "fact") (param $n i32) (result i32)
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
    local.get $acc))
