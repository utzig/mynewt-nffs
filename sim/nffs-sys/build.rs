// Build mcuboot as a library, based on the requested features.

extern crate cc;

use std::fs;
use std::io;
use std::path::Path;

fn main() {
    // Feature flags.
    let mut conf = cc::Build::new();

    conf.define("__NFFS_SIM__", None);

    conf.include("../../include");

    conf.file("../../src/nffs_area.c");
    conf.file("../../src/nffs.c");
    conf.file("../../src/nffs_crc.c");
    conf.file("../../src/nffs_file.c");
    conf.file("../../src/nffs_format.c");
    conf.file("../../src/nffs_hash.c");
    conf.file("../../src/nffs_misc.c");
    conf.file("../../src/nffs_restore.c");
    conf.file("../../src/nffs_block.c");
    conf.file("../../src/nffs_cache.c");
    conf.file("../../src/nffs_dir.c");
    conf.file("../../src/nffs_flash.c");
    conf.file("../../src/nffs_gc.c");
    conf.file("../../src/nffs_inode.c");
    conf.file("../../src/nffs_path.c");
    conf.file("../../src/nffs_write.c");

    conf.file("csupport/run.c");
    conf.include("csupport");
    conf.debug(true);
    conf.flag("-Wall");
    conf.flag("-Werror");

    // FIXME: travis-ci still uses gcc 4.8.4 which defaults to std=gnu90.
    // It has incomplete std=c11 and std=c99 support but std=c99 was checked
    // to build correctly so leaving it here to updated in the future...
    conf.flag("-std=c99");

    conf.compile("libnffs.a");

    walk_dir("../../src").unwrap();
    walk_dir("../../include/nffs").unwrap();
    walk_dir("csupport").unwrap();
}

// Output the names of all files within a directory so that Cargo knows when to rebuild.
fn walk_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    for ent in fs::read_dir(path.as_ref())? {
        let ent = ent?;
        let p = ent.path();
        if p.is_dir() {
            walk_dir(p)?;
        } else {
            // Note that non-utf8 names will fail.
            let name = p.to_str().unwrap();
            if name.ends_with(".c") || name.ends_with(".h") {
                println!("cargo:rerun-if-changed={}", name);
            }
        }
    }

    Ok(())
}
