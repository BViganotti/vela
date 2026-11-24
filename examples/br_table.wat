(module
  (func (export "switch") (param $i i32) (result i32)
    block $default
      block $case2
        block $case1
          block $case0
            local.get $i
            br_table $case0 $case1 $case2 $default
          end
          i32.const 100
          return
        end
        i32.const 10
        return
      end
      i32.const 20
      return
    end
    i32.const -1))
