/*
 * Tardygrada VM — Memory Manager Implementation
 * Direct syscalls. No malloc. No stdlib allocator.
 */

#include "memory.h"
#include "crypto.h"
#include <sys/mman.h>
#include <unistd.h>
#include <string.h>

/* ============================================
 * System
 * ============================================ */

static size_t cached_page_size = 0;

size_t tardy_page_size(void)
{
    if (cached_page_size == 0)
        cached_page_size = (size_t)sysconf(_SC_PAGESIZE);
    return cached_page_size;
}

/* Round up to page boundary */
static size_t align_to_page(size_t size)
{
    size_t ps = tardy_page_size();
    return (size + ps - 1) & ~(ps - 1);
}

/* ============================================
 * Page Operations
 * ============================================ */

tardy_page_t tardy_page_alloc(size_t data_size)
{
    tardy_page_t page = {0};
    page.size = align_to_page(data_size);
    page.ptr = mmap(NULL, page.size,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANONYMOUS,
                    -1, 0);
    if (page.ptr == MAP_FAILED)
        page.ptr = NULL;
    page.locked = false;
    page.protection = TARDY_TRUST_MUTABLE;
    return page;
}

void tardy_page_free(tardy_page_t *page)
{
    if (!page || !page->ptr)
        return;
    /* Must unlock before freeing */
    if (page->locked)
        tardy_page_unlock(page);
    munmap(page->ptr, page->size);
    page->ptr = NULL;
    page->size = 0;
}

int tardy_page_lock(tardy_page_t *page)
{
    if (!page || !page->ptr)
        return -1;
    int ret = mprotect(page->ptr, page->size, PROT_READ);
    if (ret == 0)
        page->locked = true;
    return ret;
}

int tardy_page_unlock(tardy_page_t *page)
{
    if (!page || !page->ptr)
        return -1;
    int ret = mprotect(page->ptr, page->size, PROT_READ | PROT_WRITE);
    if (ret == 0)
        page->locked = false;
    return ret;
}

void tardy_page_write(tardy_page_t *page, const void *data, size_t len)
{
    if (!page || !page->ptr || page->locked)
        return;
    memcpy(page->ptr, data, len);
}

void tardy_page_read(const tardy_page_t *page, void *out, size_t len)
{
    if (!page || !page->ptr)
        return;
    memcpy(out, page->ptr, len);
}

/* ============================================
 * Agent Memory — Allocation
 * ============================================ */

