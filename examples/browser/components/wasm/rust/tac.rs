#![no_std]

static mut INPUT: [u8; 2048] = [0; 2048];
static mut OUTPUT: [u8; 128] = [0; 128];
static mut OUTPUT_LEN: usize = 0;
static mut CLICKS: u32 = 0;

const PREFIX: &[u8] = br#"{"state":{"clicks":"#;
const SUFFIX: &[u8] = br#","label":"Rust"}}"#;

unsafe fn write_state() {
    let mut pos = 0;
    for byte in PREFIX {
        OUTPUT[pos] = *byte;
        pos += 1;
    }
    pos = write_u32(CLICKS, pos);
    for byte in SUFFIX {
        OUTPUT[pos] = *byte;
        pos += 1;
    }
    OUTPUT_LEN = pos;
}

unsafe fn read_clicks(ptr: *const u8, len: usize) -> u32 {
    let payload = core::slice::from_raw_parts(ptr, len);
    let marker = b"\"clicks\":";
    let mut index = 0;
    while index + marker.len() <= payload.len() {
        if &payload[index..index + marker.len()] == marker {
            let mut cursor = index + marker.len();
            let mut value = 0u32;
            while cursor < payload.len() {
                let byte = payload[cursor];
                if !(b'0'..=b'9').contains(&byte) {
                    break;
                }
                value = value * 10 + u32::from(byte - b'0');
                cursor += 1;
            }
            return value;
        }
        index += 1;
    }
    0
}

unsafe fn write_u32(value: u32, mut pos: usize) -> usize {
    let mut buffer = [0u8; 10];
    let mut cursor = buffer.len();
    let mut current = value;
    loop {
        cursor -= 1;
        buffer[cursor] = b'0' + (current % 10) as u8;
        current /= 10;
        if current == 0 {
            break;
        }
    }
    while cursor < buffer.len() {
        OUTPUT[pos] = buffer[cursor];
        pos += 1;
        cursor += 1;
    }
    pos
}

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    if size > unsafe { INPUT.len() } {
        core::ptr::null_mut()
    } else {
        unsafe { INPUT.as_mut_ptr() }
    }
}

#[no_mangle]
pub extern "C" fn dealloc(_ptr: *mut u8, _len: usize) {}

#[no_mangle]
pub extern "C" fn init(_ptr: *const u8, _len: usize) {
    unsafe {
        CLICKS = 0;
        write_state();
    }
}

#[no_mangle]
pub extern "C" fn call(_method_ptr: *const u8, _method_len: usize, payload_ptr: *const u8, payload_len: usize) {
    unsafe {
        CLICKS = read_clicks(payload_ptr, payload_len) + 1;
        write_state();
    }
}

#[no_mangle]
pub extern "C" fn output_ptr() -> usize {
    unsafe { OUTPUT.as_ptr() as usize }
}

#[no_mangle]
pub extern "C" fn output_len() -> usize {
    unsafe { OUTPUT_LEN }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
