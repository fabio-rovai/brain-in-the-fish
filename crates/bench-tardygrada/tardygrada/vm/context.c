/*
 * Tardygrada VM — Context Pointer Implementation
 */

#include "context.h"
#include <string.h>
#include <time.h>

/* ============================================
 * Timestamp — monotonic nanoseconds
 * ============================================ */

static tardy_timestamp_t now_ns(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/* ============================================
 * UUID generation — simple xorshift64 PRNG
 * Good enough for agent IDs. Not crypto.
 * ============================================ */

static uint64_t rng_state = 0;

static uint64_t xorshift64(void)
{
    if (rng_state == 0)
        rng_state = now_ns() ^ 0xdeadbeefcafe1234ULL;
    rng_state ^= rng_state << 13;
    rng_state ^= rng_state >> 7;
    rng_state ^= rng_state << 17;
    return rng_state;
}

tardy_uuid_t tardy_uuid_gen(void)
{
    tardy_uuid_t id;
    id.hi = xorshift64();
    id.lo = xorshift64();
    return id;
}

bool tardy_uuid_eq(const tardy_uuid_t *a, const tardy_uuid_t *b)
{
    return a->hi == b->hi && a->lo == b->lo;
}

/* ============================================
 * Context Pointer — Create
 * ============================================ */

tardy_ctx_ptr_t tardy_ctx_create(tardy_agent_t *agent, tardy_uuid_t owner)
{
    tardy_ctx_ptr_t ptr = {0};
    ptr.memory   = &agent->memory;
    ptr.agent_id = agent->id;
    ptr.trust    = agent->trust;
    ptr.type_tag = agent->type_tag;
    ptr.owner_id = owner;
    return ptr;
}

/* ============================================
 * Context Pointer — Dereference
 *
 * This is THE fundamental operation.
 * Every variable read in Tardygrada goes through here.
 *
 * For TRUST_DEFAULT: this compiles to ~3 instructions.
 * For TRUST_SOVEREIGN: full BFT + hash + sig check.
 * ============================================ */

tardy_read_status_t tardy_ctx_deref(const tardy_ctx_ptr_t *ptr,
                                     void *out, size_t len)
{
    if (!ptr || !ptr->memory)
        return TARDY_READ_HASH_MISMATCH;

    /* Delegate to memory's trust-aware read */
    return tardy_mem_read(ptr->memory, out, len);
}

/* ============================================
 * Type Check
 * ============================================ */

bool tardy_ctx_type_check(const tardy_ctx_ptr_t *ptr, tardy_type_t expected)
{
    if (!ptr)
        return false;
    return ptr->type_tag == expected;
}

/* ============================================
 * Ontology Check — context compatibility
 * An agent in Physics ontology can't be read by Literature ontology
 * unless explicitly bridged.
 * ============================================ */

bool tardy_ctx_ontology_check(const tardy_ctx_ptr_t *ptr,
                               tardy_uuid_t accessor_ontology)
{
    /* Ontology compatibility check.
     * Currently: same owner = same context = allowed.
     * With ontology bridge: check if accessor's ontology
     * is compatible with the pointer's ontology via OWL reasoning. */
    return tardy_uuid_eq(&ptr->owner_id, &accessor_ontology);
}
