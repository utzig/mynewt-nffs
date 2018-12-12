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

int
nffs_os_mempool_init(void)
{
    /*
     * file
     */
    nffs_file_pool = malloc(nffs_config.nc_num_files * sizeof(struct nffs_file));
    assert(nffs_file_pool);
    printf("nffs_file_pool=%p\n", nffs_file_pool);

    file_pool_inuse = malloc(nffs_config.nc_num_files);
    assert(file_pool_inuse);
    memset(file_pool_inuse, 0, nffs_config.nc_num_files);

    /*
     * dir
     */
    nffs_dir_pool = malloc(nffs_config.nc_num_dirs * sizeof(struct nffs_dir));
    assert(nffs_dir_pool);
    printf("nffs_dir_pool=%p\n", nffs_dir_pool);

    dir_pool_inuse = malloc(nffs_config.nc_num_files);
    assert(dir_pool_inuse);
    memset(dir_pool_inuse, 0, nffs_config.nc_num_dirs);

    /*
     * inode
     */
    nffs_inode_entry_pool = malloc(nffs_config.nc_num_inodes * sizeof(struct nffs_inode_entry));
    assert(nffs_inode_entry_pool);
    printf("nffs_inode_entry_pool=%p\n", nffs_inode_entry_pool);

    inode_pool_inuse = malloc(nffs_config.nc_num_inodes);
    assert(inode_pool_inuse);
    memset(inode_pool_inuse, 0, nffs_config.nc_num_inodes);

    /*
     * block
     */
    nffs_block_entry_pool = malloc(nffs_config.nc_num_blocks * sizeof(struct nffs_hash_entry));
    assert(nffs_block_entry_pool);
    printf("nffs_block_entry_pool=%p\n", nffs_block_entry_pool);

    block_pool_inuse = malloc(nffs_config.nc_num_blocks);
    assert(block_pool_inuse);
    memset(block_pool_inuse, 0, nffs_config.nc_num_blocks);

    /*
     * icache
     */
    nffs_cache_inode_pool = malloc(nffs_config.nc_num_cache_inodes * sizeof(struct nffs_cache_inode));
    assert(nffs_cache_inode_pool);
    printf("nffs_cache_inode_pool=%p\n", nffs_cache_inode_pool);

    icache_pool_inuse = malloc(nffs_config.nc_num_cache_inodes);
    assert(icache_pool_inuse);
    memset(icache_pool_inuse, 0, nffs_config.nc_num_cache_inodes);

    /*
     * bcache
     */
    nffs_cache_block_pool = malloc(nffs_config.nc_num_cache_blocks * sizeof(struct nffs_cache_block));
    assert(nffs_cache_block_pool);
    printf("nffs_cache_block_pool=%p\n", nffs_cache_block_pool);

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
                return (struct nffs_file *)nffs_file_pool + i * sizeof(struct nffs_file);
            }
        }
    } else if (*pool == nffs_dir_pool) {
        for (i = 0; i < nffs_config.nc_num_dirs; i++) {
            if (dir_pool_inuse[i] == 0) {
                dir_pool_inuse[i] = 1;
                return (struct nffs_dir *)nffs_dir_pool + i * sizeof(struct nffs_dir);
            }
        }
    } else if (*pool == nffs_inode_entry_pool) {
        for (i = 0; i < nffs_config.nc_num_inodes; i++) {
            if (inode_pool_inuse[i] == 0) {
                inode_pool_inuse[i] = 1;
                return (struct nffs_inode_entry *)nffs_inode_entry_pool + i * sizeof(struct nffs_inode_entry);
            }
        }
    } else if (*pool == nffs_block_entry_pool) {
        for (i = 0; i < nffs_config.nc_num_blocks; i++) {
            if (block_pool_inuse[i] == 0) {
                block_pool_inuse[i] = 1;
                return (struct nffs_hash_entry *)nffs_block_entry_pool + i * sizeof(struct nffs_hash_entry);
            }
        }
    } else if (*pool == nffs_cache_inode_pool) {
        for (i = 0; i < nffs_config.nc_num_cache_inodes; i++) {
            if (icache_pool_inuse[i] == 0) {
                icache_pool_inuse[i] = 1;
                return (struct nffs_cache_inode *)nffs_cache_inode_pool + i * sizeof(struct nffs_cache_inode);
            }
        }
    } else if (*pool == nffs_cache_block_pool) {
        for (i = 0; i < nffs_config.nc_num_cache_blocks; i++) {
            if (bcache_pool_inuse[i] == 0) {
                bcache_pool_inuse[i] = 1;
                return (struct nffs_cache_block *)nffs_cache_block_pool + i * sizeof(struct nffs_cache_block);
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

    assert(0);

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
    (void)final;
    LOG_DBG("%s", __func__);
    return sim_crc16(initial, buf, len);
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
