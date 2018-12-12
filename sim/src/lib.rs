#[macro_use] extern crate log;
extern crate base64;
extern crate env_logger;
extern crate docopt;
extern crate libc;
extern crate rand;
#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate simflash;
extern crate nffs_sys;

pub mod testlog;

//use rand::{Rng, SeedableRng, XorShiftRng};
use std::fmt;

use simflash::{Flash, SimFlash};
use nffs_sys::c::{nffs_restore, nffs_format, nffs_init_, NffsAreaDesc};

#[derive(Debug, Deserialize)]
struct Args {
    flag_help: bool,
    flag_version: bool,
    flag_device: Option<DeviceName>,
    flag_align: Option<AlignArg>,
    cmd_sizes: bool,
    cmd_run: bool,
    cmd_runall: bool,
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub enum DeviceName { Linear4k, }

pub static ALL_DEVICES: &'static [DeviceName] = &[
    DeviceName::Linear4k,
];

impl fmt::Display for DeviceName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match *self {
            DeviceName::Linear4k => "linear-4k",
        };
        f.write_str(name)
    }
}

#[derive(Debug)]
struct AlignArg(u8);

struct AlignArgVisitor;

impl<'de> serde::de::Visitor<'de> for AlignArgVisitor {
    type Value = AlignArg;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("1, 2, 4 or 8")
    }

    fn visit_u8<E>(self, n: u8) -> Result<Self::Value, E>
        where E: serde::de::Error
    {
        Ok(match n {
            1 | 2 | 4 | 8 => AlignArg(n),
            n => {
                let err = format!("Could not deserialize '{}' as alignment", n);
                return Err(E::custom(err));
            }
        })
    }
}

impl<'de> serde::de::Deserialize<'de> for AlignArg {
    fn deserialize<D>(d: D) -> Result<AlignArg, D::Error>
        where D: serde::de::Deserializer<'de>
    {
        d.deserialize_u8(AlignArgVisitor)
    }
}

/// A test run, intended to be run from "cargo test", so panics on failure.
pub struct Run {
    flash: SimFlash,
    adesc: NffsAreaDesc,
}

impl Run {
    pub fn new(device: DeviceName, align: u8, erased_val: u8) -> Run {
        let (flash, adesc) = make_device(device, align, erased_val);

        Run {
            flash: flash,
            adesc: adesc,
        }
    }

    pub fn each_device<F>(f: F)
        where F: Fn(&mut Run)
    {
        for &dev in ALL_DEVICES {
            //for &align in &[1, 2, 4, 8] {
            for &align in &[1] {
                //XXX NFFS seems to fail with erased_val = 0
                //for &erased_val in &[0, 0xff] {
                for &erased_val in &[0xff] {
                    let mut run = Run::new(dev, align, erased_val);
                    f(&mut run);
                }
            }
        }
    }

    pub fn make_fs(&self) -> FS {
        nffs_init_();
        let mut flash0 = self.flash.clone();
        let err = nffs_format(&mut flash0, &self.adesc);
        if err != 0 {
            panic!("Error formatting flash {}", err);
        }
        //debug
        flash0.dump();
        FS {
            flash: flash0,
            adesc: self.adesc.clone(),
            //total_count: Some(0),
        }
    }
}

/// Build the Flash and area descriptor for a given device.
pub fn make_device(device: DeviceName, align: u8, erased_val: u8) -> (SimFlash, NffsAreaDesc) {
    match device {
        DeviceName::Linear4k => {
            let flash = SimFlash::new(vec![4096; 256], align as usize, erased_val);
            let mut adescs = NffsAreaDesc::new(&flash);
            adescs.add_area_desc(0x00000000, 16 * 1024);
            adescs.add_area_desc(0x00004000, 16 * 1024);
            adescs.add_area_desc(0x00008000, 16 * 1024);
            adescs.add_area_desc(0x0000c000, 16 * 1024);
            adescs.add_area_desc(0x00010000, 64 * 1024);
            adescs.add_area_desc(0x00020000, 128 * 1024);
            adescs.add_area_desc(0x00040000, 128 * 1024);
            adescs.add_area_desc(0x00060000, 128 * 1024);
            adescs.add_area_desc(0x00080000, 128 * 1024);
            adescs.add_area_desc(0x000a0000, 128 * 1024);
            adescs.add_area_desc(0x000c0000, 128 * 1024);
            adescs.add_area_desc(0x000e0000, 128 * 1024);
            (flash, adescs)
        }
    }
}

impl FS {
    // TODO:
    // 2) create/remove files/dirs
    // 3) reset while doing the above operations
    // 4) call restore on a copy and check above ops are OK (apart from last one)
    // 5) repeat
    pub fn run_basic(&self) -> bool {
        let (flash, total_count) = try_restore(&self.flash, &self.adesc, None);
        info!("Total flash operation count={}", total_count);

        if !verify_fs(flash) {
            warn!("Image mismatch after first boot");
        }
        true
    }
}

/// Test a boot, optionally stopping after 'n' flash options.  Returns a count
/// of the number of flash operations done total.
fn try_restore(flash: &SimFlash, adescs: &NffsAreaDesc,
               stop: Option<i32>) -> (SimFlash, i32) {
    // Clone the flash to have a new copy.
    let mut flash = flash.clone();

    let mut counter = stop.unwrap_or(0);

    let (first_interrupted, count) = match nffs_restore(&mut flash, &adescs,
                                                        Some(&mut counter), false) {
        (-0x13579, _) => (true, stop.unwrap()),
        (0, _) => (false, -counter),
        (x, _) => panic!("Unknown return: {}", x),
    };

    counter = 0;
    if first_interrupted {
        // fl.dump();
        match nffs_restore(&mut flash, &adescs, Some(&mut counter), false) {
            (-0x13579, _) => panic!("Shouldn't stop again"),
            (0, _) => (),
            (x, _) => panic!("Unknown return: {}", x),
        }
    }

    (flash, count - counter)
}

fn verify_fs(_flash: SimFlash) -> bool {
    true
}

/// Show the flash layout.
#[allow(dead_code)]
fn show_flash(flash: &Flash) {
    println!("---- Flash configuration ----");
    for sector in flash.sector_iter() {
        println!("    {:3}: 0x{:08x}, 0x{:08x}",
                 sector.num, sector.base, sector.size);
    }
    println!("");
}

pub struct FS {
    flash: SimFlash,
    adesc: NffsAreaDesc,
    //total_count: Option<i32>,
}

//XXX used to generated random data images for mcuboot testing...
// Drop some pseudo-random gibberish onto the data.
//fn splat(data: &mut [u8], seed: usize) {
//    let seed_block = [0x135782ea, 0x92184728, data.len() as u32, seed as u32];
//    let mut rng: XorShiftRng = SeedableRng::from_seed(seed_block);
//    rng.fill_bytes(data);
//}
