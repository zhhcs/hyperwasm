(module
  (func $add (result i32)
    (local $i i32)  
    (local.set $i (i32.const 0))
    
    (block $done
      (loop $loop
        (local.get $i)
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br_if $done (i32.eq (local.get $i) (i32.const 10000000))) 
        (br $loop)
      )
    )
    
    (local.get $i)
  )

  (export "add" (func $add))
)