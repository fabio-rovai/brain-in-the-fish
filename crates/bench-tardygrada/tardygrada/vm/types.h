/*
 * Tardygrada VM — Core Types
 * Every value is an agent. Every agent carries context.
 */

#ifndef TARDY_TYPES_H
#define TARDY_TYPES_H

#include <stdint.h>
#include <stdbool.h>
#include <time.h>

/* ============================================
 * Agent Identity
 * ============================================ */

typedef struct {
    uint64_t hi;
    uint64_t lo;
} tardy_uuid_t;

/* ============================================
 * Value Types — what an agent can hold
 * ============================================ */

typedef enum {
    TARDY_TYPE_INT     = 0x01,
    TARDY_TYPE_FLOAT   = 0x02,
    TARDY_TYPE_BOOL    = 0x03,
    TARDY_TYPE_STR     = 0x04,
    TARDY_TYPE_UNIT    = 0x05,
    TARDY_TYPE_FACT    = 0x06,  /* grounded claim with evidence */
    TARDY_TYPE_AGENT   = 0x07,  /* composite: agent holding agents */
    TARDY_TYPE_ERROR   = 0x08,  /* error: queryable agent explaining what went wrong */
} tardy_type_t;

/* ============================================
 * Trust Levels — tiered immutability
 * ============================================ */

typedef enum {
    TARDY_TRUST_MUTABLE   = 0x00,  /* no let: read-write, provenance tracked */
    TARDY_TRUST_DEFAULT   = 0x01,  /* let: mprotect read-only */
    TARDY_TRUST_VERIFIED  = 0x02,  /* + SHA-256 hash check per read */
    TARDY_TRUST_HARDENED  = 0x03,  /* + N replicas + Byzantine vote */
    TARDY_TRUST_SOVEREIGN = 0x04,  /* + ed25519 signature + full BFT */
} tardy_trust_t;

/* ============================================
 * Agent Lifecycle States
 * ============================================ */

typedef enum {
    TARDY_STATE_LIVE    = 0x01,  /* full agent, all metadata */
    TARDY_STATE_STATIC  = 0x02,  /* demoted to plain value + snapshot */
    TARDY_STATE_TEMP    = 0x03,  /* resurrected temporarily */
    TARDY_STATE_DEAD    = 0x04,  /* tombstone only */
} tardy_state_t;

/* ============================================
 * Truth Strength — not boolean, a spectrum
 * ============================================ */

typedef enum {
    TARDY_TRUTH_REFUTED      = 0x00,
    TARDY_TRUTH_CONTESTED    = 0x01,
    TARDY_TRUTH_HYPOTHETICAL = 0x02,
    TARDY_TRUTH_ATTESTED     = 0x03,
    TARDY_TRUTH_EVIDENCED    = 0x04,
    TARDY_TRUTH_PROVEN       = 0x05,
    TARDY_TRUTH_AXIOMATIC    = 0x06,
} tardy_truth_strength_t;

/* ============================================
 * Knowledge Status — grounding result
 * ============================================ */

typedef enum {
    TARDY_KNOWLEDGE_GROUNDED    = 0x01,
    TARDY_KNOWLEDGE_UNKNOWN     = 0x02,
    TARDY_KNOWLEDGE_CONTRADICTED = 0x03,
    TARDY_KNOWLEDGE_CONSISTENT  = 0x04,  /* structurally valid per frame, not contradicted */
} tardy_knowledge_status_t;

/* ============================================
 * Laziness Types — VM-detected violations
 * ============================================ */

typedef enum {
    TARDY_LAZY_NONE          = 0x00,
    TARDY_LAZY_NO_WORK       = 0x01,
    TARDY_LAZY_SHALLOW       = 0x02,
    TARDY_LAZY_FAKE_PROOF    = 0x03,
    TARDY_LAZY_COPIED        = 0x04,
    TARDY_LAZY_CIRCULAR      = 0x05,
} tardy_laziness_t;

/* ============================================
 * Verification Pipeline Layer Results
 * ============================================ */

typedef enum {
    TARDY_LAYER_DECOMPOSE      = 0,
    TARDY_LAYER_GROUNDING      = 1,
    TARDY_LAYER_CONSISTENCY    = 2,
    TARDY_LAYER_PROBABILISTIC  = 3,
    TARDY_LAYER_PROTOCOL       = 4,
    TARDY_LAYER_CERTIFICATION  = 5,
    TARDY_LAYER_CROSS_REP      = 6,
    TARDY_LAYER_WORK_VERIFY    = 7,
    TARDY_LAYER_COUNT          = 8,
} tardy_layer_t;

/* ============================================
 * Timestamps — monotonic nanoseconds
 * ============================================ */

typedef uint64_t tardy_timestamp_t;

#endif /* TARDY_TYPES_H */
