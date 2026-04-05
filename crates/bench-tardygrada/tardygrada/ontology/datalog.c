/*
 * Tardygrada -- Datalog Inference Engine
 *
 * Bottom-up semi-naive evaluation. Facts are ground atoms.
 * Rules are Horn clauses. The engine derives all consequences
 * at startup and after each new fact batch. Query = membership
 * check in derived set.
 *
 * ~400 lines, no dependencies beyond libc.
 */

#include "datalog.h"
#include <string.h>
#include <stdio.h>
#include <ctype.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/mman.h>

/* ============================================
 * Helpers
 * ============================================ */

static int is_variable(const char *s)
{
    return s && s[0] && isupper((unsigned char)s[0]);
}

static int atoms_equal(const tardy_dl_atom_t *a, const tardy_dl_atom_t *b)
{
    return strcmp(a->pred, b->pred) == 0 &&
           strcmp(a->arg1, b->arg1) == 0 &&
           strcmp(a->arg2, b->arg2) == 0;
}

/* Case-insensitive string compare */
static int ci_streq(const char *a, const char *b)
{
    if (!a || !b) return 0;
    while (*a && *b) {
        char ca = *a, cb = *b;
        if (ca >= 'A' && ca <= 'Z') ca += 32;
        if (cb >= 'A' && cb <= 'Z') cb += 32;
        if (ca != cb) return 0;
        a++; b++;
    }
    return *a == *b;
}

/* Predicate matching: exact, case-insensitive, or underscore/camelCase normalized.
 * "located_in" matches "locatedIn", "LocatedIn", "located_in", etc. */
static int pred_match(const char *a, const char *b)
{
    /* Exact match */
    if (strcmp(a, b) == 0) return 1;

    /* Case-insensitive match */
    if (ci_streq(a, b)) return 1;

    /* Normalize both: remove underscores, lowercase, then compare */
    char norm_a[64], norm_b[64];
    int ai = 0, bi = 0;
    for (int i = 0; a[i] && ai < 63; i++) {
        if (a[i] != '_')
            norm_a[ai++] = (a[i] >= 'A' && a[i] <= 'Z') ? a[i] + 32 : a[i];
    }
    norm_a[ai] = '\0';
    for (int i = 0; b[i] && bi < 63; i++) {
        if (b[i] != '_')
            norm_b[bi++] = (b[i] >= 'A' && b[i] <= 'Z') ? b[i] + 32 : b[i];
    }
    norm_b[bi] = '\0';
    return strcmp(norm_a, norm_b) == 0;
}

/* Check if fact already exists (linear scan, fine for 4K facts) */
static int fact_exists(const tardy_dl_program_t *prog,
                       const tardy_dl_atom_t *atom)
{
    for (int i = 0; i < prog->fact_count; i++) {
        if (atoms_equal(&prog->facts[i], atom))
            return 1;
    }
    return 0;
}

/* ============================================
 * Init / Add
 * ============================================ */

void tardy_dl_init(tardy_dl_program_t *prog)
{
    if (!prog) return;
    memset(prog, 0, sizeof(tardy_dl_program_t));
}

int tardy_dl_add_fact(tardy_dl_program_t *prog,
                       const char *pred, const char *arg1, const char *arg2)
{
    if (!prog || !pred || prog->fact_count >= TARDY_DL_MAX_FACTS)
        return -1;

    tardy_dl_atom_t *f = &prog->facts[prog->fact_count];
    strncpy(f->pred, pred, 63);   f->pred[63] = '\0';
    strncpy(f->arg1, arg1 ? arg1 : "", TARDY_DL_MAX_STR - 1);
    f->arg1[TARDY_DL_MAX_STR - 1] = '\0';
    strncpy(f->arg2, arg2 ? arg2 : "", TARDY_DL_MAX_STR - 1);
    f->arg2[TARDY_DL_MAX_STR - 1] = '\0';

    /* Dedup */
    if (fact_exists(prog, f)) return 0;

    prog->fact_count++;
    prog->evaluated = false;
    return 0;
}

int tardy_dl_add_rule(tardy_dl_program_t *prog, const tardy_dl_rule_t *rule)
{
    if (!prog || !rule || prog->rule_count >= TARDY_DL_MAX_RULES)
        return -1;

    prog->rules[prog->rule_count] = *rule;
    prog->rule_count++;
    prog->evaluated = false;
    return 0;
}

