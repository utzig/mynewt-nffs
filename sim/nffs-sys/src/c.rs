/// Interface wrappers to C API entering to the bootloader

use simflash::{Flash, SimFlash, Sector};
use libc;
use api;
use std::sync::Mutex;

lazy_static! {
    /// Mutex to lock the simulation.  The C code for the bootloader uses
    /// global variables, and is therefore non-reentrant.
    static ref _LOCK: Mutex<()> = Mutex::new(());
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CNffsAreaDesc {
    offset: u32,
    length: u32, // has value zero on the last element
    flash_id: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CNffsAreaDescArray {
    arr: [CNffsAreaDesc; 16],
}

#[derive(Debug, Clone)]
pub struct NffsAreaDesc {
    adescs: Vec<CNffsAreaDesc>,
    sectors: Vec<Sector>,
}

impl NffsAreaDesc {
    pub fn new(flash: &Flash) -> NffsAreaDesc {
        let mut adesc = NffsAreaDesc {
            adescs: vec![],
            sectors: vec![],
        };

        for sector in flash.sector_iter() {
            adesc.sectors.push(sector);
        }

        return adesc;
    }

    pub fn add_area_desc(&mut self, base: usize, len: usize) {
        let mut sbase = base;
        let mut slen = len;

        // Assure area descritions are sector aligned
        for sector in &self.sectors {
            if slen == 0 {
                break;
            };
            if sbase > sector.base + sector.size - 1 {
                continue;
            }
            if sector.base != sbase {
                panic!("Image does not start on a sector boundary");
            }
            sbase += sector.size;
            slen -= sector.size;
        }

        if slen != 0 {
            panic!("Image goes past end of device");
        }

        self.adescs.push(CNffsAreaDesc {
            offset: base as u32,
            length: len as u32,
            flash_id: 0,          // FIXME
        });
    }

    // FIXME: using a hardcoded maximum amount of entries
    pub fn get_c(&self) -> [CNffsAreaDesc; 16] {
        let mut adescs = [CNffsAreaDesc {
            offset: 0, length: 0, flash_id: 0 }; 16];

        if self.adescs.len() >= 15 {
            panic!("Not enough storage for all nffs area descs");
        }

        for (i, adesc) in self.adescs.iter().enumerate() {
            adescs[i].offset = adesc.offset;
            adescs[i].length= adesc.length;
            adescs[i].flash_id = adesc.flash_id;
        }

        let last = self.adescs.len();
        adescs[last].offset = 0;
        adescs[last].length = 0;

        adescs
    }
}

pub fn nffs_init_() {
    unsafe {
        raw::nffs_init();
    };
}

pub fn nffs_format(flash: &mut SimFlash, adesc: &NffsAreaDesc) -> i32 {
    let _lock = _LOCK.lock().unwrap();

    unsafe {
        api::set_flash(flash);
        raw::c_catch_asserts = 0;
        raw::c_asserts = 0u8;
        raw::flash_counter = 0;
    }
    let result = unsafe {
        raw::invoke_format(adesc.get_c().as_ptr() as *const _) as i32
    };
    unsafe {
        api::clear_flash();
    };
    result
}

pub fn nffs_restore(flash: &mut SimFlash, adesc: &NffsAreaDesc,
                    counter: Option<&mut i32>, catch_asserts: bool) -> (i32, u8) {
    let _lock = _LOCK.lock().unwrap();

    unsafe {
        api::set_flash(flash);
        raw::c_catch_asserts = if catch_asserts { 1 } else { 0 };
        raw::c_asserts = 0u8;
        raw::flash_counter = match counter {
            None => 0,
            Some(ref c) => **c as libc::c_int
        };
    }
    let result = unsafe {
        raw::invoke_restore(adesc.get_c().as_ptr() as *const _) as i32
    };
    let asserts = unsafe { raw::c_asserts };
    unsafe {
        counter.map(|c| *c = raw::flash_counter as i32);
        api::clear_flash();
    };
    (result, asserts)
}

mod raw {
    use libc;
    use crate::c::CNffsAreaDesc;

    extern "C" {
        pub fn nffs_init();
        pub fn invoke_restore(adesc: *const CNffsAreaDesc) -> libc::c_int;
        pub fn invoke_format(adesc: *const CNffsAreaDesc) -> libc::c_int;
        pub static mut flash_counter: libc::c_int;
        pub static mut c_asserts: u8;
        pub static mut c_catch_asserts: u8;
    }
}
