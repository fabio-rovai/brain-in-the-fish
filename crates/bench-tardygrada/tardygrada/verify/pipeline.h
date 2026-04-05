/*
 * Tardygrada — Verification Pipeline
 *
 * 8 layers. Every LLM-produced Fact goes through all of them.
 * Fail fast: one layer fails, stop, report which one.
 * Skip for literals, arithmetic, internal routing.
 *
 * Layer 1: Decompose — text to triples (multiple independent agents)
 * Layer 2: Ontology grounding — triples vs knowledge graph
 * Layer 3: Consistency check — OWL reasoner for contradictions
 * Layer 4: Probabilistic scoring — quantitative confidence
 * Layer 5: Protocol check — session types compliance
 * Layer 6: Formal certification — proof-certificate asymmetry
 * Layer 7: Cross-representation bridge — all layers agree
 * Layer 8: VM work verification — laziness detection
 */

#ifndef TARDY_PIPELINE_H
#define TARDY_PIPELINE_H

#include "../vm/types.h"
#include "../vm/semantics.h"
#include "../vm/crypto.h"

/* ============================================
 * Triple — atomic claim extracted from LLM output
 * (subject, predicate, object) in RDF-like form
 * ============================================ */

#define TARDY_MAX_TRIPLES     64
#define TARDY_MAX_TRIPLE_LEN  256

typedef struct {
    char subject[TARDY_MAX_TRIPLE_LEN];
    char predicate[TARDY_MAX_TRIPLE_LEN];
    char object[TARDY_MAX_TRIPLE_LEN];
} tardy_triple_t;

/* ============================================
 * Decomposition Result — text broken into triples
 * Multiple agents decompose independently, results merged
 * ============================================ */

typedef struct {
    tardy_triple_t triples[TARDY_MAX_TRIPLES];
    int            count;
    tardy_uuid_t   decomposer;   /* which agent did this */
    float          agreement;    /* overlap with other decomposers */
} tardy_decomposition_t;

/* ============================================
 * Grounding Result — each triple checked against ontology
 * ============================================ */

typedef struct {
    tardy_triple_t           triple;
    tardy_knowledge_status_t status;     /* grounded / unknown / contradicted */
    int                      evidence_count; /* how many supporting triples found */
    float                    confidence;
} tardy_grounding_result_t;

typedef struct {
    tardy_grounding_result_t results[TARDY_MAX_TRIPLES];
    int                      count;
    int                      grounded;     /* count of GROUNDED */
    int                      consistent;   /* count of CONSISTENT (frame-valid) */
    int                      unknown;      /* count of UNKNOWN */
    int                      contradicted; /* count of CONTRADICTED */
} tardy_grounding_t;

/* ============================================
 * Consistency Result — OWL reasoner output
 * ============================================ */

typedef struct {
    bool consistent;
    int  contradiction_count;
    char explanation[512];
} tardy_consistency_t;

/* ============================================
 * Layer Result — generic result per layer
 * ============================================ */

typedef struct {
    tardy_layer_t layer;
    bool          passed;
    float         confidence;     /* 0.0 to 1.0 */
    char          detail[256];    /* human-readable explanation */
    uint64_t      compute_ns;    /* how long this layer took */
} tardy_layer_result_t;

/* ============================================
 * Work Log — VM-observed operations (not self-reported)
 * The dashcam. Agent can't fake this.
 * ============================================ */

typedef struct {
    int      ontology_queries;   /* how many ontology lookups observed */
    int      context_reads;      /* how many context pointer dereferences */
    int      agents_spawned;     /* how many sub-agents created */
    uint64_t compute_ns;         /* actual CPU time consumed */
    size_t   memory_used;        /* actual memory allocated */
    tardy_hash_t operations_hash; /* hash of all operations for tamper detection */
} tardy_work_log_t;

/* ============================================
 * Work Spec — minimum work required (computed by VM, not agent)
 * Deterministic C. Not an LLM. Can't be gamed.
 * ============================================ */

typedef struct {
    int      min_ontology_queries;
    int      min_context_reads;
    int      min_agents;
    uint64_t min_compute_ns;
    int      min_evidence_triples;
} tardy_work_spec_t;

/* ============================================
 * Laziness Verdict
 * ============================================ */

typedef struct {
    tardy_laziness_t type;
    char             detail[256];
    int              claimed_operations;
    int              observed_operations;
} tardy_laziness_verdict_t;

