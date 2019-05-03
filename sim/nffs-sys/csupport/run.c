/* Run the boot image. */

#include <assert.h>
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#include "nffs/nffs.h"

#define LOG_LEVEL LOG_LEVEL_ERROR
#include "logging.h"

nffs_os_mempool_t nffs_file_pool;
nffs_os_mempool_t nffs_dir_pool;
nffs_os_mempool_t nffs_inode_entry_pool;
nffs_os_mempool_t nffs_block_entry_pool;
nffs_os_mempool_t nffs_cache_inode_pool;
nffs_os_mempool_t nffs_cache_block_pool;

static uint8_t * file_pool_inuse;
static uint8_t * dir_pool_inuse;
static uint8_t * inode_pool_inuse;
static uint8_t * block_pool_inuse;
static uint8_t * icache_pool_inuse;
static uint8_t * bcache_pool_inuse;

struct nffs_config {
    uint32_t nc_num_inodes;
    uint32_t nc_num_blocks;
    uint32_t nc_num_files;
    uint32_t nc_num_dirs;
    uint32_t nc_num_cache_inodes;
    uint32_t nc_num_cache_blocks;
};

const struct nffs_config nffs_config_dflt = {
    .nc_num_inodes = 100,
    .nc_num_blocks = 100,
    .nc_num_files = 4,
    .nc_num_cache_inodes = 4,
    .nc_num_cache_blocks = 64,
    .nc_num_dirs = 4,
};

struct nffs_config nffs_config;

void
nffs_init(void)
{
    if (nffs_config.nc_num_inodes == 0) {
        nffs_config.nc_num_inodes = nffs_config_dflt.nc_num_inodes;
    }
    if (nffs_config.nc_num_blocks == 0) {
        nffs_config.nc_num_blocks = nffs_config_dflt.nc_num_blocks;
    }
    if (nffs_config.nc_num_files == 0) {
        nffs_config.nc_num_files = nffs_config_dflt.nc_num_files;
    }
    if (nffs_config.nc_num_cache_inodes == 0) {
        nffs_config.nc_num_cache_inodes = nffs_config_dflt.nc_num_cache_inodes;
    }
    if (nffs_config.nc_num_cache_blocks == 0) {
        nffs_config.nc_num_cache_blocks = nffs_config_dflt.nc_num_cache_blocks;
    }
    if (nffs_config.nc_num_dirs == 0) {
        nffs_config.nc_num_dirs = nffs_config_dflt.nc_num_dirs;
    }
}

extern int sim_flash_erase(uint32_t offset, uint32_t size);
extern int sim_flash_read(uint32_t offset, uint8_t *dest,
        uint32_t size);
extern int sim_flash_write(uint32_t offset, const uint8_t *src,
        uint32_t size);
extern int sim_flash_info(uint32_t sector, uint32_t *address,
        uint32_t *size);
extern uint16_t sim_crc16(uint16_t initial, const uint8_t *buf, int len);

static jmp_buf sim_jmpbuf;
int flash_counter;

int jumped = 0;
uint8_t c_asserts = 0;
uint8_t c_catch_asserts = 0;

static struct nffs_area_desc *area_descs;

//int nffs_dir_open(const char *path, struct nffs_dir **out_dir);
int nffs_file_open(struct nffs_file **out_file, const char *path,
        uint8_t access_flags);
//int nffs_file_seek(struct nffs_file *file, uint32_t offset);
int nffs_write_to_file(struct nffs_file *file, const void *data, int len);
int nffs_path_unlink(const char *path);
int nffs_path_rename(const char *from, const char *to);

/* Free resources */
//int nffs_dir_close(struct nffs_dir *dir);
int nffs_file_close(struct nffs_file *file);

#define END                  0
#define FILE_OPEN            1
#define WRITE_TO_FILE        2
#define PATH_RENAME          3
#define PATH_UNLINK          4
#define RESTORE              5
#define FORMAT               6

struct _file_open_data {
    int name_len;
    char name[256];
};

struct _write_to_file_data {
    int len;
    uint8_t c;
};

struct _path_rename_data {
    int name_len_a;
    int name_len_b;
    char name_a[256];
    char name_b[256];
};

struct _path_unlink_data {
    int name_len;
    char name[256];
};

struct script_cmd {
    uint8_t cmd;
    union {
        struct _file_open_data     file_open_data;
        struct _write_to_file_data write_to_file_data;
        struct _path_rename_data   path_rename_data;
        struct _path_unlink_data   path_unlink_data;
    };
};