tardy_dl_rule_t tardy_dl_make_rule(
    const char *head_pred, const char *head_a1, const char *head_a2,
    const char *b1_pred, const char *b1_a1, const char *b1_a2,
    const char *b2_pred, const char *b2_a1, const char *b2_a2)
{
    tardy_dl_rule_t r;
    memset(&r, 0, sizeof(r));

    strncpy(r.head.pred, head_pred ? head_pred : "", 63);
    strncpy(r.head.arg1, head_a1 ? head_a1 : "", TARDY_DL_MAX_STR - 1);
    strncpy(r.head.arg2, head_a2 ? head_a2 : "", TARDY_DL_MAX_STR - 1);

    if (b1_pred) {
        strncpy(r.body[0].pred, b1_pred, 63);
        strncpy(r.body[0].arg1, b1_a1 ? b1_a1 : "", TARDY_DL_MAX_STR - 1);
        strncpy(r.body[0].arg2, b1_a2 ? b1_a2 : "", TARDY_DL_MAX_STR - 1);
        r.body_count = 1;
    }

    if (b2_pred) {
        strncpy(r.body[1].pred, b2_pred, 63);
        strncpy(r.body[1].arg1, b2_a1 ? b2_a1 : "", TARDY_DL_MAX_STR - 1);
        strncpy(r.body[1].arg2, b2_a2 ? b2_a2 : "", TARDY_DL_MAX_STR - 1);
        r.body_count = 2;
    }

    return r;
}

/* ============================================
 * Semi-Naive Evaluation
 *
 * 1. delta = base facts
 * 2. repeat:
 *      for each rule:
 *        for each way to match rule body against (facts + delta):
 *          if head instantiation is NEW: add to new_delta
 *      delta = new_delta
 *      add new_delta to facts
 *    until delta is empty (fixpoint)
 * ============================================ */

/* Binding environment: maps variable names to values */
#define MAX_BINDINGS 8

typedef struct {
    char var[8];
    char val[TARDY_DL_MAX_STR];
} binding_t;

typedef struct {
    binding_t bindings[MAX_BINDINGS];
    int       count;
} env_t;

static void env_clear(env_t *e) { e->count = 0; }

static const char *env_lookup(const env_t *e, const char *var)
{
    for (int i = 0; i < e->count; i++) {
        if (strcmp(e->bindings[i].var, var) == 0)
            return e->bindings[i].val;
    }
    return NULL;
}

static int env_bind(env_t *e, const char *var, const char *val)
{
    const char *existing = env_lookup(e, var);
    if (existing)
        return ci_streq(existing, val) ? 1 : 0;

    if (e->count >= MAX_BINDINGS) return 0;
    strncpy(e->bindings[e->count].var, var, 7);
    e->bindings[e->count].var[7] = '\0';
    strncpy(e->bindings[e->count].val, val, TARDY_DL_MAX_STR - 1);
    e->bindings[e->count].val[TARDY_DL_MAX_STR - 1] = '\0';
    e->count++;
    return 1;
}

/* Try to unify a body atom with a fact under environment e.
 * Returns 1 if successful (may extend e), 0 if not. */
static int unify_atom(const tardy_dl_atom_t *pattern,
                      const tardy_dl_atom_t *fact,
                      env_t *e)
{
    /* Predicate must match (case-insensitive + underscore/camelCase normalized) */
    if (!pred_match(pattern->pred, fact->pred))
        return 0;

    /* Unify arg1 */
    if (is_variable(pattern->arg1)) {
        if (!env_bind(e, pattern->arg1, fact->arg1))
            return 0;
    } else {
        if (!ci_streq(pattern->arg1, fact->arg1))
            return 0;
    }

    /* Unify arg2 */
    if (is_variable(pattern->arg2)) {
        if (!env_bind(e, pattern->arg2, fact->arg2))
            return 0;
    } else {
        if (!ci_streq(pattern->arg2, fact->arg2))
            return 0;
    }

    return 1;
}

/* Instantiate head atom using bindings */
static void instantiate(const tardy_dl_atom_t *head, const env_t *e,
                         tardy_dl_atom_t *out)
{
    strncpy(out->pred, head->pred, 63);
    out->pred[63] = '\0';

    if (is_variable(head->arg1)) {
        const char *v = env_lookup(e, head->arg1);
        strncpy(out->arg1, v ? v : "", TARDY_DL_MAX_STR - 1);
    } else {
        strncpy(out->arg1, head->arg1, TARDY_DL_MAX_STR - 1);
    }
    out->arg1[TARDY_DL_MAX_STR - 1] = '\0';

    if (is_variable(head->arg2)) {
        const char *v = env_lookup(e, head->arg2);
        strncpy(out->arg2, v ? v : "", TARDY_DL_MAX_STR - 1);
    } else {
        strncpy(out->arg2, head->arg2, TARDY_DL_MAX_STR - 1);
    }
    out->arg2[TARDY_DL_MAX_STR - 1] = '\0';
}

