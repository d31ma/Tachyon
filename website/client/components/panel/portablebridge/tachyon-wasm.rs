#![no_std]
#![no_main]
extern crate alloc;
use alloc::alloc::{alloc as raw_alloc, Layout};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! { loop {} }

static mut HEAP: [u8; 65536] = [0; 65536];
static mut NEXT: usize = 0;

struct Bump;
unsafe impl core::alloc::GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let start = (NEXT + layout.align() - 1) & !(layout.align() - 1);
            NEXT = start + layout.size();
            HEAP.as_mut_ptr().add(start)
        }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}
#[global_allocator]
static ALLOCATOR: Bump = Bump;

static mut COUNT: i64 = 6;

#[no_mangle]
pub extern "C" fn tac_alloc(size: i32) -> i32 {
    unsafe {
        let layout = Layout::from_size_align_unchecked(size as usize, 1);
        raw_alloc(layout) as i32
    }
}

fn field(request: &str, key: &str) -> Option<String> {
    let needle = ["\"", key, "\":\""].concat();
    let start = request.find(&needle)? + needle.len();
    let rest = &request[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[no_mangle]
pub extern "C" fn tac_invoke(ptr: i32, len: i32) -> i64 {
    let request = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr as *const u8, len as usize))
    };
    let op = field(request, "op").unwrap_or_default();
    let name = field(request, "name").unwrap_or_default();
    let response = match (op.as_str(), name.as_str()) {
        ("init", _) => "{\"value\":{\"fields\":[\"count\",\"label\"],\"methods\":[\"doubled\"]}}"
            .to_string(),
        ("get", "count") => ["{\"value\":", &unsafe { COUNT }.to_string(), "}"].concat(),
        ("get", "label") => "{\"value\":\"from Rust\"}".to_string(),
        ("call", "doubled") => ["{\"value\":", &(unsafe { COUNT } * 2).to_string(), "}"].concat(),
        ("set", "count") => {
            let start = request.find("\"value\":").map(|index| index + 8).unwrap_or(0);
            let rest = &request[start..];
            let end = rest.find('}').unwrap_or(0);
            unsafe { COUNT = rest[..end].trim().parse::<i64>().unwrap_or(0) };
            "{\"value\":null}".to_string()
        }
        _ => "{\"error\":\"unknown member\"}".to_string(),
    };
    let bytes: Vec<u8> = response.into_bytes();
    let out = tac_alloc(bytes.len() as i32);
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, bytes.len()) };
    ((out as i64) << 32) | (bytes.len() as i64)
}
