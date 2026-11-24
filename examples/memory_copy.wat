(module
  (memory (export "memory") 1)
  (data (i32.const 0) "hello")
  (func (export "copy") (result i32)
    (local $i i32)
    block $done
      loop $loop
        local.get $i
        i32.const 5
        i32.ge_u
        br_if $done
        ;; memory[100 + i] = memory[i]
        local.get $i
        i32.const 100
        i32.add
        local.get $i
        i32.load8_u
        i32.store8
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop
      end
    end
    i32.const 100
    i32.load))
