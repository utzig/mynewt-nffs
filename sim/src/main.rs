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
extern crate regex;
#[macro_use] extern crate lazy_static;

pub mod testlog;

//use rand::{Rng, SeedableRng, XorShiftRng};
use std::{fmt, process, io};
use std::fs::{self, File, ReadDir, DirEntry};
use std::io::BufRead; // BufReader
use std::path::{Path, PathBuf};
use docopt::Docopt;
use regex::Regex;

use simflash::{Flash, SimFlash};
use nffs_sys::c::{
    nffs_restore,
    nffs_format,
    nffs_init_,
    nffs_write_to_file,
    NffsAreaDesc
};

pub const SCRIPT_DIR: &str = "scripts/";

lazy_static! {
    static ref FORMAT_RE: Regex = Regex::new(r"^\s*format\s*$").unwrap();
    static ref RESTORE_RE: Regex = Regex::new(r"^\s*restore\s*$").unwrap();
    static ref FILE_OPEN_RE: Regex = Regex::new(r"^\s*file_open\s+(\w+)\s+(append|truncate)?\s*$").unwrap();
    static ref WRITE_TO_FILE_RE: Regex = Regex::new(r"^\s*write_to_file\s+(\w+)\s+(\w)\s+(\d+)\s*$").unwrap();
    static ref PATH_RENAME_RE: Regex = Regex::new(r"^\s*path_rename\s+(\w+)\s+(\w+)\s*$").unwrap();
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

/// Build the Flash and area descriptor for a given device.
pub fn make_device(device: DeviceName, align: u8, erased_val: u8) -> (SimFlash, NffsAreaDesc) {
    match device {
        DeviceName::Linear4k => {
            let flash = SimFlash::new(vec![256; 32], align as usize, erased_val);
            let mut adescs = NffsAreaDesc::new(&flash);
            adescs.add_area_desc(0x00000000, 256);
            adescs.add_area_desc(0x00000100, 256);
            adescs.add_area_desc(0x00000200, 256);
            adescs.add_area_desc(0x00000300, 256);
            adescs.add_area_desc(0x00000400, 1024);
            adescs.add_area_desc(0x00000800, 1024);
            adescs.add_area_desc(0x00000c00, 1024);
            adescs.add_area_desc(0x00001000, 2048);
            adescs.add_area_desc(0x00001800, 2048);
            (flash, adescs)
        }
    }
}

pub struct DirWalker {
    walker: ReadDir,
}

impl Iterator for DirWalker {
    type Item = PathBuf;

    fn next(&mut self) -> Option<PathBuf> {
        let entry = self.walker.next();
        match entry {
            Some(v) => match v {
                Ok(entry) => Some(entry.path()),
                Err(err) => panic!("Path error: {}", err),
            }
            None => None,
        }
    }
}

impl DirWalker {
    pub fn new(dirname: &'static str) -> Self {
        let dir = Path::new(dirname);
        if !dir.is_dir() {
            panic!("Invalid script directory");
        }
        let walker = fs::read_dir(&dir);
        match walker {
            Ok(w) => Self {
                walker: w,
            },
            Err(_) => panic!("Error reading script directory"),
        }
    }
}

pub struct ScriptRunner {}

impl ScriptRunner {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(&self, path: &PathBuf, fs: &mut NFFS) -> Result<i32, &'static str> {
        if !path.is_dir() {
            info!("Running script: \"{}\"", path.display());
            match File::open(path) {
                Ok(f) =>
                    for line in io::BufReader::new(f).lines() {
                        self.process(line.unwrap(), fs);
                    },
                Err(e) => panic!("{}", e),
            }
        }
        Ok(0)
    }

    fn process(&self, cmdline: String, fs: &mut NFFS) -> bool {
        let cmdline = cmdline.as_str();
        if FORMAT_RE.is_match(cmdline) {
            println!("format");
            fs.format();
        } else if RESTORE_RE.is_match(cmdline) {
            println!("restore");
            fs.restore();
        } else if FILE_OPEN_RE.is_match(cmdline) {
            for cap in FILE_OPEN_RE.captures_iter(cmdline) {
                let name = &cap[1];
                let flags = cap[2].as_bytes()[0];
                println!("file_open name={} flags={}", name, flags);
                fs.file_open(name, flags);
            }
        } else if WRITE_TO_FILE_RE.is_match(cmdline) {
            for cap in WRITE_TO_FILE_RE.captures_iter(cmdline) {
                let name = &cap[1];
                let c = &cap[2];
                if c.len() != 1 {
                    panic!("Invalid character to use as fill");
                }
                let len = cap[3].parse::<usize>().unwrap();
                println!("write_to_file filename={} char={} len={}", name, c, len);
                let mut buf: Vec<u8> = Vec::new();
                let c = c.as_bytes()[0];
                buf.resize_with(len, || c);
                fs.write_to_file(name, &buf);
            }
        } else if PATH_RENAME_RE.is_match(cmdline) {
            for cap in PATH_RENAME_RE.captures_iter(cmdline) {
                println!("path_rename oldname={} newname={}", &cap[1], &cap[2]);
            }
        } else {
            println!("Invalid command \"{}\"", cmdline);
            return false;
        }
        true
    }
}