int tardy_dl_evaluate(tardy_dl_program_t *prog)
{
    if (!prog) return -1;

    prog->base_fact_count = prog->fact_count;
    int iterations = 0;
    int max_iterations = 100; /* safety bound */

    while (iterations < max_iterations) {
        int new_facts = 0;
        iterations++;

        for (int r = 0; r < prog->rule_count; r++) {
            const tardy_dl_rule_t *rule = &prog->rules[r];

            if (rule->body_count == 0) continue;

            /* Match body[0] against all facts */
            for (int f0 = 0; f0 < prog->fact_count; f0++) {
                env_t env;
                env_clear(&env);

                if (!unify_atom(&rule->body[0], &prog->facts[f0], &env))
                    continue;

                if (rule->body_count == 1) {
                    /* Single-body rule: instantiate head */
                    tardy_dl_atom_t derived;
                    instantiate(&rule->head, &env, &derived);

                    if (!fact_exists(prog, &derived) &&
                        prog->fact_count < TARDY_DL_MAX_FACTS) {
                        prog->facts[prog->fact_count++] = derived;
                        new_facts++;
                    }
                } else if (rule->body_count == 2) {
                    /* Two-body rule: join on body[1] */
                    for (int f1 = 0; f1 < prog->fact_count; f1++) {
                        env_t env2 = env; /* copy bindings from body[0] */

                        if (!unify_atom(&rule->body[1], &prog->facts[f1],
                                         &env2))
                            continue;

                        tardy_dl_atom_t derived;
                        instantiate(&rule->head, &env2, &derived);

                        if (!fact_exists(prog, &derived) &&
                            prog->fact_count < TARDY_DL_MAX_FACTS) {
                            prog->facts[prog->fact_count++] = derived;
                            new_facts++;
                        }
                    }
                }
            }
        }

        if (new_facts == 0) break; /* fixpoint reached */
    }

    prog->evaluated = true;
    return prog->fact_count - prog->base_fact_count;
}

/* ============================================
 * Query
 * ============================================ */

int tardy_dl_query(const tardy_dl_program_t *prog,
                    const char *pred, const char *arg1, const char *arg2)
{
    if (!prog || !pred) return 0;

    for (int i = 0; i < prog->fact_count; i++) {
        if (!pred_match(prog->facts[i].pred, pred))
            continue;

        int a1_match = (!arg1 || !arg1[0] ||
                        ci_streq(prog->facts[i].arg1, arg1));
        int a2_match = (!arg2 || !arg2[0] ||
                        ci_streq(prog->facts[i].arg2, arg2));

        if (a1_match && a2_match)
            return 1;
    }

    return 0;
}

/* ============================================
 * Backbone Rules — loaded at startup
 * (placeholder: implemented in Task 2)
 * ============================================ */

