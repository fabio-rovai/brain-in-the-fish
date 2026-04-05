/*
 * Tardygrada VM — Semantic Query
 * Find agents by keyword matching against name and value.
 * Parent agents are searchable context stores.
 */

#ifndef TARDY_SEMANTIC_H
#define TARDY_SEMANTIC_H

#include "types.h"

typedef struct {
    tardy_uuid_t agent_id;
    float        score;     /* 0.0 to 1.0 — keyword overlap */
    char         name[64];  /* agent name for convenience */
} tardy_query_result_t;

#define TARDY_MAX_QUERY_RESULTS 16

/* Query agents in scope by keyword matching.
 * Searches agent names and string values.
 * Returns number of results found. */
int tardy_vm_query(void *vm_ptr, tardy_uuid_t scope,
                    const char *query,
                    tardy_query_result_t *results, int max_results);

#endif /* TARDY_SEMANTIC_H */