/******************************************************************/

int
invoke_test_script(struct nffs_area_desc *adesc, struct script_cmd *cmds)
{
    int rc;
    struct nffs_file *f;
    char path[256];
    char pathb[256];
    size_t BUFSZ = 1024;
    char buf[BUFSZ];
    int len;
    size_t i;

    area_descs = adesc;
    if (setjmp(sim_jmpbuf) == 0) {
        for (i = 0; cmds[i].cmd != END; i++) {
            switch (cmds[i].cmd) {
            case FILE_OPEN:
                len = cmds[i].file_open_data.name_len;
                memcpy(path, cmds[i].file_open_data.name, len);
                path[len] = 0;
                rc = nffs_file_open(&f, path, FS_ACCESS_WRITE | FS_ACCESS_APPEND);
                assert(rc == 0);
                break;
            case WRITE_TO_FILE:
                len = cmds[i].write_to_file_data.len;
                assert((size_t)len <= BUFSZ);
                memset(buf, cmds[i].write_to_file_data.c, len);
                rc = nffs_write_to_file(f, buf, len);
                assert(rc == 0);
                break;
            case PATH_RENAME:
                len = cmds[i].path_rename_data.name_len_a;
                memcpy(path, cmds[i].path_rename_data.name_a, len);
                path[len] = 0;
                len = cmds[i].path_rename_data.name_len_b;
                memcpy(pathb, cmds[i].path_rename_data.name_b, len);
                pathb[len] = 0;
                rc = nffs_path_rename(path, pathb);
                assert(rc == 0);
                break;
            case PATH_UNLINK:
                len = cmds[i].path_unlink_data.name_len;
                memcpy(path, cmds[i].path_unlink_data.name, len);
                path[len] = 0;
                rc = nffs_path_unlink(path);
                assert(rc == 0);
                break;
            case RESTORE:
                rc = nffs_restore_full(area_descs);
                assert(rc == 0);
                break;
            case FORMAT:
                rc = nffs_format_full(area_descs);
                assert(rc == 0);
                break;
            default:
                assert(0);
            }
        }
        area_descs = NULL;
        return rc;
    } else {
        area_descs = NULL;
        return -0x13579;
    }
}

int invoke_format(struct nffs_area_desc *adesc)
{
    int res;

    area_descs = adesc;
    if (setjmp(sim_jmpbuf) == 0) {
        res = nffs_format_full(area_descs);
        area_descs = NULL;
        return res;
    } else {
        area_descs = NULL;
        return -0x13579;
    }
}

int invoke_restore(struct nffs_area_desc *adesc)
{
    int res;

    area_descs = adesc;
    if (setjmp(sim_jmpbuf) == 0) {
        res = nffs_restore_full(area_descs);
        area_descs = NULL;
        return res;
    } else {
        area_descs = NULL;
        return -0x13579;
    }
}

/*
 * Open file and write contents.
 *
 * NOTE: if file does not exist, it is created;
 * NOTE: if len of data is zero, only inode is created;
 */
int invoke_write_to_file(struct nffs_area_desc *adesc, char *name, uint8_t namelen,
        uint8_t *data, int len)
{
    int res;
    struct nffs_file *f;
    char fname[256];

    area_descs = adesc;
    if (setjmp(sim_jmpbuf) == 0) {
        memcpy(fname, name, namelen);
        fname[namelen] = 0;
        res = nffs_file_open(&f, fname, FS_ACCESS_WRITE | FS_ACCESS_APPEND);
        if (res == 0 && len > 0) {
            res = nffs_write_to_file(f, data, len);
        }
        area_descs = NULL;
        return res;
    } else {
        area_descs = NULL;
        return -0x13579;
    }
}

int invoke_path_rename(struct nffs_area_desc *adesc, char *oldname, char *newname)
{
    int res;

    area_descs = adesc;
    if (setjmp(sim_jmpbuf) == 0) {
        res = nffs_path_rename(oldname, newname);
        area_descs = NULL;
        return res;
    } else {
        area_descs = NULL;
        return -0x13579;
    }
}

