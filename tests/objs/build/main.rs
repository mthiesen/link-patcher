#![no_std]
#![feature(lang_items, panic_implementation)]

use core::panic::PanicInfo;

extern { fn add(a: u32, b: u32) -> u32; }

#[link(name = "kernel32")]
extern "stdcall" {
    pub fn ExitProcess(uExitCode: u32);
}

#[lang = "eh_personality"] extern fn eh_personality() {}
#[panic_implementation]
fn my_panic(_pi: &PanicInfo) -> ! { unsafe { ExitProcess(1); } loop {} }

#[no_mangle]
pub extern "C" fn main() {
    unsafe { ExitProcess(add(363, 974)) };
}
