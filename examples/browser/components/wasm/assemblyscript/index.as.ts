let clicks: i32 = 0
let output: ArrayBuffer = new ArrayBuffer(0)

function writeState(label: string): void {
  output = String.UTF8.encode(
    '{"state":{"clicks":' + clicks.toString() + ',"label":"' + label + '"}}',
    true
  )
}

function readClicks(payloadPtr: i32, payloadLen: i32): i32 {
  const payload = String.UTF8.decodeUnsafe(payloadPtr, payloadLen)
  const marker = '"clicks":'
  let cursor = payload.indexOf(marker)
  if (cursor < 0) return 0
  cursor += marker.length
  let value: i32 = 0
  while (cursor < payload.length) {
    const code = payload.charCodeAt(cursor)
    if (code < 48 || code > 57) break
    value = value * 10 + (code - 48)
    cursor += 1
  }
  return value
}

export function alloc(size: i32): i32 {
  return heap.alloc(size)
}

export function dealloc(ptr: i32, len: i32): void {
  heap.free(ptr)
}

export function init(ptr: i32, len: i32): void {
  clicks = 0
  writeState('AS')
}

export function call(methodPtr: i32, methodLen: i32, payloadPtr: i32, payloadLen: i32): void {
  clicks = readClicks(payloadPtr, payloadLen) + 1
  writeState('AS')
}

export function output_ptr(): i32 {
  return changetype<i32>(output)
}

export function output_len(): i32 {
  return output.byteLength - 1
}
