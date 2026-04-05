/*
 * Tardygrada -- Ontology Inference Engine
 */

#include "inference.h"
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <fcntl.h>
#include <ctype.h>

/* Case-insensitive substring search */
static int ici_contains(const char *haystack, const char *needle)
{
    if (!haystack || !needle || !needle[0]) return 0;
    int hlen = (int)strlen(haystack);
    int nlen = (int)strlen(needle);
    if (nlen > hlen) return 0;
    for (int i = 0; i <= hlen - nlen; i++) {
        int match = 1;
        for (int j = 0; j < nlen; j++) {
            char h = haystack[i + j];
            char n = needle[j];
            if (h >= 'A' && h <= 'Z') h += 32;
            if (n >= 'A' && n <= 'Z') n += 32;
            if (h != n) { match = 0; break; }
        }
        if (match) return 1;
    }
    return 0;
}

/* ============================================
 * Synthetic Backbone
 *
 * Structural rules the system is born with.
 * These encode basic ontological reasoning,
 * like OWL inference but as simple if-then rules.
 * ============================================ */

void tardy_inference_init(tardy_ruleset_t *rs)
{
    if (!rs) return;
    memset(rs, 0, sizeof(tardy_ruleset_t));
    int n = 0;

    /* Spatial inference */
    /* capitalOf -> located_in */
    strncpy(rs->rules[n].if_pred, "capitalOf", 63);
    strncpy(rs->rules[n].then_pred, "located_in", 63);
    rs->rules[n].swap_so = 0;
    rs->rules[n].confidence = 0.95f;
    n++;

    /* location -> located_in (synonym) */
    strncpy(rs->rules[n].if_pred, "location", 63);
    strncpy(rs->rules[n].then_pred, "located_in", 63);
    rs->rules[n].swap_so = 1; /* X location Y -> Y located_in X */
    rs->rules[n].confidence = 0.90f;
    n++;

    /* locationCreated -> created_in */
    strncpy(rs->rules[n].if_pred, "locationCreated", 63);
    strncpy(rs->rules[n].then_pred, "created_in", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.90f;
    n++;

    /* Creator inference */
    /* creator -> created_by (symmetric) */
    strncpy(rs->rules[n].if_pred, "creator", 63);
    strncpy(rs->rules[n].then_pred, "created_by", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.95f;
    n++;

    /* founder -> founded_by */
    strncpy(rs->rules[n].if_pred, "founder", 63);
    strncpy(rs->rules[n].then_pred, "founded_by", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.95f;
    n++;

    /* inventor -> invented_by */
    strncpy(rs->rules[n].if_pred, "inventor", 63);
    strncpy(rs->rules[n].then_pred, "invented_by", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.95f;
    n++;

    /* discoverer -> discovered_by */
    strncpy(rs->rules[n].if_pred, "discoverer", 63);
    strncpy(rs->rules[n].then_pred, "discovered_by", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.95f;
    n++;

    /* Temporal */
    /* dateCreated -> created_in */
    strncpy(rs->rules[n].if_pred, "dateCreated", 63);
    strncpy(rs->rules[n].then_pred, "created_in", 63);
    rs->rules[n].swap_so = 0;
    rs->rules[n].confidence = 0.85f;
    n++;

    /* Classification */
    /* knownFor -> associated_with */
    strncpy(rs->rules[n].if_pred, "knownFor", 63);
    strncpy(rs->rules[n].then_pred, "associated_with", 63);
    rs->rules[n].swap_so = 0;
    rs->rules[n].confidence = 0.80f;
    n++;

    /* Reverse lookups */
    /* X created_by Y -> Y creator X */
    strncpy(rs->rules[n].if_pred, "created_by", 63);
    strncpy(rs->rules[n].then_pred, "creator", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.95f;
    n++;

    /* X located_in Y -> Y contains X */
    strncpy(rs->rules[n].if_pred, "located_in", 63);
    strncpy(rs->rules[n].then_pred, "contains", 63);
    rs->rules[n].swap_so = 1;
    rs->rules[n].confidence = 0.90f;
    n++;

    rs->count = n;
}

/* ============================================
 * Self-Healing: Infer Missing Triples
 *
 * Given a query triple that wasn't found in the ontology,
 * try to derive it from existing triples + rules.
 * ============================================ */

int tardy_inference_heal(tardy_ruleset_t *rs,
                          tardy_self_ontology_t *ont,
                          const tardy_triple_t *query, int query_count,
                          tardy_triple_t *inferred, int max_inferred)
{
    if (!rs || !ont || !ont->initialized) return 0;

    tardy_agent_t *ont_agent = tardy_vm_find(ont->vm, ont->ontology_agent);
    if (!ont_agent) return 0;

    int inf_count = 0;

    for (int q = 0; q < query_count && inf_count < max_inferred; q++) {
        /* For each ungrounded query triple, try each rule */
        for (int r = 0; r < rs->count; r++) {
            /* Does any existing ontology triple match the rule's condition
             * and produce a triple that matches our query? */
            for (int c = 0; c < ont_agent->context.child_count; c++) {
                const char *child = ont_agent->context.children[c].name;

                /* Check if this ontology triple has the rule's if_pred */
                if (!ici_contains(child, rs->rules[r].if_pred))
                    continue;

                /* Check if the ontology triple shares subject or object
                 * with our query */
                int shares_subject = ici_contains(child, query[q].subject);
                int shares_object = ici_contains(child, query[q].object);

                if (shares_subject || shares_object) {
                    /* Can infer: the query triple is derivable */
                    if (rs->rules[r].swap_so) {
                        strncpy(inferred[inf_count].subject,
                                query[q].object, TARDY_MAX_TRIPLE_LEN - 1);
                        strncpy(inferred[inf_count].object,
                                query[q].subject, TARDY_MAX_TRIPLE_LEN - 1);
                    } else {
                        strncpy(inferred[inf_count].subject,
                                query[q].subject, TARDY_MAX_TRIPLE_LEN - 1);
                        strncpy(inferred[inf_count].object,
                                query[q].object, TARDY_MAX_TRIPLE_LEN - 1);
                    }
                    strncpy(inferred[inf_count].predicate,
                            rs->rules[r].then_pred, TARDY_MAX_TRIPLE_LEN - 1);
                    inf_count++;
                    break; /* one inference per query triple */
                }
            }
            if (inf_count > 0) break; /* found one, move to next query */
        }
    }

    return inf_count;
}

/* ============================================
 * Rule Mining: Learn from Verification
 *
 * When a claim is verified, extract the pattern
 * and add it as a new rule if it's novel.
 * "predicate X tends to co-occur with predicate Y"
 * ============================================ */

int tardy_inference_learn(tardy_ruleset_t *rs,
                           const tardy_triple_t *triples, int count)
{
    if (!rs || count < 2 || rs->count >= TARDY_MAX_RULES)
        return 0;

    int learned = 0;

    /* Look for predicate pairs that share a subject */
    for (int i = 0; i < count; i++) {
        for (int j = i + 1; j < count; j++) {
            if (strcmp(triples[i].subject, triples[j].subject) != 0)
                continue;

            /* Same subject, different predicates -> potential rule */
            if (strcmp(triples[i].predicate, triples[j].predicate) == 0)
                continue;

            /* Check if this rule already exists */
            int exists = 0;
            for (int r = 0; r < rs->count; r++) {
                if (strcmp(rs->rules[r].if_pred, triples[i].predicate) == 0 &&
                    strcmp(rs->rules[r].then_pred, triples[j].predicate) == 0) {
                    /* Rule exists, boost confidence */
                    rs->rules[r].confidence *= 1.05f;
                    if (rs->rules[r].confidence > 1.0f)
                        rs->rules[r].confidence = 1.0f;
                    exists = 1;
                    break;
                }
            }

            if (!exists && rs->count < TARDY_MAX_RULES) {
                tardy_rule_t *nr = &rs->rules[rs->count];
                strncpy(nr->if_pred, triples[i].predicate, 63);
                strncpy(nr->then_pred, triples[j].predicate, 63);
                nr->swap_so = 0;
                nr->confidence = 0.60f; /* new rules start low */
                rs->count++;
                learned++;
            }
        }
    }

    return learned;
}

/* ============================================
 * Computational Verification
 *
 * If a claim contains numbers and math operations,
 * try to verify by actually running the computation.
 * "The speed of light is 299792458 m/s" -> check known constant
 * "5 + 3 = 8" -> compute and compare
 * ============================================ */

/* Known constants for quick verification */
static struct {
    const char *name;
    const char *value;
} known_constants[] = {
    {"speed of light", "299792458"},
    {"pi", "3.14159"},
    {"euler", "2.71828"},
    {"avogadro", "6.022e23"},
    {"planck", "6.626e-34"},
    {"gravitational constant", "6.674e-11"},
    {"boltzmann", "1.381e-23"},
    {"absolute zero", "-273.15"},
    {"boiling point of water", "100"},
    {"freezing point of water", "0"},
    {NULL, NULL}
};

int tardy_inference_compute(const char *claim, int len,
                             float *confidence)
{
    if (!claim || len <= 0 || !confidence) return -1;
    *confidence = 0.0f;

    /* Check against known constants */
    for (int i = 0; known_constants[i].name; i++) {
        if (ici_contains(claim, known_constants[i].name) &&
            ici_contains(claim, known_constants[i].value)) {
            *confidence = 0.99f;
            return 1;
        }
    }

    /* Try to find "X = Y" pattern and verify with shell */
    const char *eq = strstr(claim, " = ");
    if (!eq) eq = strstr(claim, " is ");
    if (!eq) return -1; /* not a computational claim */

    /* Extract the number after = */
    const char *num_start = eq + 3;
    while (*num_start == ' ') num_start++;

    /* Check if there's actually a number */
    if (!isdigit((unsigned char)*num_start) && *num_start != '-')
        return -1;

    /* Extract the expression before = */
    char expr[256];
    int elen = (int)(eq - claim);
    if (elen > 255) elen = 255;

    /* Find the mathematical part (numbers and operators) */
    int has_math = 0;
    for (int i = 0; i < elen; i++) {
        if (claim[i] == '+' || claim[i] == '-' || claim[i] == '*' ||
            claim[i] == '/' || claim[i] == '^')
            has_math = 1;
    }

    if (!has_math) return -1; /* not a computation */

    /* Build a python expression to verify */
    memcpy(expr, claim, elen);
    expr[elen] = '\0';

    /* Strip non-math characters */
    char clean[256];
    int ci = 0;
    for (int i = 0; i < elen; i++) {
        char c = expr[i];
        if (isdigit((unsigned char)c) || c == '+' || c == '-' ||
            c == '*' || c == '/' || c == '.' || c == ' ' ||
            c == '(' || c == ')' || c == '^')
            clean[ci++] = (c == '^') ? '*' : c; /* ^ -> ** in Python */
    }
    clean[ci] = '\0';

    if (ci < 3) return -1;

    /* Run python to compute */
    char cmd[512];
    snprintf(cmd, sizeof(cmd),
             "python3 -c 'print(eval(\"%s\"))' 2>/dev/null", clean);

    int pipefd[2];
    if (pipe(pipefd) < 0) return -1;

    pid_t pid = fork();
    if (pid < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        return -1;
    }

    if (pid == 0) {
        close(pipefd[0]);
        dup2(pipefd[1], STDOUT_FILENO);
        close(pipefd[1]);
        int devnull = open("/dev/null", O_WRONLY);
        if (devnull >= 0) { dup2(devnull, STDERR_FILENO); close(devnull); }
        execl("/bin/sh", "sh", "-c", cmd, (char *)NULL);
        _exit(127);
    }

    close(pipefd[1]);
    char result[64];
    int rlen = 0;
    ssize_t n;
    while ((n = read(pipefd[0], result + rlen, sizeof(result) - (size_t)rlen - 1)) > 0)
        rlen += (int)n;
    result[rlen] = '\0';
    close(pipefd[0]);

    int wstatus;
    waitpid(pid, &wstatus, 0);

    if (!WIFEXITED(wstatus) || WEXITSTATUS(wstatus) != 0 || rlen == 0)
        return -1;

    /* Trim whitespace */
    while (rlen > 0 && (result[rlen - 1] == '\n' || result[rlen - 1] == ' '))
        result[--rlen] = '\0';

    /* Compare computed result to claimed result */
    if (ici_contains(num_start, result) || ici_contains(result, num_start)) {
        *confidence = 0.99f;
        return 1;
    }

    /* Try numeric comparison (handles floating point) */
    double computed = atof(result);
    double claimed = atof(num_start);
    if (computed != 0.0 && claimed != 0.0) {
        double diff = (computed - claimed) / computed;
        if (diff < 0) diff = -diff;
        if (diff < 0.001) { /* within 0.1% */
            *confidence = 0.95f;
            return 1;
        }
    }

    *confidence = 0.0f;
    return 0; /* computation doesn't match */
}
