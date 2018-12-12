#[macro_use] extern crate lazy_static;
extern crate libc;
#[macro_use] extern crate log;
extern crate simflash;
extern crate crc;

pub mod c;

// The API needs to be public, even though it isn't intended to be called by Rust code, but the
// functions are exported to C code.
pub mod api;
