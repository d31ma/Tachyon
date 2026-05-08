(module
  (memory (export "memory") 1)
  (global $clicks (mut i32) (i32.const 0))
  (global $out_ptr (mut i32) (i32.const 1024))
  (global $out_len (mut i32) (i32.const 0))

  (data (i32.const 64) "{\"state\":{\"clicks\":")
  (data (i32.const 96) ",\"label\":\"WAT\"}}")

  (func (export "alloc") (param $size i32) (result i32)
    (i32.const 2048))

  (func (export "dealloc") (param $ptr i32) (param $len i32))

  (func $write_state
    (local $pos i32)
    (local $n i32)
    (memory.copy (i32.const 1024) (i32.const 64) (i32.const 19))
    (local.set $pos (i32.const 1043))
    (local.set $n (global.get $clicks))

    (if (i32.ge_u (local.get $n) (i32.const 100))
      (then
        (i32.store8
          (local.get $pos)
          (i32.add (i32.const 48) (i32.div_u (local.get $n) (i32.const 100))))
        (local.set $pos (i32.add (local.get $pos) (i32.const 1)))
        (local.set $n (i32.rem_u (local.get $n) (i32.const 100)))))

    (if (i32.ge_u (global.get $clicks) (i32.const 10))
      (then
        (i32.store8
          (local.get $pos)
          (i32.add (i32.const 48) (i32.div_u (local.get $n) (i32.const 10))))
        (local.set $pos (i32.add (local.get $pos) (i32.const 1)))
        (local.set $n (i32.rem_u (local.get $n) (i32.const 10)))))

    (i32.store8
      (local.get $pos)
      (i32.add (i32.const 48) (local.get $n)))
    (local.set $pos (i32.add (local.get $pos) (i32.const 1)))
    (memory.copy (local.get $pos) (i32.const 96) (i32.const 16))
    (local.set $pos (i32.add (local.get $pos) (i32.const 16)))
    (global.set $out_ptr (i32.const 1024))
    (global.set $out_len (i32.sub (local.get $pos) (i32.const 1024))))

  (func $read_clicks (param $ptr i32) (param $len i32) (result i32)
    (local $cursor i32)
    (local $end i32)
    (local $value i32)
    (local.set $cursor (local.get $ptr))
    (local.set $end (i32.add (local.get $ptr) (local.get $len)))

    (block $not_found
      (loop $scan
        (br_if $not_found (i32.gt_u (i32.add (local.get $cursor) (i32.const 8)) (local.get $end)))
        (if
          (i32.and
            (i32.and
              (i32.and
                (i32.eq (i32.load8_u (local.get $cursor)) (i32.const 99))
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 1))) (i32.const 108)))
              (i32.and
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 2))) (i32.const 105))
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 3))) (i32.const 99))))
            (i32.and
              (i32.and
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 4))) (i32.const 107))
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 5))) (i32.const 115)))
              (i32.and
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 6))) (i32.const 34))
                (i32.eq (i32.load8_u (i32.add (local.get $cursor) (i32.const 7))) (i32.const 58)))))
          (then
            (local.set $cursor (i32.add (local.get $cursor) (i32.const 8)))
            (local.set $value (i32.const 0))
            (block $done_digits
              (loop $digits
                (br_if $done_digits (i32.ge_u (local.get $cursor) (local.get $end)))
                (br_if $done_digits (i32.lt_u (i32.load8_u (local.get $cursor)) (i32.const 48)))
                (br_if $done_digits (i32.gt_u (i32.load8_u (local.get $cursor)) (i32.const 57)))
                (local.set $value
                  (i32.add
                    (i32.mul (local.get $value) (i32.const 10))
                    (i32.sub (i32.load8_u (local.get $cursor)) (i32.const 48))))
                (local.set $cursor (i32.add (local.get $cursor) (i32.const 1)))
                (br $digits)))
            (return (local.get $value))))
        (local.set $cursor (i32.add (local.get $cursor) (i32.const 1)))
        (br $scan)))
    (i32.const 0))

  (func (export "init") (param $ptr i32) (param $len i32)
    (global.set $clicks (i32.const 0))
    (call $write_state))

  (func (export "call") (param $method_ptr i32) (param $method_len i32) (param $payload_ptr i32) (param $payload_len i32)
    (global.set $clicks (i32.add (call $read_clicks (local.get $payload_ptr) (local.get $payload_len)) (i32.const 1)))
    (call $write_state))

  (func (export "output_ptr") (result i32)
    (global.get $out_ptr))

  (func (export "output_len") (result i32)
    (global.get $out_len)))
