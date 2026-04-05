/*
 * Tardygrada VM — Constitution Implementation
 * Runtime enforcement of agent invariants.
 */

#include "constitution.h"
#include <string.h>

/* Compute hash over all invariants */
static void compute_constitutional_hash(tardy_constitution_t *con)
{
    if (con->count == 0) {
        /* Empty constitution gets a hash of zero bytes */
        uint8_t zero = 0;
        tardy_sha256(&zero, sizeof(zero), &con->constitutional_hash);
        return;
    }

    /* Hash the invariant array */
    tardy_sha256(con->invariants,
                 (size_t)con->count * sizeof(tardy_invariant_t),
                 &con->constitutional_hash);
}

void tardy_constitution_init(tardy_constitution_t *con)
{
    if (!con)
        return;

    memset(con, 0, sizeof(tardy_constitution_t));
    compute_constitutional_hash(con);
}

int tardy_constitution_add(tardy_constitution_t *con,
                            tardy_invariant_t inv)
{
    if (!con)
        return -1;

    if (con->count >= TARDY_MAX_INVARIANTS)
        return -1;

    con->invariants[con->count] = inv;
    con->count++;
    compute_constitutional_hash(con);

    return 0;
}

int tardy_constitution_check(const tardy_constitution_t *con,
                              tardy_type_t type, tardy_trust_t trust,
                              const void *value, size_t value_len)
{
    if (!con)
        return -1;

    /* Empty constitution — always passes */
    if (con->count == 0)
        return 0;

    for (int i = 0; i < con->count; i++) {
        const tardy_invariant_t *inv = &con->invariants[i];

        switch (inv->type) {
        case TARDY_INVARIANT_TYPE_CHECK:
            if (type != inv->type_arg)
                return -1;
            break;

        case TARDY_INVARIANT_RANGE:
            if (type != TARDY_TYPE_INT)
                return -1;
            if (value && value_len >= sizeof(int64_t)) {
                int64_t val;
                memcpy(&val, value, sizeof(int64_t));
                if (val < inv->min_val || val > inv->max_val)
                    return -1;
            }
            break;

        case TARDY_INVARIANT_NON_EMPTY:
            if (type != TARDY_TYPE_STR)
                return -1;
            if (!value || value_len == 0)
                return -1;
            /* Check the string isn't just a null terminator */
            if (((const char *)value)[0] == '\0')
                return -1;
            break;

        case TARDY_INVARIANT_TRUST_MIN:
            if (trust < inv->trust_arg)
                return -1;
            break;
        }
    }

    return 0;
}

int tardy_constitution_verify_integrity(const tardy_constitution_t *con)
{
    if (!con)
        return -1;

    tardy_hash_t recomputed;

    if (con->count == 0) {
        uint8_t zero = 0;
        tardy_sha256(&zero, sizeof(zero), &recomputed);
    } else {
        tardy_sha256(con->invariants,
                     (size_t)con->count * sizeof(tardy_invariant_t),
                     &recomputed);
    }

    if (!tardy_hash_eq(&recomputed, &con->constitutional_hash))
        return -1;

    return 0;
}
