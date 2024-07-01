(module
  (func $fib (param $n i64) (result i64)
    (local $a i64)
    (local $b i64)
    (local $t i64)
    i64.const 1
    local.set $a
    i64.const 1 
    local.set $b
    (loop $loop
      local.get $n
      i64.const 1
      i64.sub  
      local.tee $n
      i64.const 0
      i64.ne
      if 
        local.get $a
        local.set $t
        local.get $b
        local.set $a
        local.get $a
        local.get $t
        i64.add
        local.set $b
        br $loop
      end
    )
    local.get $b
  )
  (export "fib" (func $fib))
)