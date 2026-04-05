/*
 * Tardygrada VM — Formal Semantics as Thresholds
 * Every guarantee is a number. Every number is tunable.
 * Defaults are safe. ARIA research refines over time.
 */

#ifndef TARDY_SEMANTICS_H
#define TARDY_SEMANTICS_H

#include <stdbool.h>

typedef struct {
    int   min_evidence_triples;        /* min ontology triples to call something Fact */
    int   max_contradictions;          /* max contradictions allowed (0 = absolute) */
    float min_confidence;              /* min probabilistic confidence */
    int   min_consensus_agents;        /* min agents that must independently agree */
    float min_agreement_ratio;         /* min agreement ratio (0.67 = 2/3 BFT) */
} tardy_truth_semantics_t;

typedef struct {
    float grounding_threshold;         /* below this = hallucinated */
    int   min_decomposers;             /* min independent decomposition agents */
    float min_decomposition_agreement; /* min triple overlap between decomposers */
    bool  require_dual_ontology;       /* must pass both sketch and complete */
} tardy_hallucination_semantics_t;

typedef struct {
    int   min_observed_operations;     /* min VM-observed operations */
    float min_work_authenticity;       /* claimed vs observed work ratio */
    int   max_idle_ms;                 /* max idle time during task */
    int   min_impossibility_verifiers; /* min verifiers for impossibility proof */
    float max_work_similarity;         /* max similarity before flagged as copy */
    int   max_verification_chain;      /* max depth before circular flag */
} tardy_laziness_semantics_t;

typedef struct {
    int   hardened_replica_count;      /* replicas for @hardened */
    int   sovereign_replica_count;     /* replicas for @sovereign */
    float sovereign_quorum_ratio;      /* quorum for @sovereign BFT */
} tardy_immutability_semantics_t;

typedef struct {
    int demotion_idle_ms;              /* idle time before Live -> Static */
    int temp_ttl_ms;                   /* Temp agent TTL before re-demotion */
    int sovereign_dump_idle_ms;        /* idle time before sovereign dumps to disk */
    int gc_interval_ms;                /* GC scan interval */
} tardy_lifecycle_semantics_t;

typedef struct {
    bool layer_ontology_grounding;     /* layer 1-2 */
    bool layer_consistency_check;      /* layer 3 */
    bool layer_probabilistic_scoring;  /* layer 4 */
    bool layer_protocol_check;         /* layer 5 */
    bool layer_formal_certification;   /* layer 6 */
    bool layer_cross_representation;   /* layer 7 */
    bool layer_work_verification;      /* layer 8 */
    int  min_passing_layers;           /* minimum layers that must pass */
    bool skip_for_literals;            /* skip pipeline for literal assignments */
    bool skip_for_arithmetic;          /* skip pipeline for pure math */
    bool skip_for_internal_routing;    /* skip for internal agent messages */
} tardy_pipeline_semantics_t;

/* The full semantics — one struct rules everything */
typedef struct {
    tardy_truth_semantics_t          truth;
    tardy_hallucination_semantics_t  hallucination;
    tardy_laziness_semantics_t       laziness;
    tardy_immutability_semantics_t   immutability;
    tardy_lifecycle_semantics_t      lifecycle;
    tardy_pipeline_semantics_t       pipeline;
} tardy_semantics_t;

/* Safe defaults */
static const tardy_semantics_t TARDY_DEFAULT_SEMANTICS = {
    .truth = {
        .min_evidence_triples        = 1,
        .max_contradictions          = 0,
        .min_confidence              = 0.85f,
        .min_consensus_agents        = 3,
        .min_agreement_ratio         = 0.67f,
    },
    .hallucination = {
        .grounding_threshold         = 0.0f,
        .min_decomposers             = 3,
        .min_decomposition_agreement = 0.5f,
        .require_dual_ontology       = true,
    },
    .laziness = {
        .min_observed_operations     = 1,
        .min_work_authenticity       = 0.9f,
        .max_idle_ms                 = 5000,
        .min_impossibility_verifiers = 2,
        .max_work_similarity         = 0.95f,
        .max_verification_chain      = 3,
    },
    .immutability = {
        .hardened_replica_count      = 3,
        .sovereign_replica_count     = 5,
        .sovereign_quorum_ratio      = 0.67f,
    },
    .lifecycle = {
        .demotion_idle_ms            = 30000,
        .temp_ttl_ms                 = 60000,
        .sovereign_dump_idle_ms      = 300000,
        .gc_interval_ms              = 1000,
    },
    .pipeline = {
        .layer_ontology_grounding    = true,
        .layer_consistency_check     = true,
        .layer_probabilistic_scoring = true,
        .layer_protocol_check        = true,
        .layer_formal_certification  = false,  /* expensive, opt-in */
        .layer_cross_representation  = false,  /* bleeding edge, opt-in */
        .layer_work_verification     = true,
        .min_passing_layers          = 5,
        .skip_for_literals           = true,
        .skip_for_arithmetic         = true,
        .skip_for_internal_routing   = true,
    },
};

#endif /* TARDY_SEMANTICS_H */