/* ============================================
 * Failure Intelligence — structured error types
 * ============================================ */

typedef enum {
    TARDY_FAIL_NONE = 0,
    TARDY_FAIL_DECOMPOSITION,     /* text couldn't be broken into meaningful triples */
    TARDY_FAIL_ONTOLOGY_GAP,      /* ontology has no data (UNKNOWN, not contradicted) */
    TARDY_FAIL_CONTRADICTION,     /* ontology contradicts the claim */
    TARDY_FAIL_LOW_CONFIDENCE,    /* evidence exists but below threshold */
    TARDY_FAIL_INCONSISTENCY,     /* triples contradict each other */
    TARDY_FAIL_NO_EVIDENCE,       /* grounding found zero supporting triples */
    TARDY_FAIL_PROTOCOL,          /* claim structure invalid */
    TARDY_FAIL_LAZINESS,          /* agent didn't do enough work */
    TARDY_FAIL_AMBIGUITY,         /* multiple interpretations, can't resolve */
    TARDY_FAIL_CROSS_REP,         /* layers disagree with each other */
} tardy_failure_type_t;

/* ============================================
 * Pipeline Result — the full output
 * ============================================ */

typedef struct {
    bool                    passed;         /* all required layers passed */
    float                   confidence;     /* minimum across all layers */
    int                     layers_passed;
    int                     layers_failed;
    tardy_layer_t           failed_at;      /* which layer failed first */
    tardy_layer_result_t    layers[TARDY_LAYER_COUNT];
    tardy_truth_strength_t  strength;       /* computed truth strength */
    tardy_laziness_verdict_t laziness;      /* work verification result */
    tardy_failure_type_t    failure_type;    /* structured failure reason */
    char                    failure_detail[256]; /* human-readable failure detail */
} tardy_pipeline_result_t;

/* ============================================
 * Pipeline API
 * ============================================ */

/* Initialize a work log for monitoring an agent's task */
void tardy_worklog_init(tardy_work_log_t *log);

/* Record operations in the work log (called by VM, not agent) */
void tardy_worklog_record_query(tardy_work_log_t *log);
void tardy_worklog_record_read(tardy_work_log_t *log);
void tardy_worklog_record_spawn(tardy_work_log_t *log);
void tardy_worklog_record_compute(tardy_work_log_t *log, uint64_t ns);
void tardy_worklog_record_memory(tardy_work_log_t *log, size_t bytes);

/* Compute minimum work spec for a task (deterministic, not LLM) */
tardy_work_spec_t tardy_compute_work_spec(const tardy_semantics_t *sem);

/* Run the full verification pipeline on a claim */
tardy_pipeline_result_t tardy_pipeline_verify(
    const char *claim,
    int claim_len,
    const tardy_decomposition_t *decompositions,
    int decomposition_count,
    const tardy_grounding_t *grounding,
    const tardy_consistency_t *consistency,
    const tardy_work_log_t *work_log,
    const tardy_work_spec_t *work_spec,
    const tardy_semantics_t *semantics
);

/* Individual layer checks */
tardy_layer_result_t tardy_verify_decomposition(
    const tardy_decomposition_t *decomps, int count,
    const tardy_semantics_t *sem);

tardy_layer_result_t tardy_verify_grounding(
    const tardy_grounding_t *grounding,
    const tardy_semantics_t *sem);

tardy_layer_result_t tardy_verify_consistency(
    const tardy_consistency_t *consistency,
    const tardy_semantics_t *sem);

tardy_layer_result_t tardy_verify_probabilistic(
    const tardy_grounding_t *grounding,
    const tardy_semantics_t *sem);

tardy_layer_result_t tardy_verify_protocol(const char *claim, int claim_len);

tardy_layer_result_t tardy_verify_certification(
    const tardy_decomposition_t *decomps, int decomp_count);

tardy_layer_result_t tardy_verify_cross_representation(
    const tardy_layer_result_t *layers, int count);

tardy_layer_result_t tardy_verify_work(
    const tardy_work_log_t *log,
    const tardy_work_spec_t *spec,
    const tardy_semantics_t *sem);

/* Compute truth strength from pipeline results */
tardy_truth_strength_t tardy_compute_truth_strength(
    const tardy_pipeline_result_t *result);

#endif /* TARDY_PIPELINE_H */