void tardy_dl_load_backbone(tardy_dl_program_t *prog)
{
    if (!prog) return;

    /* Spatial: capital(X, Y) -> locatedIn(X, Y) */
    tardy_dl_rule_t r;

    r = tardy_dl_make_rule("locatedIn", "X", "Y",
                            "capital", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    r = tardy_dl_make_rule("locatedIn", "X", "Y",
                            "capitalOf", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    /* contains(Y, X) :- locatedIn(X, Y) */
    r = tardy_dl_make_rule("contains", "Y", "X",
                            "locatedIn", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    /* Creation synonyms -> createdBy */
    r = tardy_dl_make_rule("createdBy", "X", "Y",
                            "creator", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    r = tardy_dl_make_rule("createdBy", "X", "Y",
                            "founder", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    r = tardy_dl_make_rule("createdBy", "X", "Y",
                            "inventor", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    r = tardy_dl_make_rule("createdBy", "X", "Y",
                            "discoverer", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    /* Reverse: creator(Y, X) :- createdBy(X, Y) */
    r = tardy_dl_make_rule("creator", "Y", "X",
                            "createdBy", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    /* Reverse: locatedIn(Y, X) :- contains(X, Y) */
    r = tardy_dl_make_rule("locatedIn", "Y", "X",
                            "contains", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    /* Temporal: createdIn(X, Y) :- dateCreated(X, Y) */
    r = tardy_dl_make_rule("createdIn", "X", "Y",
                            "dateCreated", "X", "Y", NULL, NULL, NULL);
    tardy_dl_add_rule(prog, &r);

    /* Chain: locatedIn(X, Z) :- locatedIn(X, Y), locatedIn(Y, Z) */
    r = tardy_dl_make_rule("locatedIn", "X", "Z",
                            "locatedIn", "X", "Y",
                            "locatedIn", "Y", "Z");
    tardy_dl_add_rule(prog, &r);

    /* Chain: associatedWith(X, Z) :- createdBy(X, Y), locatedIn(Y, Z) */
    r = tardy_dl_make_rule("associatedWith", "X", "Z",
                            "createdBy", "X", "Y",
                            "locatedIn", "Y", "Z");
    tardy_dl_add_rule(prog, &r);
}

/* ============================================
 * Load N-Triples file into Datalog facts
 * ============================================ */

static void dl_extract_local_name(const char *iri, char *out, int max_len)
{
    const char *last_slash = NULL;
    const char *last_hash = NULL;
    for (const char *p = iri; *p; p++) {
        if (*p == '/') last_slash = p;
        if (*p == '#') last_hash = p;
    }
    const char *start = last_hash ? last_hash + 1 :
                        (last_slash ? last_slash + 1 : iri);

    if (*start == '<') start++;
    int len = 0;
    while (start[len] && start[len] != '>' && len < max_len - 1) {
        out[len] = start[len];
        len++;
    }
    out[len] = '\0';
}

int tardy_dl_load_nt(tardy_dl_program_t *prog, const char *path)
{
    if (!prog || !path) return -1;

    int fd = open(path, O_RDONLY);
    if (fd < 0) return -1;

    struct stat st;
    if (fstat(fd, &st) < 0) { close(fd); return -1; }
    if (st.st_size > 1024 * 1024) { close(fd); return -1; }

    char *buf = (char *)mmap(NULL, (size_t)st.st_size + 1,
                              PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (buf == MAP_FAILED) { close(fd); return -1; }

    ssize_t n = read(fd, buf, (size_t)st.st_size);
    close(fd);
    if (n <= 0) { munmap(buf, (size_t)st.st_size + 1); return -1; }
    buf[n] = '\0';

    int loaded = 0;
    char *line = buf;
    while (*line) {
        while (*line == ' ' || *line == '\t' || *line == '\n' || *line == '\r')
            line++;
        if (*line == '#' || *line == '@') {
            while (*line && *line != '\n') line++;
            continue;
        }
        if (!*line) break;

        if (*line != '<') {
            while (*line && *line != '\n') line++;
            continue;
        }

        /* Extract subject */
        char *s_end = strchr(line, '>');
        if (!s_end) break;

        char subj[128];
        *s_end = '\0';
        dl_extract_local_name(line, subj, sizeof(subj));
        *s_end = '>';
        s_end++;

        /* Skip whitespace to predicate */
        char *p_start = s_end;
        while (*p_start == ' ' || *p_start == '\t') p_start++;
        if (*p_start != '<') { while (*line && *line != '\n') line++; continue; }

        char *p_end = strchr(p_start, '>');
        if (!p_end) break;

        char pred[128];
        *p_end = '\0';
        dl_extract_local_name(p_start, pred, sizeof(pred));
        *p_end = '>';
        p_end++;

        /* Skip whitespace to object */
        char *o_start = p_end;
        while (*o_start == ' ' || *o_start == '\t') o_start++;

        char obj[128];
        if (*o_start == '<') {
            char *o_end = strchr(o_start, '>');
            if (!o_end) break;
            *o_end = '\0';
            dl_extract_local_name(o_start, obj, sizeof(obj));
            *o_end = '>';
            line = o_end + 1;
        } else if (*o_start == '"') {
            o_start++;
            char *o_end = strchr(o_start, '"');
            if (!o_end) break;
            int olen = (int)(o_end - o_start);
            if (olen > 127) olen = 127;
            memcpy(obj, o_start, (size_t)olen);
            obj[olen] = '\0';
            line = o_end + 1;
        } else {
            while (*line && *line != '\n') line++;
            continue;
        }

        if (subj[0] && pred[0] && obj[0]) {
            tardy_dl_add_fact(prog, pred, subj, obj);
            loaded++;
        }

        while (*line && *line != '\n') line++;
    }

    munmap(buf, (size_t)st.st_size + 1);
    return loaded;
}