int
nffs_os_mempool_init(void)
{
    /*
     * file
     */
    nffs_file_pool = malloc(nffs_config.nc_num_files * sizeof(struct nffs_file));
    assert(nffs_file_pool);

    file_pool_inuse = malloc(nffs_config.nc_num_files);
    assert(file_pool_inuse);
    memset(file_pool_inuse, 0, nffs_config.nc_num_files);

    /*
     * dir
     */
    nffs_dir_pool = malloc(nffs_config.nc_num_dirs * sizeof(struct nffs_dir));
    assert(nffs_dir_pool);

    dir_pool_inuse = malloc(nffs_config.nc_num_files);
    assert(dir_pool_inuse);
    memset(dir_pool_inuse, 0, nffs_config.nc_num_dirs);

    /*
     * inode
     */
    nffs_inode_entry_pool = malloc(nffs_config.nc_num_inodes * sizeof(struct nffs_inode_entry));
    assert(nffs_inode_entry_pool);

    inode_pool_inuse = malloc(nffs_config.nc_num_inodes);
    assert(inode_pool_inuse);
    memset(inode_pool_inuse, 0, nffs_config.nc_num_inodes);

    /*
     * block
     */
    nffs_block_entry_pool = malloc(nffs_config.nc_num_blocks * sizeof(struct nffs_hash_entry));
    assert(nffs_block_entry_pool);

    block_pool_inuse = malloc(nffs_config.nc_num_blocks);
    assert(block_pool_inuse);
    memset(block_pool_inuse, 0, nffs_config.nc_num_blocks);

    /*
     * icache
     */
    nffs_cache_inode_pool = malloc(nffs_config.nc_num_cache_inodes * sizeof(struct nffs_cache_inode));
    assert(nffs_cache_inode_pool);

    icache_pool_inuse = malloc(nffs_config.nc_num_cache_inodes);
    assert(icache_pool_inuse);
    memset(icache_pool_inuse, 0, nffs_config.nc_num_cache_inodes);

    /*
     * bcache
     */
    nffs_cache_block_pool = malloc(nffs_config.nc_num_cache_blocks * sizeof(struct nffs_cache_block));
    assert(nffs_cache_block_pool);

    bcache_pool_inuse = malloc(nffs_config.nc_num_cache_blocks);
    assert(bcache_pool_inuse);
    memset(bcache_pool_inuse, 0, nffs_config.nc_num_cache_blocks);

    return 0;
}

void *
nffs_os_mempool_get(nffs_os_mempool_t *pool)
{
    uint32_t i;

    if (*pool == nffs_file_pool) {
        for (i = 0; i < nffs_config.nc_num_files; i++) {
            if (file_pool_inuse[i] == 0) {
                file_pool_inuse[i] = 1;
                return (struct nffs_file *)nffs_file_pool + i;
            }
        }
    } else if (*pool == nffs_dir_pool) {
        for (i = 0; i < nffs_config.nc_num_dirs; i++) {
            if (dir_pool_inuse[i] == 0) {
                dir_pool_inuse[i] = 1;
                return (struct nffs_dir *)nffs_dir_pool + i;
            }
        }
    } else if (*pool == nffs_inode_entry_pool) {
        for (i = 0; i < nffs_config.nc_num_inodes; i++) {
            if (inode_pool_inuse[i] == 0) {
                inode_pool_inuse[i] = 1;
                return (struct nffs_inode_entry *)nffs_inode_entry_pool + i;
            }
        }
    } else if (*pool == nffs_block_entry_pool) {
        for (i = 0; i < nffs_config.nc_num_blocks; i++) {
            if (block_pool_inuse[i] == 0) {
                block_pool_inuse[i] = 1;
                return (struct nffs_hash_entry *)nffs_block_entry_pool + i;
            }
        }
    } else if (*pool == nffs_cache_inode_pool) {
        for (i = 0; i < nffs_config.nc_num_cache_inodes; i++) {
            if (icache_pool_inuse[i] == 0) {
                icache_pool_inuse[i] = 1;
                return (struct nffs_cache_inode *)nffs_cache_inode_pool + i;
            }
        }
    } else if (*pool == nffs_cache_block_pool) {
        for (i = 0; i < nffs_config.nc_num_cache_blocks; i++) {
            if (bcache_pool_inuse[i] == 0) {
                bcache_pool_inuse[i] = 1;
                return (struct nffs_cache_block *)nffs_cache_block_pool + i;
            }
        }
    }

    /* looking for a pool that does not exist is a bug */
    assert(0);
    return NULL;
}