tardy_agent_memory_t tardy_mem_alloc(size_t data_size, tardy_trust_t trust,
                                      int replica_count)
{
    tardy_agent_memory_t mem = {0};
    mem.trust = trust;

    /* Primary page — always allocated */
    mem.primary = tardy_page_alloc(data_size);

    /* @hardened / @sovereign: allocate replicas */
    if (trust >= TARDY_TRUST_HARDENED && replica_count > 0) {
        /* Use mmap for the replica array itself */
        size_t arr_size = align_to_page(sizeof(tardy_page_t) * replica_count);
        mem.replicas = mmap(NULL, arr_size, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (mem.replicas == MAP_FAILED)
            mem.replicas = NULL;
        mem.replica_count = replica_count;
        for (int i = 0; i < replica_count; i++)
            mem.replicas[i] = tardy_page_alloc(data_size);
    }

    /* @sovereign: allocate hash replicas */
    if (trust >= TARDY_TRUST_SOVEREIGN && replica_count > 0) {
        size_t harr_size = align_to_page(sizeof(tardy_hash_t) * replica_count);
        mem.hash_replicas = mmap(NULL, harr_size, PROT_READ | PROT_WRITE,
                                MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (mem.hash_replicas == MAP_FAILED)
            mem.hash_replicas = NULL;
        mem.hash_replica_count = replica_count;
    }

    return mem;
}

/* ============================================
 * Agent Memory — Initialize and Lock
 * ============================================ */

void tardy_mem_init(tardy_agent_memory_t *mem, const void *data, size_t len,
                    const tardy_keypair_t *parent_key)
{
    if (!mem || !mem->primary.ptr)
        return;

    /* Write value to primary page */
    tardy_page_write(&mem->primary, data, len);

    /* @verified+: compute birth hash */
    if (mem->trust >= TARDY_TRUST_VERIFIED) {
        tardy_sha256(data, len, &mem->birth_hash);
        mem->has_hash = true;
    }

    /* @hardened+: write to all replicas */
    if (mem->trust >= TARDY_TRUST_HARDENED && mem->replicas) {
        for (int i = 0; i < mem->replica_count; i++)
            tardy_page_write(&mem->replicas[i], data, len);
    }

    /* @sovereign: sign the hash, replicate hashes */
    if (mem->trust >= TARDY_TRUST_SOVEREIGN && parent_key) {
        tardy_sign(parent_key, &mem->birth_hash, sizeof(tardy_hash_t),
                   &mem->signature);
        mem->has_signature = true;
        memcpy(mem->signer_pub, parent_key->public, 32);

        if (mem->hash_replicas) {
            for (int i = 0; i < mem->hash_replica_count; i++)
                mem->hash_replicas[i] = mem->birth_hash;
        }
    }

    /* Lock primary page if immutable */
    if (mem->trust >= TARDY_TRUST_DEFAULT)
        tardy_page_lock(&mem->primary);

    /* Lock replica pages */
    if (mem->trust >= TARDY_TRUST_HARDENED && mem->replicas) {
        for (int i = 0; i < mem->replica_count; i++)
            tardy_page_lock(&mem->replicas[i]);
    }
}

/* ============================================
 * Agent Memory — Verified Read
 * ============================================ */

/* Byzantine majority — generic byte-level vote.
 * Hashes each replica and votes on hashes, then returns
 * the data from the majority replica. Works for any data size. */
static bool byzantine_majority(const tardy_page_t *replicas, int count,
                                void *out, size_t len)
{
    if (count > 16) count = 16;

    /* Hash each replica's data */
    tardy_hash_t hashes[16];
    for (int i = 0; i < count; i++) {
        uint8_t tmp[4096];
        size_t read_len = len < sizeof(tmp) ? len : sizeof(tmp);
        tardy_page_read(&replicas[i], tmp, read_len);
        tardy_sha256(tmp, read_len, &hashes[i]);
    }

    /* Vote on hashes */
    for (int i = 0; i < count; i++) {
        int votes = 0;
        for (int j = 0; j < count; j++) {
            if (tardy_hash_eq(&hashes[j], &hashes[i]))
                votes++;
        }
        if (votes > count / 2) {
            /* Majority found — read from this replica */
            tardy_page_read(&replicas[i], out, len);
            return true;
        }
    }
    return false;
}

tardy_read_status_t tardy_mem_read(const tardy_agent_memory_t *mem,
                                    void *out, size_t len)
{
    if (!mem || !mem->primary.ptr)
        return TARDY_READ_HASH_MISMATCH;

    /* MUTABLE / DEFAULT: just read. mprotect guards immutability. */
    if (mem->trust <= TARDY_TRUST_DEFAULT) {
        tardy_page_read(&mem->primary, out, len);
        return TARDY_READ_OK;
    }

    /* @verified: read + hash check */
    if (mem->trust == TARDY_TRUST_VERIFIED) {
        tardy_page_read(&mem->primary, out, len);
        if (mem->has_hash) {
            tardy_hash_t current;
            tardy_sha256(out, len, &current);
            if (!tardy_hash_eq(&current, &mem->birth_hash))
                return TARDY_READ_HASH_MISMATCH;
        }
        return TARDY_READ_OK;
    }

    /* @hardened: Byzantine vote + hash check */
    if (mem->trust == TARDY_TRUST_HARDENED) {
        if (!byzantine_majority(mem->replicas, mem->replica_count, out, len))
            return TARDY_READ_NO_CONSENSUS;

        if (mem->has_hash) {
            tardy_hash_t current;
            tardy_sha256(out, len, &current);
            if (!tardy_hash_eq(&current, &mem->birth_hash))
                return TARDY_READ_HASH_MISMATCH;
        }

        return TARDY_READ_OK;
    }

    /* @sovereign: vote + hash + signature verification */
    if (mem->trust == TARDY_TRUST_SOVEREIGN) {
        if (!byzantine_majority(mem->replicas, mem->replica_count, out, len))
            return TARDY_READ_NO_CONSENSUS;

        if (mem->has_hash) {
            tardy_hash_t current;
            tardy_sha256(out, len, &current);
            if (!tardy_hash_eq(&current, &mem->birth_hash))
                return TARDY_READ_HASH_MISMATCH;
        }

        if (mem->has_signature) {
            /* Verify signature against stored signer public key.
             * The signature was made over the birth_hash at init time. */
            if (!tardy_verify(mem->signer_pub,
                              &mem->birth_hash, sizeof(tardy_hash_t),
                              &mem->signature))
                return TARDY_READ_SIG_INVALID;
        }

        return TARDY_READ_OK;
    }

    return TARDY_READ_OK;
}

/* ============================================
 * Agent Memory — Mutation (mutable only)
 * ============================================ */

int tardy_mem_mutate(tardy_agent_memory_t *mem, const void *data, size_t len)
{
    if (!mem || !mem->primary.ptr)
        return -1;

    /* Only mutable agents can be mutated */
    if (mem->trust != TARDY_TRUST_MUTABLE)
        return -1;

    tardy_page_write(&mem->primary, data, len);
    return 0;
}

/* ============================================
 * Agent Memory — Free
 * ============================================ */

void tardy_mem_free(tardy_agent_memory_t *mem)
{
    if (!mem)
        return;

    tardy_page_free(&mem->primary);

    if (mem->replicas) {
        for (int i = 0; i < mem->replica_count; i++)
            tardy_page_free(&mem->replicas[i]);
        munmap(mem->replicas,
               align_to_page(sizeof(tardy_page_t) * mem->replica_count));
        mem->replicas = NULL;
    }

    if (mem->hash_replicas) {
        munmap(mem->hash_replicas,
               align_to_page(sizeof(tardy_hash_t) * mem->hash_replica_count));
        mem->hash_replicas = NULL;
    }
}
