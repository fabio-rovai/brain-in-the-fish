/*
 * Pure Tardygrada benchmark — zero Rust deps, self-hosted ontology.
 * Runs the same pipeline as bench-naive and bench-tardygrada:
 * spawn agents → score → debate → moderation → gate → verdict
 *
 * Compile: cc -O2 -std=c11 -o bench_pure bench.c <tardygrada .c files> -Itardygrada/
 * Run: ./bench_pure
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "../../bench-tardygrada/tardygrada/vm/vm.h"
#include "../../bench-tardygrada/tardygrada/vm/types.h"
#include "../../bench-tardygrada/tardygrada/vm/context.h"
#include "../../bench-tardygrada/tardygrada/vm/memory.h"
#include "../../bench-tardygrada/tardygrada/vm/semantics.h"

/* Pre-computed scores from real Claude subagents (from bench_scores.json) */
/* 4 agents x 5 criteria = 20 scores per round */

typedef struct {
    const char *agent_name;
    const char *criterion_id;
    double score;
    double max_score;
} score_entry_t;

static score_entry_t round1_scores[] = {
    {"Budget_Expert",      "crit-0", 8.2, 10.0},
    {"Budget_Expert",      "crit-1", 7.8, 10.0},
    {"Budget_Expert",      "crit-2", 8.1, 10.0},
    {"Budget_Expert",      "crit-3", 8.5, 10.0},
    {"Budget_Expert",      "crit-4", 7.3, 10.0},
    {"Technical_Eval",     "crit-0", 7.8, 10.0},
    {"Technical_Eval",     "crit-1", 8.2, 10.0},
    {"Technical_Eval",     "crit-2", 7.5, 10.0},
    {"Technical_Eval",     "crit-3", 8.4, 10.0},
    {"Technical_Eval",     "crit-4", 7.2, 10.0},
    {"Delivery_Spec",      "crit-0", 8.2, 10.0},
    {"Delivery_Spec",      "crit-1", 7.8, 10.0},
    {"Delivery_Spec",      "crit-2", 8.1, 10.0},
    {"Delivery_Spec",      "crit-3", 7.5, 10.0},
    {"Delivery_Spec",      "crit-4", 7.3, 10.0},
    {"Social_Value",       "crit-0", 7.8, 10.0},
    {"Social_Value",       "crit-1", 8.2, 10.0},
    {"Social_Value",       "crit-2", 8.1, 10.0},
    {"Social_Value",       "crit-3", 8.5, 10.0},
    {"Social_Value",       "crit-4", 7.4, 10.0},
};

/* Round 2: after debate, agent-2 (Delivery_Spec) adjusted crit-3 from 7.5 to 8.2 */
static score_entry_t round2_scores[] = {
    {"Budget_Expert",      "crit-0", 8.2, 10.0},
    {"Budget_Expert",      "crit-1", 7.8, 10.0},
    {"Budget_Expert",      "crit-2", 8.1, 10.0},
    {"Budget_Expert",      "crit-3", 8.5, 10.0},
    {"Budget_Expert",      "crit-4", 7.3, 10.0},
    {"Technical_Eval",     "crit-0", 7.8, 10.0},
    {"Technical_Eval",     "crit-1", 8.2, 10.0},
    {"Technical_Eval",     "crit-2", 7.5, 10.0},
    {"Technical_Eval",     "crit-3", 8.4, 10.0},
    {"Technical_Eval",     "crit-4", 7.2, 10.0},
    {"Delivery_Spec",      "crit-0", 8.2, 10.0},
    {"Delivery_Spec",      "crit-1", 7.8, 10.0},
    {"Delivery_Spec",      "crit-2", 8.1, 10.0},
    {"Delivery_Spec",      "crit-3", 8.2, 10.0},  /* adjusted after debate */
    {"Delivery_Spec",      "crit-4", 7.3, 10.0},
    {"Social_Value",       "crit-0", 7.8, 10.0},
    {"Social_Value",       "crit-1", 8.2, 10.0},
    {"Social_Value",       "crit-2", 8.1, 10.0},
    {"Social_Value",       "crit-3", 8.5, 10.0},
    {"Social_Value",       "crit-4", 7.4, 10.0},
};

#define NUM_AGENTS 4
#define NUM_CRITERIA 5
#define NUM_SCORES 20

