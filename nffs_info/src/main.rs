use std::env;
use std::fs::File;
use std::io::Read;
use std::str::from_utf8;
use std::mem::transmute;
use crc16::{XMODEM, State};
use std::collections::{BTreeMap, HashMap};
use std::process::exit;
use std::cmp::Ordering;

#[macro_use]
extern crate lazy_static;

const NFFS_AREA_MAGIC0: u32 = 0xb98a_31e2;
const NFFS_AREA_MAGIC1: u32 = 0x7fb0_428c;
const NFFS_AREA_MAGIC2: u32 = 0xace0_8253;
const NFFS_AREA_MAGIC3: u32 = 0xb185_fc8e;

const NFFS_DISK_AREA_SZ: usize = 24;
const NFFS_DISK_INODE_SZ: usize = 20;
const NFFS_DISK_BLOCK_SZ: usize = 20;

const NFFS_ID_NONE: u32      = 0xffff_ffff;
const NFFS_ID_FILE_MIN: u32  = 0x1000_0000;
const NFFS_ID_BLOCK_MIN: u32 = 0x8000_0000;

const ABORT_AREA: &str = "Object with invalid size, aborting area inspection";

#[derive(Debug)]
pub struct NffsDiskArea {
    magic: [u32; 4],
    length: u32,
    version: u8,
    gc_seq: u8,
    reserved8: u8,
    id: u8,
}

const NFFS_DISK_INODE_OFFSET_CRC: usize = 18;
const NFFS_DISK_BLOCK_OFFSET_CRC: usize = 18;

#[repr(packed)]
pub struct PackedNffsDiskInode {
    /* Unique object ID. */
    id: u32,
    /* Object ID of parent directory inode. */
    parent_id: u32,
    /* Object ID of parent directory inode. */
    lastblock_id: u32,
    /* Sequence number; greater supersedes lesser. */
    seq: u16,
    _reserve16: u16,
    flags: u8,
    filename_len: u8,
    /* Covers rest of header and filename. */
    crc16: u16,
    /* Followed by filename. */
}

#[derive(Debug)]
pub struct NffsDiskInode {
    parent_id: u32,
    lastblock_id: u32,
    seq: u16,
    name: String,
    crc_ok: bool,
}

#[repr(packed)]
pub struct PackedNffsDiskBlock {
    /* Unique object ID. */
    id: u32,
    /* Object ID of owning inode. */
    inode_id: u32,
    /* Object ID of previous block in file;
       NFFS_ID_NONE if this is the first block. */
    prev_id: u32,
    /* Sequence number; greater supersedes lesser. */
    seq: u16,
    _reserved16: u16,
    data_len: u16,
    /* Covers rest of header and data. */
    crc16: u16,
    /* Followed by 'ndb_data_len' bytes of data. */
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NffsDiskBlock {
    inode_id: u32,
    prev_id: u32,
    seq: u16,
    len: u16,
    crc_ok: bool,
}

const INODE_FREE: u8        = 0x00;
const INODE_DUMMY: u8       = 0x01;    /* inode is a dummy */
const INODE_DUMMYPARENT: u8 = 0x02;    /* parent not in cache */
const INODE_DUMMYLSTBLK: u8 = 0x04;    /* lastblock not in cache */
const INODE_DUMMYINOBLK: u8 = 0x08;    /* dummy inode for blk */
const INODE_OBSOLETE: u8    = 0x10;    /* always replace if same ID */
const INODE_INTREE: u8      = 0x20;    /* in directory structure */
const INODE_INHASH: u8      = 0x40;    /* in hash table */
const INODE_DELETED: u8     = 0x80;    /* inode deleted */

lazy_static! {
    static ref INODE_FLAGS_MAP: BTreeMap<u8, &'static str> = {
        let mut map = BTreeMap::new();
        map.insert(INODE_DUMMY, "DUMMY");
        map.insert(INODE_DUMMYPARENT, "DUMMYPARENT");
        map.insert(INODE_DUMMYLSTBLK, "DUMMYLSTBLK");
        map.insert(INODE_DUMMYINOBLK, "DUMMYLINOBLK");
        map.insert(INODE_OBSOLETE, "OBSOLETE");
        map.insert(INODE_INTREE, "INTREE");
        map.insert(INODE_INHASH, "INHASH");
        map.insert(INODE_DELETED, "DELETED");
        map
    };
}

