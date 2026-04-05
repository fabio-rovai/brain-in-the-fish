/*
 * Tardygrada VM — Constitution Checking
 * Invariants enforced on every operation.
 * A constitution cannot be forgotten or ignored.
 */

#ifndef TARDY_CONSTITUTION_H
#define TARDY_CONSTITUTION_H

#include "types.h"
#include "crypto.h"

typedef enum {
    TARDY_INVARIANT_TYPE_CHECK,    /* value must be this type */
    TARDY_INVARIANT_RANGE,         /* int must be in [min, max] */
    TARDY_INVARIANT_NON_EMPTY,     /* string must not be empty */
    TARDY_INVARIANT_TRUST_MIN,     /* must have minimum trust level */
} tardy_invariant_type_t;

typedef struct {
    tardy_invariant_type_t type;
    int64_t                min_val;    /* for RANGE */
    int64_t                max_val;    /* for RANGE */
    tardy_type_t           type_arg;   /* for TYPE_CHECK */
    tardy_trust_t          trust_arg;  /* for TRUST_MIN */
} tardy_invariant_t;

#define TARDY_MAX_INVARIANTS 16

typedef struct {
    tardy_invariant_t invariants[TARDY_MAX_INVARIANTS];
    int               count;
    tardy_hash_t      constitutional_hash; /* hash of all invariants */
} tardy_constitution_t;

/* Initialize constitution and compute hash */
void tardy_constitution_init(tardy_constitution_t *con);

/* Add an invariant */
int tardy_constitution_add(tardy_constitution_t *con,
                            tardy_invariant_t inv);

/* Check all invariants against an agent. Returns 0 if all pass, -1 if any fail. */
int tardy_constitution_check(const tardy_constitution_t *con,
                              tardy_type_t type, tardy_trust_t trust,
                              const void *value, size_t value_len);

/* Verify the constitution itself hasn't been tampered with */
int tardy_constitution_verify_integrity(const tardy_constitution_t *con);

#endif /* TARDY_CONSTITUTION_H */