pub struct NFFS {
    flash: SimFlash,
    adesc: NffsAreaDesc,
    total_count: Option<i32>,
}

impl NFFS {
    pub fn format(&mut self) -> bool {
        let err = nffs_format(&mut self.flash, &self.adesc);
        if err != 0 {
            warn!("Error formatting flash {}", err);
            return false;
        }
        true
    }

    pub fn restore(&mut self) -> bool {
        let mut counter = 0;
        let (err, _) = nffs_restore(&mut self.flash, &self.adesc, Some(&mut counter), false);
        if err != 0 {
            warn!("Error formatting flash {}", err);
            return false;
        }
        true
    }

    pub fn file_open(&mut self, name: &str, flags: u8) -> bool {
        false
    }

    pub fn write_to_file(&mut self, name: &str, buf: &[u8]) -> bool {
        let mut count = 0;
        match nffs_write_to_file(&mut self.flash, &self.adesc, name, buf,
                                 Some(&mut count), false) {
            (0, v) => warn!("Sanity tests asserted {} times", v),
            (e, _) => panic!("Error running sanity tests: {}", e),
        };

        warn!("Total flash operation count={}", count);

        false
    }
}

//        count = -count;
//        for i in 1..count {
//            let mut flash1 = self.flash.clone();
//
//            info!("Running stop={}", i);
//
//            //TODO: format
//
//            let mut counter = i;
//            match nffs_file_open(&mut flash1, &self.adesc, Some(&mut counter), false) {
//                (-0x13579, c) => warn!("First interrupt at {}", c),
//                (0, v) => warn!("Sanity tests asserted {} times", v),
//                (e, _) => panic!("Error running sanity tests: {}", e),
//            };
//
//            flash1.dump();
//
//            warn!("Total flash operation count={}", count);
//        }

/// Test a boot, optionally stopping after 'n' flash options.  Returns a count
/// of the number of flash operations done total.
//fn try_restore(flash: &SimFlash, adescs: &NffsAreaDesc,
//               stop: Option<i32>) -> (SimFlash, i32) {
//    // Clone the flash to have a new copy.
//    let mut flash = flash.clone();
//
//    let mut counter = stop.unwrap_or(0);
//
//    let (first_interrupted, count) = match nffs_restore(&mut flash, &adescs,
//                                                        Some(&mut counter), false) {
//        (-0x13579, _) => (true, stop.unwrap()),
//        (0, _) => (false, -counter),
//        (x, _) => panic!("Unknown return: {}", x),
//    };
//
//    counter = 0;
//    if first_interrupted {
//        match nffs_restore(&mut flash, &adescs, Some(&mut counter), false) {
//            (-0x13579, _) => panic!("Shouldn't stop again"),
//            (0, _) => (),
//            (x, _) => panic!("Unknown return: {}", x),
//        }
//    }
//
//    (flash, count - counter)
//}

//fn verify_fs(_flash: SimFlash) -> bool {
//    true
//}

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

pub struct RunStatus {
    failures: usize,
    passes: usize,
    runner: ScriptRunner,
    flash: SimFlash,
    adesc: NffsAreaDesc,
}

impl RunStatus {
    pub fn new(runner: ScriptRunner, device: DeviceName, align: u8,
               erased_val: u8) -> RunStatus {
        let (flash, adesc) = make_device(device, align, erased_val);
        nffs_init_();
        RunStatus {
            failures: 0,
            passes: 0,
            runner: runner,
            flash: flash,
            adesc: adesc,
        }
    }

    /// Make a fresh copy of the flash
    pub fn new_fs(&self) -> NFFS {
        NFFS {
            flash: self.flash.clone(),
            adesc: self.adesc.clone(),
            total_count: Some(0),
        }
    }

    pub fn run(&mut self, script: &PathBuf) {
        let mut fs = self.new_fs();
        let failed = match self.runner.run(script, &mut fs) {
            Ok(_i) => false,
            Err(_) => true,
        };
        if failed {
            self.failures += 1;
        } else {
            self.passes += 1;
        }
    }

    pub fn failures(&self) -> usize {
        self.failures
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

const USAGE: &'static str = "
NFFS simulator

Usage:
  nffssim
  nffssim (--help | --version)

Options:
  -h, --help         Show this message
  --version          Version
  --align SIZE       Flash write alignment
  --erase VAL        Value read from erased flash
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_help: bool,
    flag_version: bool,
    flag_align: Option<AlignArg>,
    flag_erased_val: Option<u8>,
}

pub fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    let align = args.flag_align.map(|x| x.0).unwrap_or(1);
    let erased_val = args.flag_erased_val.map(|x| x).unwrap_or(0xff);

    let runner = ScriptRunner::new();
    let mut status = RunStatus::new(runner, DeviceName::Linear4k, align, erased_val);
    let walker = DirWalker::new(SCRIPT_DIR);

    for script in walker {
        status.run(&script);
    }

    if status.failures > 0 {
        error!("{} Tests ran with {} failures", status.failures + status.passes, status.failures);
        process::exit(1);
    } else {
        error!("{} Tests ran successfully", status.passes);
        process::exit(0);
    }
}
