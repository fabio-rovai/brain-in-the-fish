/*
 * Tardygrada VM — Memory Manager
 * Custom allocator using mmap/mprotect directly.
 * No malloc. No stdlib. Just syscalls.
 *
 * Every allocation is a page (or set of pages) that can be
 * individually locked via mprotect for immutability enforcement.
 */

#ifndef TARDY_MEMORY_H
#define TARDY_MEMORY_H

#include <stddef.h>
#include <stdbool.h>
#include "types.h"
#include "crypto.h"

/* ============================================
 * Page-aligned memory region
 * The fundamental unit of agent storage.
 * ============================================ */

typedef struct {
    void          *ptr;        /* page-aligned pointer from mmap */
    size_t         size;       /* allocation size (multiple of page size) */
    tardy_trust_t  protection; /* current protection level */
    bool           locked;     /* true if mprotect(PROT_READ) applied */
} tardy_page_t;

/* ============================================
 * Agent Memory — the full storage for one agent
 * Includes value, replicas, hashes, signatures
 * depending on trust level.
 * ============================================ */

typedef struct {
    tardy_trust_t   trust;
    tardy_page_t    primary;          /* main value page */

    /* @verified and above */
    tardy_hash_t    birth_hash;       /* SHA-256 at creation */
    bool            has_hash;

    /* @hardened and above */
    tardy_page_t   *replicas;         /* array of replica pages */
    int             replica_count;

    /* @sovereign */
    tardy_signature_t signature;      /* ed25519 from parent */
    bool              has_signature;
    uint8_t           signer_pub[32]; /* public key of signer for verify */
    tardy_hash_t     *hash_replicas;  /* replicated hashes */
    int               hash_replica_count;
} tardy_agent_memory_t;

/* ============================================
 * System — get page size once at init
 * ============================================ */

size_t tardy_page_size(void);

/* ============================================
 * Page Operations — direct syscalls
 * ============================================ */

/* Allocate page-aligned memory via mmap */
tardy_page_t tardy_page_alloc(size_t data_size);

/* Free page via munmap */
void tardy_page_free(tardy_page_t *page);

/* Lock page read-only via mprotect — CPU enforced */
int tardy_page_lock(tardy_page_t *page);

/* Unlock page for write (needed before free, or for mutable agents) */
int tardy_page_unlock(tardy_page_t *page);

/* Write data to page (must be unlocked) */
void tardy_page_write(tardy_page_t *page, const void *data, size_t len);

/* Read data from page */
void tardy_page_read(const tardy_page_t *page, void *out, size_t len);

/* ============================================
 * Agent Memory Operations
 * ============================================ */

/* Allocate agent memory at given trust level */
tardy_agent_memory_t tardy_mem_alloc(size_t data_size, tardy_trust_t trust,
                                      int replica_count);

/* Initialize: write value, then lock if immutable */
void tardy_mem_init(tardy_agent_memory_t *mem, const void *data, size_t len,
                    const tardy_keypair_t *parent_key);

/* Read with verification — checks hash/replicas/signature based on trust */
typedef enum {
    TARDY_READ_OK             = 0,
    TARDY_READ_HASH_MISMATCH  = 1,
    TARDY_READ_NO_CONSENSUS   = 2,
    TARDY_READ_SIG_INVALID    = 3,
} tardy_read_status_t;

tardy_read_status_t tardy_mem_read(const tardy_agent_memory_t *mem,
                                    void *out, size_t len);

/* Mutate — only for TARDY_TRUST_MUTABLE, fails otherwise */
int tardy_mem_mutate(tardy_agent_memory_t *mem, const void *data, size_t len);

/* Free all memory (unlocks mprotect'd pages first) */
void tardy_mem_free(tardy_agent_memory_t *mem);

#endif /* TARDY_MEMORY_H */
