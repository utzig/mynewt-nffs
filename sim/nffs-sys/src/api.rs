//! HAL api for MyNewt applications

use simflash::{Result, Flash};
use libc;
use log::LogLevel;
use std::mem;
use std::slice;
use crc::{crc16, Hasher16};

struct FlashParams {
    align: u8,
    erased_val: u8,
}

static mut FLASH: Option<*mut Flash> = None;
static mut FLASH_PARAMS: Option<FlashParams> = None;

// Set the flash device to be used by the simulation.  The pointer is unsafely stashed away.
pub unsafe fn set_flash(dev: &mut Flash) {
    FLASH_PARAMS = Some(FlashParams {
        align: dev.align() as u8,
        erased_val: dev.erased_val(),
    });

    let dev: &'static mut Flash = mem::transmute(dev);
    FLASH = Some(dev as *mut Flash);
}

pub unsafe fn clear_flash() {
    FLASH_PARAMS = None;
    FLASH = None;
}

// This isn't meant to call directly, but by a wrapper.

#[no_mangle]
pub unsafe extern fn sim_flash_erase(offset: u32, size: u32) -> libc::c_int {
    if let Some(flash) = FLASH {
        let dev = &mut *flash;
        return map_err(dev.erase(offset as usize, size as usize));
    }
    -19
}

#[no_mangle]
pub unsafe extern fn sim_flash_read(offset: u32, dest: *mut u8, size: u32) -> libc::c_int {
    if let Some(flash) = FLASH {
        let mut buf: &mut[u8] = slice::from_raw_parts_mut(dest, size as usize);
        let dev = &mut *flash;
        return map_err(dev.read(offset as usize, &mut buf));
    }
    -19
}

#[no_mangle]
pub unsafe extern fn sim_flash_write(offset: u32, src: *const u8, size: u32) -> libc::c_int {
    if let Some(flash) = FLASH {
        let buf: &[u8] = slice::from_raw_parts(src, size as usize);
        let dev = &mut *flash;
        return map_err(dev.write(offset as usize, &buf));
    }
    -19
}

#[no_mangle]
pub unsafe extern fn sim_flash_info(sector: u32, address: *mut u32, size: *mut u32) -> libc::c_int {
    if let Some(flash) = FLASH {
        let dev = &mut *flash;
        let addr = &mut *address;
        let sz = &mut *size;
        return map_err(dev.info(sector as usize, addr, sz));
    }
    -19
}

#[no_mangle]
pub unsafe extern fn sim_flash_align(_id: u8) -> u8 {
    if let Some(ref params) = FLASH_PARAMS {
        return params.align;
    }
    1
}

#[no_mangle]
pub unsafe extern fn sim_flash_erased_val(_id: u8) -> u8 {
    if let Some(ref params) = FLASH_PARAMS {
        return params.erased_val;
    }
    0xff
}

#[no_mangle]
pub extern fn sim_crc16(initial: u16, buf: *const u8, len: libc::c_int) -> u16 {
    let buf: &[u8] = unsafe { slice::from_raw_parts(buf, len as usize) };
    let mut digest = crc16::Digest::new_with_initial(crc16::X25, initial);
    digest.write(buf);
    return digest.sum16();
}

fn map_err(err: Result<()>) -> libc::c_int {
    match err {
        Ok(()) => 0,
        Err(e) => {
            warn!("{}", e);
            -1
        },
    }
}

/// Called by C code to determine if we should log at this level.  Levels are defined in
/// bootutil/bootutil_log.h.  This makes the logging from the C code controlled by bootsim::api, so
/// for example, it can be enabled with something like:
///     RUST_LOG=bootsim::api=info cargo run --release runall
/// or
///     RUST_LOG=bootsim=info cargo run --release runall
#[no_mangle]
pub extern fn sim_log_enabled(level: libc::c_int) -> libc::c_int {
    let res = match level {
        1 => log_enabled!(LogLevel::Error),
        2 => log_enabled!(LogLevel::Warn),
        3 => log_enabled!(LogLevel::Info),
        4 => log_enabled!(LogLevel::Trace),
        _ => false,
    };
    if res {
        1
    } else {
        0
    }
}
