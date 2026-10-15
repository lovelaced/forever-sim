//! WebAssembly entry points (plain C ABI, no wasm-bindgen needed).
//! JavaScript writes a UTF-8 JSON request into memory from `fsim_alloc`,
//! calls `fsim_call`, then reads `fsim_out_len` bytes at `fsim_out_ptr`.
//! Compiled on every target so it is type-checked by the native tests too.

use std::cell::RefCell;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Reserve `len` bytes for a request. Freed by `fsim_call`.
#[no_mangle]
pub extern "C" fn fsim_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Handle the request in `ptr[..len]` (allocated with `fsim_alloc`), free it,
/// and keep the JSON response for `fsim_out_ptr` / `fsim_out_len`.
///
/// # Safety
/// `ptr` must come from `fsim_alloc(len)` and hold `len` initialised bytes.
#[no_mangle]
pub unsafe extern "C" fn fsim_call(ptr: *mut u8, len: usize) -> usize {
    let bytes = Vec::from_raw_parts(ptr, len, len.max(1));
    let req = String::from_utf8_lossy(&bytes).into_owned();
    drop(bytes);
    let out = crate::api::handle(&req).into_bytes();
    let n = out.len();
    OUT.with(|o| *o.borrow_mut() = out);
    n
}

#[no_mangle]
pub extern "C" fn fsim_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn fsim_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}