int
nffs_os_mempool_free(nffs_os_mempool_t *pool, void *block)
{
    uint32_t i;

    if (*pool == nffs_file_pool) {
        struct nffs_file *p = (struct nffs_file *)nffs_file_pool;
        for (i = 0; i < nffs_config.nc_num_files; i++) {
            if (&p[i] == (struct nffs_file *)block) {
                file_pool_inuse[i] = 0;
                return 0;
            }
        }
    } else if (*pool == nffs_dir_pool) {
        struct nffs_dir *p = (struct nffs_dir *)nffs_dir_pool;
        for (i = 0; i < nffs_config.nc_num_dirs; i++) {
            if (&p[i] == (struct nffs_dir *)block) {
                dir_pool_inuse[i] = 0;
                return 0;
            }
        }
    } else if (*pool == nffs_inode_entry_pool) {
        struct nffs_inode_entry *p = (struct nffs_inode_entry *)nffs_inode_entry_pool;
        for (i = 0; i < nffs_config.nc_num_inodes; i++) {
            if (&p[i] == (struct nffs_inode_entry *)block) {
                inode_pool_inuse[i] = 0;
                return 0;
            }
        }
    } else if (*pool == nffs_block_entry_pool) {
        struct nffs_hash_entry *p = (struct nffs_hash_entry *)nffs_block_entry_pool;
        for (i = 0; i < nffs_config.nc_num_blocks; i++) {
            if (&p[i] == (struct nffs_hash_entry *)block) {
                block_pool_inuse[i] = 0;
                return 0;
            }
        }
    } else if (*pool == nffs_cache_inode_pool) {
        struct nffs_cache_inode *p = (struct nffs_cache_inode *)nffs_cache_inode_pool;
        for (i = 0; i < nffs_config.nc_num_cache_inodes; i++) {
            if (&p[i] == (struct nffs_cache_inode *)block) {
                icache_pool_inuse[i] = 0;
                return 0;
            }
        }
    } else if (*pool == nffs_cache_block_pool) {
        struct nffs_cache_block *p = (struct nffs_cache_block *)nffs_cache_block_pool;
        for (i = 0; i < nffs_config.nc_num_cache_blocks; i++) {
            if (&p[i] == (struct nffs_cache_block *)block) {
                bcache_pool_inuse[i] = 0;
                return 0;
            }
        }
    }

    /* XXX block must probably be dereferenced */
    assert(0);
    return -1;
}

int
nffs_os_flash_read(uint8_t id, uint32_t address, void *dst,
        uint32_t num_bytes)
{
    (void)id;
    LOG_DBG("%s: id=%u, address=%x, num_bytes=%d", __func__, id, address,
            num_bytes);
    return sim_flash_read(address, dst, num_bytes);
}

int
nffs_os_flash_write(uint8_t id, uint32_t address, const void *src,
        uint32_t num_bytes)
{
    (void)id;
    LOG_DBG("%s: id=%d, address=%x, num_bytes=%x", __func__, id, address,
            num_bytes);
    if (--flash_counter == 0) {
        jumped++;
        longjmp(sim_jmpbuf, 1);
    }
    return sim_flash_write(address, src, num_bytes);
}

int
nffs_os_flash_erase(uint8_t id, uint32_t address, uint32_t num_bytes)
{
    int rc;

    (void)id;
    LOG_DBG("%s: id=%d, address=%x, num_bytes=%d", __func__, id, address,
            num_bytes);
    if (--flash_counter == 0) {
        jumped++;
        longjmp(sim_jmpbuf, 1);
    }
    rc = sim_flash_erase(address, num_bytes);
    if (rc != 0) {
        printf("address=%x, num_bytes=%u\n", address, num_bytes);
        assert (0);
    }
    return rc;
}

int
nffs_os_flash_info(uint8_t id, uint32_t sector, uint32_t *address,
        uint32_t *size)
{
    (void)id;
    LOG_DBG("%s: id=%d, sector=%x", __func__, id, sector);
    return sim_flash_info(sector, address, size);
}

uint16_t
nffs_os_crc16_ccitt(uint16_t initial, const void *buf, int len,
        int final)
{
    uint16_t crc;
    (void)final;
    LOG_DBG("%s", __func__);
    crc = sim_crc16(initial, buf, len);
    //printf("crc: initial=0x%x, len=%d, crc=0x%x\n", initial, len, crc);
    return crc;
}

void sim_assert(int x, const char *assertion, const char *file, unsigned int line, const char *function)
{
    if (!(x)) {
        if (c_catch_asserts) {
            c_asserts++;
        } else {
            LOG_ERR("%s:%d: %s: Assertion `%s' failed.", file, line, function, assertion);

            /* NOTE: if the assert below is triggered, the place where it was originally
             * asserted is printed by the message above...
             */
            assert(x);
        }
    }
}