fn check_magic(magic: &[u32]) -> &str {
    if magic[0] == NFFS_AREA_MAGIC0 && magic[1] == NFFS_AREA_MAGIC1 &&
       magic[2] == NFFS_AREA_MAGIC2 && magic[3] == NFFS_AREA_MAGIC3 {
        return "good";
    }
    "bad"
}

fn print_block_info(obj: &PackedNffsDiskBlock, index: u8, crc: u16) {
    println!("\t-> Block {}", index);
    let id = obj.id;
    let inode_id = obj.inode_id;
    let prev_id = obj.prev_id;
    let seq = obj.seq;
    let len = obj.data_len;
    println!("\t\tid: 0x{:08x}", id);
    println!("\t\tinode_id: 0x{:08x}", inode_id);
    println!("\t\tprev_id: 0x{:08x}", prev_id);
    println!("\t\tseq: {}", seq);
    println!("\t\tdata_len: {}", len);
    if obj.crc16 == crc {
        println!("\t\tcrc: 0x{:04x} (good)", crc);
    } else {
        println!("\t\tcrc: 0x{:04x} (bad)", crc);
    }
}

fn unpack_block(obj: &PackedNffsDiskBlock, data: &[u8], crc_ok: bool) -> NffsDiskBlock {
    NffsDiskBlock {
        inode_id: obj.inode_id,
        prev_id: obj.prev_id,
        seq: obj.seq,
        len: data.len() as u16,
        crc_ok: crc_ok,
    }
}

fn inode_flags_str(flags: u8) -> String {
    let mut s = String::new();
    let mut need_or = false;
    s += " (";
    if flags == INODE_FREE {
        s += "FREE";
    } else {
        for (k, v) in INODE_FLAGS_MAP.iter() {
            if need_or {
                s += "|";
                need_or = false;
            }
            if flags & k != 0 {
                s += v;
                need_or = true;
            }
        }
    }
    s += ")";
    s
}

fn print_inode_info(obj: &PackedNffsDiskInode, index: u8, name: &[u8], crc: u16) {
    println!("\t-> Inode {}", index);
    let id = obj.id;
    let parent_id = obj.parent_id;
    let lastblock_id = obj.lastblock_id;
    let seq = obj.seq;
    let crc16 = obj.crc16;
    println!("\t\tid: 0x{:08x}", id);
    println!("\t\tparent_id: 0x{:08x}", parent_id);
    println!("\t\tlastblock_id: 0x{:08x}", lastblock_id);
    println!("\t\tseq: {}", seq);
    println!("\t\tflags: 0x{:02x}{}", obj.flags, inode_flags_str(obj.flags));
    if crc == crc16 {
        let name = from_utf8(name);
        if id < NFFS_ID_FILE_MIN {
            println!("\t\tdirname: \"{}\"",
                     if id == 0 {
                         "/"
                     } else {
                         name.unwrap()
                     });
        } else {
            println!("\t\tfilename: \"{}\"", name.unwrap());
        }
        println!("\t\tcrc: 0x{:04x} (good)", crc);
    } else {
        println!("\t\tcrc: 0x{:04x} (bad)", crc);
    }
}

fn unpack_inode(obj: &PackedNffsDiskInode, name: &[u8], crc_ok: bool) -> NffsDiskInode {
    NffsDiskInode {
        parent_id: obj.parent_id,
        lastblock_id: obj.lastblock_id,
        seq: obj.seq,
        name: from_utf8(name).unwrap().to_string(),
        crc_ok: crc_ok,
    }
}

