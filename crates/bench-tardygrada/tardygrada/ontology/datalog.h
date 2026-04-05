#ifndef TARDY_DATALOG_H
#define TARDY_DATALOG_H

#include <stdbool.h>

#define TARDY_DL_MAX_FACTS   4096
#define TARDY_DL_MAX_RULES   128
#define TARDY_DL_MAX_BODY    4
#define TARDY_DL_MAX_STR     128

/* A ground atom: predicate(arg1, arg2) */
typedef struct {
    char pred[64];
    char arg1[TARDY_DL_MAX_STR];
    char arg2[TARDY_DL_MAX_STR];
} tardy_dl_atom_t;

/* A rule: head :- body[0], body[1], ...
 * Variables are uppercase single letters: X, Y, Z
 * Constants are lowercase strings */
typedef struct {
    tardy_dl_atom_t head;
    tardy_dl_atom_t body[TARDY_DL_MAX_BODY];
    int             body_count;
} tardy_dl_rule_t;

/* The Datalog program */
typedef struct {
    tardy_dl_atom_t facts[TARDY_DL_MAX_FACTS];
    int             fact_count;
    tardy_dl_rule_t rules[TARDY_DL_MAX_RULES];
    int             rule_count;
    int             base_fact_count;  /* facts before derivation */
    bool            evaluated;        /* has fixpoint been reached? */
} tardy_dl_program_t;

/* Initialize empty program */
void tardy_dl_init(tardy_dl_program_t *prog);

/* Add a ground fact */
int tardy_dl_add_fact(tardy_dl_program_t *prog,
                       const char *pred, const char *arg1, const char *arg2);

/* Add a rule */
int tardy_dl_add_rule(tardy_dl_program_t *prog, const tardy_dl_rule_t *rule);

/* Helper: build a rule from strings
 * e.g., tardy_dl_make_rule("locatedIn", "X", "Y", "capital", "X", "Y", NULL)
 * means: locatedIn(X, Y) :- capital(X, Y) */
tardy_dl_rule_t tardy_dl_make_rule(
    const char *head_pred, const char *head_a1, const char *head_a2,
    const char *b1_pred, const char *b1_a1, const char *b1_a2,
    const char *b2_pred, const char *b2_a1, const char *b2_a2);

/* Run semi-naive evaluation to fixpoint */
int tardy_dl_evaluate(tardy_dl_program_t *prog);

/* Query: can this atom be derived?
 * Returns: 1 = yes (grounded), 0 = no (unknown), -1 = contradicted */
int tardy_dl_query(const tardy_dl_program_t *prog,
                    const char *pred, const char *arg1, const char *arg2);

/* Load the synthetic backbone rules */
void tardy_dl_load_backbone(tardy_dl_program_t *prog);

/* Load facts from N-Triples file */
int tardy_dl_load_nt(tardy_dl_program_t *prog, const char *path);

#endif
