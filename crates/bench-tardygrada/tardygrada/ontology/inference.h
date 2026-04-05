/*
 * Tardygrada -- Ontology Inference Engine
 *
 * Three capabilities:
 *
 * 1. Synthetic backbone: structural rules generated at startup.
 *    "If X capitalOf Y, then X located_in Y."
 *    Lets the system reason about facts it hasn't seen.
 *
 * 2. Self-healing: when a gap is found, try to infer the missing
 *    triple from existing ones via the rules.
 *
 * 3. Rule mining: learn new rules from verified claims.
 *    "Claims matching pattern P tend to verify." -> new rule.
 *
 * 4. Computational verification: if a claim contains numbers,
 *    try to verify by running the computation.
 */

#ifndef TARDY_INFERENCE_H
#define TARDY_INFERENCE_H

#include "self.h"

#define TARDY_MAX_RULES 64

/* An inference rule: if (condition) then (conclusion) */
typedef struct {
    char if_pred[64];    /* if triple has this predicate... */
    char then_pred[64];  /* ...infer a triple with this predicate */
    int  swap_so;        /* 1 = swap subject/object in conclusion */
    float confidence;    /* confidence of the inferred triple */
} tardy_rule_t;

typedef struct {
    tardy_rule_t rules[TARDY_MAX_RULES];
    int          count;
} tardy_ruleset_t;

/* Initialize with synthetic backbone rules */
void tardy_inference_init(tardy_ruleset_t *rs);

/* Try to infer missing triples from existing ontology + rules.
 * Returns number of new triples inferred. */
int tardy_inference_heal(tardy_ruleset_t *rs,
                          tardy_self_ontology_t *ont,
                          const tardy_triple_t *query, int query_count,
                          tardy_triple_t *inferred, int max_inferred);

/* Mine a new rule from a verified claim pattern.
 * Called after successful verification. */
int tardy_inference_learn(tardy_ruleset_t *rs,
                           const tardy_triple_t *triples, int count);

/* Verify a computational claim by running it.
 * Returns 1 if verified, 0 if not, -1 if not computational. */
int tardy_inference_compute(const char *claim, int len,
                             float *confidence);

#endif
