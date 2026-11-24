(module
  (func $gcd (export "gcd") (param $a i32) (param $b i32) (result i32)
    block $done
      loop $loop
        local.get $b
        i32.eqz
        br_if $done
        local.get $a
        local.get $b
        i32.rem_u
        local.get $b
        local.set $a
        local.set $b
        br $loop
      end
    end
    local.get $a))