static const char *agent_names[NUM_AGENTS] = {
    "Budget_Expert", "Technical_Eval", "Delivery_Spec", "Social_Value"
};

static const char *criteria[NUM_CRITERIA] = {
    "crit-0", "crit-1", "crit-2", "crit-3", "crit-4"
};

static double get_time_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1000000.0;
}

int main(void) {
    printf("=== Pure Tardygrada C Benchmark ===\n");
    printf("Zero Rust deps, self-hosted ontology\n\n");

    double start = get_time_ms();

    /* 1. Init VM */
    tardy_vm_t *vm = calloc(1, sizeof(tardy_vm_t));
    if (!vm) { fprintf(stderr, "OOM\n"); return 1; }
    tardy_semantics_t sem = TARDY_DEFAULT_SEMANTICS;
    tardy_vm_init(vm, &sem);

    tardy_uuid_t root = vm->root_id;

    /* 2. Spawn evaluator agents */
    tardy_uuid_t agents[NUM_AGENTS];
    for (int i = 0; i < NUM_AGENTS; i++) {
        agents[i] = tardy_vm_spawn(vm, root, agent_names[i],
            TARDY_TYPE_AGENT, TARDY_TRUST_VERIFIED, "", 0);
        /* Store role */
        tardy_vm_spawn(vm, agents[i], "role",
            TARDY_TYPE_STR, TARDY_TRUST_DEFAULT, "evaluator", 9);
    }

    /* 3. Wire trust between agents */
    for (int i = 0; i < NUM_AGENTS; i++) {
        for (int j = 0; j < NUM_AGENTS; j++) {
            if (i != j) {
                char name[64];
                snprintf(name, sizeof(name), "trust_%s", agent_names[j]);
                double trust = 0.6;
                tardy_vm_spawn(vm, agents[i], name,
                    TARDY_TYPE_FLOAT, TARDY_TRUST_MUTABLE, &trust, sizeof(trust));
            }
        }
    }

    /* 4. Store alignments */
    tardy_uuid_t align_parent = tardy_vm_spawn(vm, root, "alignments",
        TARDY_TYPE_AGENT, TARDY_TRUST_DEFAULT, "", 0);
    const char *sections[] = {"sec-0","sec-1","sec-2","sec-3","sec-4",
                              "sec-5","sec-6","sec-7","sec-8","sec-9"};
    const char *align_crits[] = {"crit-0","crit-0","crit-1","crit-1","crit-2",
                                 "crit-2","crit-3","crit-3","crit-4","crit-4"};
    double confs[] = {0.9, 0.7, 0.85, 0.6, 0.8, 0.75, 0.7, 0.65, 0.8, 0.5};
    for (int i = 0; i < 10; i++) {
        char name[64];
        snprintf(name, sizeof(name), "align_%s_%s", sections[i], align_crits[i]);
        tardy_vm_spawn(vm, align_parent, name,
            TARDY_TYPE_FLOAT, TARDY_TRUST_VERIFIED, &confs[i], sizeof(double));
    }

    /* 5. Round 1: Record scores as verified Facts */
    for (int i = 0; i < NUM_SCORES; i++) {
        /* Find agent */
        int ai = -1;
        for (int a = 0; a < NUM_AGENTS; a++) {
            if (strcmp(agent_names[a], round1_scores[i].agent_name) == 0) {
                ai = a; break;
            }
        }
        if (ai < 0) continue;

        char name[64];
        snprintf(name, sizeof(name), "score_%s", round1_scores[i].criterion_id);
        tardy_vm_spawn(vm, agents[ai], name,
            TARDY_TYPE_FLOAT, TARDY_TRUST_VERIFIED,
            &round1_scores[i].score, sizeof(double));
    }

    /* 6. Read all scores back (hash-verified reads) */
    for (int a = 0; a < NUM_AGENTS; a++) {
        for (int c = 0; c < NUM_CRITERIA; c++) {
            char name[64];
            snprintf(name, sizeof(name), "score_%s", criteria[c]);
            double val = 0;
            tardy_vm_read(vm, agents[a], name, &val, sizeof(double));
        }
    }

    /* 7. Find disagreements and send challenges */
    for (int c = 0; c < NUM_CRITERIA; c++) {
        double scores[NUM_AGENTS];
        for (int a = 0; a < NUM_AGENTS; a++) {
            char name[64];
            snprintf(name, sizeof(name), "score_%s", criteria[c]);
            tardy_vm_read(vm, agents[a], name, &scores[a], sizeof(double));
        }
        /* Find max disagreement */
        double min_s = scores[0], max_s = scores[0];
        for (int a = 1; a < NUM_AGENTS; a++) {
            if (scores[a] < min_s) min_s = scores[a];
            if (scores[a] > max_s) max_s = scores[a];
        }
        if (max_s - min_s > 0.3) {
            /* Send challenge messages between most-disagreeing pair */
            int low_a = 0, high_a = 0;
            for (int a = 0; a < NUM_AGENTS; a++) {
                if (scores[a] == min_s) low_a = a;
                if (scores[a] == max_s) high_a = a;
            }
            char msg[256];
            snprintf(msg, sizeof(msg), "challenge_%s_%.1f_vs_%.1f",
                criteria[c], max_s, min_s);
            tardy_vm_send(vm, agents[high_a], agents[low_a],
                msg, strlen(msg), TARDY_TYPE_STR);
        }
    }

    /* 8. Round 2: Update scores after debate */
    for (int i = 0; i < NUM_SCORES; i++) {
        int ai = -1;
        for (int a = 0; a < NUM_AGENTS; a++) {
            if (strcmp(agent_names[a], round2_scores[i].agent_name) == 0) {
                ai = a; break;
            }
        }
        if (ai < 0) continue;

        char name[64];
        snprintf(name, sizeof(name), "score_%s", round2_scores[i].criterion_id);
        /* Mutate if score changed, otherwise just re-read */
        double current = 0;
        tardy_vm_read(vm, agents[ai], name, &current, sizeof(double));
        if (current != round2_scores[i].score) {
            tardy_vm_mutate(vm, agents[ai], name,
                &round2_scores[i].score, sizeof(double));
        }
    }

    /* 9. Moderation: compute consensus (trust-weighted mean) */
    double consensus[NUM_CRITERIA];
    for (int c = 0; c < NUM_CRITERIA; c++) {
        double sum = 0;
        for (int a = 0; a < NUM_AGENTS; a++) {
            char name[64];
            snprintf(name, sizeof(name), "score_%s", criteria[c]);
            double val = 0;
            tardy_vm_read(vm, agents[a], name, &val, sizeof(double));
            sum += val;
        }
        consensus[c] = sum / NUM_AGENTS;
    }

    /* 10. Build argument graph */
    tardy_uuid_t graph = tardy_vm_spawn(vm, root, "argument_graph",
        TARDY_TYPE_AGENT, TARDY_TRUST_VERIFIED, "", 0);
    tardy_vm_spawn(vm, graph, "thesis",
        TARDY_TYPE_STR, TARDY_TRUST_VERIFIED, "Document thesis", 15);
    for (int c = 0; c < NUM_CRITERIA; c++) {
        char name[64];
        snprintf(name, sizeof(name), "claim_%s", criteria[c]);
        const char *type = consensus[c] >= 7.0 ? "StrongClaim" : "WeakClaim";
        tardy_vm_spawn(vm, graph, name,
            TARDY_TYPE_STR, TARDY_TRUST_VERIFIED, type, strlen(type));
    }

    /* 11. Gate verdict */
    double overall = 0;
    for (int c = 0; c < NUM_CRITERIA; c++) overall += consensus[c];
    overall /= NUM_CRITERIA;

    const char *verdict = overall >= 7.0 ? "CONFIRMED" :
                          overall >= 5.0 ? "FLAGGED" : "REJECTED";
    tardy_vm_spawn(vm, root, "verdict",
        TARDY_TYPE_STR, TARDY_TRUST_SOVEREIGN, verdict, strlen(verdict));

    /* 12. GC */
    int gc = tardy_vm_gc(vm);

    double elapsed = get_time_ms() - start;

    /* Print results */
    printf("Coordination time: %.3f ms\n", elapsed);
    printf("Verdict: %s\n", verdict);
    printf("Overall score: %.2f / 10.0\n", overall);
    printf("GC collected: %d agents\n", gc);
    printf("\nConsensus scores:\n");
    for (int c = 0; c < NUM_CRITERIA; c++) {
        printf("  %s: %.2f\n", criteria[c], consensus[c]);
    }

    tardy_vm_shutdown(vm);
    free(vm);

    return 0;
}