fn disk_area_info(inodes: &mut HashMap<u32, NffsDiskInode>,
                  blocks: &mut HashMap<u32, NffsDiskBlock>,
                  da: &NffsDiskArea, index: u8, area: &[u8]) {
    println!("Disk Area {}", index);
    println!("\tmagic: {}", check_magic(&da.magic));
    println!("\tlength: {}", da.length);
    println!("\tversion: {}", da.version);
    println!("\tgc_seq: {}", da.gc_seq);
    println!("\tid: {}{}", da.id, if da.id == 255 { " (scratch)" } else { "" });

    let mut arr2 = [0u8; 4];
    let mut i = 0;
    let mut objidx: usize = 0;
    loop {
        arr2.copy_from_slice(&area[objidx .. objidx+4]);
        let id = unsafe {
            transmute::<[u8; 4], u32>(arr2)
        };
        if id == NFFS_ID_NONE {
            println!("\tFree space: {}", area.len() - objidx);
            break;
        } else if id >= NFFS_ID_BLOCK_MIN {
            const SZ: usize = NFFS_DISK_BLOCK_SZ;
            let mut arr = [0u8; SZ];
            if objidx + SZ > area.len() {
                println!("\t{}", ABORT_AREA);
                break;
            }
            arr.copy_from_slice(&area[objidx .. objidx + SZ]);
            let obj = unsafe {
                transmute::<[u8; SZ], PackedNffsDiskBlock>(arr)
            };
            let len = obj.data_len as usize;
            if objidx + SZ + len > area.len() {
                println!("\t{}", ABORT_AREA);
                break;
            }
            let hdr = &area[objidx .. objidx + NFFS_DISK_BLOCK_OFFSET_CRC as usize];
            let data = &area[objidx + SZ .. objidx + SZ + len];
            let mut state = State::<XMODEM>::new();
            state.update(hdr);
            state.update(data);
            print_block_info(&obj, i, state.get());
            let crc_ok = obj.crc16 == state.get();
            if crc_ok {
                let block = unpack_block(&obj, data, crc_ok);
                blocks.insert(obj.id, block);
            }
            objidx += SZ + len;
        } else {
            const SZ: usize = NFFS_DISK_INODE_SZ;
            let mut arr = [0u8; SZ];
            if objidx + SZ > area.len() {
                println!("\t{}", ABORT_AREA);
                break;
            }
            arr.copy_from_slice(&area[objidx .. objidx + SZ]);
            let obj = unsafe {
                transmute::<[u8; SZ], PackedNffsDiskInode>(arr)
            };
            let len = obj.filename_len as usize;
            if objidx + SZ + len > area.len() {
                println!("\t{}", ABORT_AREA);
                break;
            }
            let hdr = &area[objidx .. objidx + NFFS_DISK_INODE_OFFSET_CRC as usize];
            let name = &area[objidx + SZ .. objidx + SZ + len];
            let mut state = State::<XMODEM>::new();
            state.update(hdr);
            state.update(name);
            print_inode_info(&obj, i, &name, state.get());
            let crc_ok = obj.crc16 == state.get();
            if crc_ok {
                let inode = unpack_inode(&obj, name, crc_ok);
                let id = obj.id;
                if !(inodes.contains_key(&id) && inodes.get(&id).unwrap().seq < obj.seq) {
                    inodes.insert(obj.id, inode);
                }
            }
            objidx += SZ + len;
        }
        if objidx >= area.len() {
            break;
        }
        i += 1;
    }
}

type BlockIdAndInfo<'a> = (u32, &'a NffsDiskBlock);

fn print_tree_branch(inodes: &HashMap<u32, NffsDiskInode>,
                     children: &HashMap<u32, Vec<u32>>,
                     blocktree: &HashMap<u32, Vec<BlockIdAndInfo>>,
                     id: u32, level: usize, prefix: String) {
    let v = children.get(&id).unwrap();
    for (index, &child_id) in v.iter().enumerate() {
        let child_inode = inodes.get(&child_id).unwrap();
        print!("{}", prefix);
        let last = index == v.len() - 1;
        if last {
            print!("└── {}", child_inode.name);
        } else {
            print!("├── {}", child_inode.name);
        }
        if blocktree.contains_key(&child_id) {
            let v = blocktree.get(&child_id).unwrap();
            let mut len = 0;
            let mut blocks = String::new();
            let last = v.len() - 1;
            for (index, (id, block)) in v.iter().enumerate() {
                if index != last {
                    let fmt = format!("0x{:08x}, ", id);
                    blocks.push_str(&fmt);
                } else {
                    let fmt = format!("0x{:08x}", id);
                    blocks.push_str(&fmt);
                }
                len += block.len;
            }
            println!(" [size={}, blocks=({})]", len, blocks);
        } else {
            println!();
        }
        if children.get(&child_id).is_some() {
            let new_prefix: String;
            if last {
                let postfix = format!("{: <1$}", "", (level + 1) * 5);
                new_prefix = format!("{}{}", prefix, postfix);
            } else {
                let postfix = format!("│{: <1$}", "", (level + 1) * 5 - 1);
                new_prefix = format!("{}{}", prefix, postfix);
            }
            print_tree_branch(inodes, children, blocktree, child_id, level + 1,
                              new_prefix);
        }
    }
}

// XXX maybe should update to use prev_id for sorting...
#[allow(dead_code)]
fn disk_block_sort(a: &BlockIdAndInfo, b: &BlockIdAndInfo) -> Ordering {
    if a.0 < b.0 {
        return Ordering::Less;
    } else if a.0 > b.0 {
        return Ordering::Greater;
    }
    Ordering::Equal
}

fn build_tree(inodes: &HashMap<u32, NffsDiskInode>,
              blocks: &HashMap<u32, NffsDiskBlock>) {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&id, inode) in inodes {
        if inode.parent_id == NFFS_ID_NONE {
            continue;
        } else if children.contains_key(&inode.parent_id) {
            let mut vec = children.get_mut(&inode.parent_id).unwrap().to_vec();
            vec.push(id);
            children.insert(inode.parent_id, vec);
        } else {
            children.insert(inode.parent_id, vec![id]);
        }
    }
    let mut blocktree: HashMap<u32, Vec<BlockIdAndInfo>> = HashMap::new();
    for (&id, block) in blocks {
        let inode_id = block.inode_id;
        if blocktree.contains_key(&inode_id) {
            let mut vec = blocktree.get_mut(&inode_id).unwrap().to_vec();
            vec.push((id, block));
            blocktree.insert(inode_id, vec);
        } else {
            blocktree.insert(inode_id, vec![(id, block)]);
        }
    }
    for block in blocktree.values_mut() {
        block.sort_by(disk_block_sort);
    }
    println!("/");
    print_tree_branch(&inodes, &children, &blocktree, 0, 0, String::new());
}

fn is_valid_fs(inodes: &HashMap<u32, NffsDiskInode>,
               blocks: &HashMap<u32, NffsDiskBlock>) -> bool {
    // must have a root directory
    if !inodes.contains_key(&0) {
        return false;
    }
    // every other inode must have a parent
    for (id, inode) in inodes {
         if id != &0 && !inodes.contains_key(&inode.parent_id) {
             return false;
         }
    }
    for block in blocks.values() {
        if !inodes.contains_key(&block.inode_id) {
            return false;
        }
        let prev_id = block.prev_id;
        if prev_id != NFFS_ID_NONE && !blocks.contains_key(&prev_id) {
            return false;
        }
    }
    build_tree(&inodes, &blocks);
    true
}

fn check_fs(data: Vec<u8>) -> bool {
    let mut arr = [0u8; NFFS_DISK_AREA_SZ];
    let mut i = 0;
    let mut daidx: usize = 0;
    let mut inode_map = HashMap::new();
    let mut block_map = HashMap::new();
    loop {
        let sz = NFFS_DISK_AREA_SZ;
        arr.copy_from_slice(&data[daidx .. daidx + sz]);
        let da = unsafe {
            transmute::<[u8; NFFS_DISK_AREA_SZ], NffsDiskArea>(arr)
        };
        disk_area_info(
            &mut inode_map, &mut block_map, &da, i,
            &data[daidx + sz .. daidx + da.length as usize - sz]);
        daidx += da.length as usize;
        if daidx >= data.len() {
            break;
        }
        i += 1;
    }
    is_valid_fs(&inode_map, &block_map)
}

fn open_fs(filename: &String) {
    let mut f = match File::open(&filename) {
        Err(e) => panic!("Could not open \"{}\": {}", filename, e),
        Ok(file) => file,
    };

    let mut data: Vec<u8> = Vec::new();
    match f.read_to_end(&mut data) {
        Err(e) => panic!("Could not read \"{}\": {}", filename, e),
        Ok(_) => if !check_fs(data) {
            println!("Filesystem corrupt!");
            exit(1);
        },
    };
}

fn main() {
    let args: Vec<String> = env::args().collect();

    for arg in args.iter().skip(1) {
        open_fs(&arg);
    }
}
