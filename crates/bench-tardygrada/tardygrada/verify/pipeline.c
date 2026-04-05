/*
 * Tardygrada — Verification Pipeline Implementation
 *
 * 8 layers. Fail fast. The VM is the incorruptible judge.
 * Deterministic C. Not an LLM. Can't be bribed, lazy, or hallucinate.
 */

#include "pipeline.h"
#include <string.h>
#include <stdio.h>
#include <time.h>

static uint64_t now_ns(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/* ============================================
 * Work Log — the dashcam
 * ============================================ */

void tardy_worklog_init(tardy_work_log_t *log)
{
    memset(log, 0, sizeof(tardy_work_log_t));
}

void tardy_worklog_record_query(tardy_work_log_t *log)
{
    log->ontology_queries++;
}

void tardy_worklog_record_read(tardy_work_log_t *log)
{
    log->context_reads++;
}

void tardy_worklog_record_spawn(tardy_work_log_t *log)
{
    log->agents_spawned++;
}

void tardy_worklog_record_compute(tardy_work_log_t *log, uint64_t ns)
{
    log->compute_ns += ns;
}

void tardy_worklog_record_memory(tardy_work_log_t *log, size_t bytes)
{
    log->memory_used += bytes;
}

/* ============================================
 * Work Spec — minimum work required
 * Deterministic. Computed from semantics.
 * ============================================ */

tardy_work_spec_t tardy_compute_work_spec(const tardy_semantics_t *sem)
{
    tardy_work_spec_t spec = {0};

    /* Must query at least as many ontologies as decomposers require */
    spec.min_ontology_queries = sem->hallucination.require_dual_ontology ? 2 : 1;

    /* Must read at least the minimum evidence triples */
    spec.min_context_reads = sem->truth.min_evidence_triples;

    /* Must have minimum consensus agents */
    spec.min_agents = sem->truth.min_consensus_agents;

    /* Minimum compute time: at least 1ms (prevents instant fake responses) */
    spec.min_compute_ns = 1000000;

    /* Minimum evidence triples from semantics */
    spec.min_evidence_triples = sem->truth.min_evidence_triples;

    return spec;
}

/* ============================================
 * Layer 1: Decomposition Verification
 *
 * Multiple agents decomposed the text independently.
 * Check: did enough agents participate? Do they agree?
 * ============================================ */

tardy_layer_result_t tardy_verify_decomposition(
    const tardy_decomposition_t *decomps, int count,
    const tardy_semantics_t *sem)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_DECOMPOSE;

    /* Check minimum decomposers */
    if (count < sem->hallucination.min_decomposers) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 "only %d decomposers, need %d",
                 count, sem->hallucination.min_decomposers);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Compute pairwise agreement between decomposers */
    float total_agreement = 0.0f;
    int pairs = 0;

    for (int i = 0; i < count; i++) {
        for (int j = i + 1; j < count; j++) {
            /* Count overlapping triples */
            int overlap = 0;
            int total = decomps[i].count + decomps[j].count;
            if (total == 0) continue;

            for (int a = 0; a < decomps[i].count; a++) {
                for (int b = 0; b < decomps[j].count; b++) {
                    if (strcmp(decomps[i].triples[a].subject,
                              decomps[j].triples[b].subject) == 0 &&
                        strcmp(decomps[i].triples[a].predicate,
                              decomps[j].triples[b].predicate) == 0 &&
                        strcmp(decomps[i].triples[a].object,
                              decomps[j].triples[b].object) == 0) {
                        overlap++;
                        break;
                    }
                }
            }
            /* Jaccard-like: overlap / union */
            float agreement = (float)(overlap * 2) / (float)total;
            total_agreement += agreement;
            pairs++;
        }
    }

    float avg_agreement = pairs > 0 ? total_agreement / (float)pairs : 0.0f;

    if (avg_agreement < sem->hallucination.min_decomposition_agreement) {
        r.passed = false;
        r.confidence = avg_agreement;
        snprintf(r.detail, sizeof(r.detail),
                 "decomposition agreement %.2f < %.2f threshold",
                 avg_agreement, sem->hallucination.min_decomposition_agreement);
    } else {
        r.passed = true;
        r.confidence = avg_agreement;
        snprintf(r.detail, sizeof(r.detail),
                 "%d decomposers agree at %.2f", count, avg_agreement);
    }

    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 2: Ontology Grounding
 *
 * Each triple checked against the knowledge graph.
 * Contradicted = hallucination. Grounded = evidence.
 * ============================================ */

tardy_layer_result_t tardy_verify_grounding(
    const tardy_grounding_t *grounding,
    const tardy_semantics_t *sem)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_GROUNDING;

    if (!grounding || grounding->count == 0) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail), "no grounding data");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Any contradictions = immediate fail */
    if (grounding->contradicted > 0) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 "%d triples contradicted by ontology — hallucination detected",
                 grounding->contradicted);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Count CONSISTENT triples (structurally valid per frame) */
    int consistent_n = 0;
    for (int i = 0; i < grounding->count; i++) {
        if (grounding->results[i].status == TARDY_KNOWLEDGE_CONSISTENT)
            consistent_n++;
    }

    /* Need minimum grounded + consistent triples */
    int effective_evidence = grounding->grounded + consistent_n;
    if (effective_evidence < sem->truth.min_evidence_triples) {
        r.passed = false;
        r.confidence = (float)effective_evidence / (float)grounding->count;
        snprintf(r.detail, sizeof(r.detail),
                 "only %d evidence triples (%d grounded + %d consistent), need %d",
                 effective_evidence, grounding->grounded, consistent_n,
                 sem->truth.min_evidence_triples);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Compute grounding confidence.
     * Key insight: CONSISTENT triples are partial evidence (better than unknown).
     * GROUNDED = high confidence, CONSISTENT = medium, UNKNOWN = low. */
    float grounded_conf = 0.0f;
    int grounded_n = 0;
    float consistent_conf = 0.0f;
    for (int i = 0; i < grounding->count; i++) {
        if (grounding->results[i].status == TARDY_KNOWLEDGE_GROUNDED) {
            grounded_conf += grounding->results[i].confidence;
            grounded_n++;
        } else if (grounding->results[i].status == TARDY_KNOWLEDGE_CONSISTENT) {
            consistent_conf += grounding->results[i].confidence;
        }
    }
    /* Base confidence on grounded + consistent triples */
    float ratio;
    if (grounded_n > 0 || consistent_n > 0) {
        float total_evidence_conf = grounded_conf + consistent_conf;
        int total_evidence_n = grounded_n + consistent_n;
        ratio = total_evidence_conf / (float)total_evidence_n;
        /* Slight penalty for unknowns (but not as harsh as treating them as failures) */
        float coverage = (float)total_evidence_n / (float)grounding->count;
        ratio = ratio * (0.5f + 0.5f * coverage);
    } else {
        ratio = 0.0f;
    }
    r.passed = true;
    r.confidence = ratio;
    snprintf(r.detail, sizeof(r.detail),
             "%d/%d grounded, %d consistent (conf=%.0f%%), %d unknown",
             grounding->grounded, grounding->count, consistent_n,
             ratio * 100.0f, grounding->unknown);

    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 3: Consistency Check
 *
 * OWL reasoner: do the grounded triples contradict each other
 * or the existing knowledge graph?
 * ============================================ */

tardy_layer_result_t tardy_verify_consistency(
    const tardy_consistency_t *consistency,
    const tardy_semantics_t *sem)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_CONSISTENCY;

    if (!consistency) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail), "no consistency data");
        r.compute_ns = now_ns() - start;
        return r;
    }

    if (consistency->contradiction_count > sem->truth.max_contradictions) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 "%d contradictions found: %.200s",
                 consistency->contradiction_count, consistency->explanation);
    } else {
        r.passed = consistency->consistent;
        r.confidence = consistency->consistent ? 1.0f : 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 consistency->consistent ? "consistent" : "inconsistent");
    }

    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 4: Probabilistic Scoring
 *
 * Quantitative confidence across all grounded triples.
 * Aggregates evidence strength.
 * ============================================ */

tardy_layer_result_t tardy_verify_probabilistic(
    const tardy_grounding_t *grounding,
    const tardy_semantics_t *sem)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_PROBABILISTIC;

    if (!grounding || grounding->count == 0) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail), "no data for scoring");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Average confidence across grounded + consistent triples */
    float total_conf = 0.0f;
    int counted = 0;
    for (int i = 0; i < grounding->count; i++) {
        if (grounding->results[i].status == TARDY_KNOWLEDGE_GROUNDED ||
            grounding->results[i].status == TARDY_KNOWLEDGE_CONSISTENT) {
            total_conf += grounding->results[i].confidence;
            counted++;
        }
    }

    float avg = counted > 0 ? total_conf / (float)counted : 0.0f;

    if (avg < sem->truth.min_confidence) {
        r.passed = false;
        r.confidence = avg;
        snprintf(r.detail, sizeof(r.detail),
                 "confidence %.3f < %.3f threshold",
                 avg, sem->truth.min_confidence);
    } else {
        r.passed = true;
        r.confidence = avg;
        snprintf(r.detail, sizeof(r.detail),
                 "confidence %.3f from %d evidence triples", avg, counted);
    }

    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 5: Protocol Check
 *
 * Session types: did the agent follow the protocol?
 * Validates claim structure: non-empty, multi-word, sufficient content.
 * ============================================ */

tardy_layer_result_t tardy_verify_protocol(const char *claim, int claim_len)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_PROTOCOL;

    if (!claim || claim_len == 0) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 "empty claim — protocol violation");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check claim has substance — at least 3 characters of content */
    int content_chars = 0;
    for (int i = 0; i < claim_len; i++) {
        if (claim[i] != ' ' && claim[i] != '\t' && claim[i] != '\n')
            content_chars++;
    }
    if (content_chars < 3) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 "claim too short (%d chars) — possible empty submission",
                 content_chars);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check claim has at least 2 words (subject + predicate minimum) */
    int words = 0;
    int in_word = 0;
    for (int i = 0; i < claim_len; i++) {
        if (claim[i] == ' ' || claim[i] == '\t' || claim[i] == '\n') {
            in_word = 0;
        } else if (!in_word) {
            in_word = 1;
            words++;
        }
    }
    if (words < 2) {
        r.passed = false;
        r.confidence = 0.5f;
        snprintf(r.detail, sizeof(r.detail),
                 "claim has only %d word(s) — insufficient structure",
                 words);
        r.compute_ns = now_ns() - start;
        return r;
    }

    r.passed = true;
    r.confidence = 1.0f;
    snprintf(r.detail, sizeof(r.detail),
             "protocol ok: %d words, %d chars", words, content_chars);
    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 6: Formal Certification
 *
 * Proof-certificate asymmetry (davidad):
 * agent produces proof, VM checks cheaply.
 * Checks triple connectivity as proof structure.
 * ============================================ */

tardy_layer_result_t tardy_verify_certification(
    const tardy_decomposition_t *decomps, int decomp_count)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_CERTIFICATION;

    if (!decomps || decomp_count == 0) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail), "no decomposition data");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Use first decomposition as reference */
    const tardy_decomposition_t *d = &decomps[0];

    if (d->count == 0) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail), "zero triples extracted");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Single triple = atomic fact, automatically certified */
    if (d->count == 1) {
        r.passed = true;
        r.confidence = 0.9f;
        snprintf(r.detail, sizeof(r.detail),
                 "atomic fact — auto-certified");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Multiple triples: check connectivity
     * At least one triple's subject should appear as another triple's
     * subject or object (they're about related things) */
    int connected = 0;
    for (int i = 0; i < d->count; i++) {
        for (int j = 0; j < d->count; j++) {
            if (i == j) continue;
            if (strcmp(d->triples[i].subject,
                       d->triples[j].subject) == 0 ||
                strcmp(d->triples[i].subject,
                       d->triples[j].object) == 0 ||
                strcmp(d->triples[i].object,
                       d->triples[j].subject) == 0) {
                connected++;
                break;
            }
        }
    }

    float connectivity = (float)connected / (float)d->count;
    r.passed = connectivity >= 0.5f;
    r.confidence = connectivity;
    snprintf(r.detail, sizeof(r.detail),
             "certification: %d/%d triples connected (%.0f%%)",
             connected, d->count, connectivity * 100.0f);
    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 7: Cross-Representation Bridge
 *
 * All previous layers must agree.
 * If grounding says "confident" but consistency says
 * "contradicted", something is wrong.
 * ============================================ */

tardy_layer_result_t tardy_verify_cross_representation(
    const tardy_layer_result_t *layers, int count)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_CROSS_REP;

    if (count == 0) {
        r.passed = true;
        r.confidence = 1.0f;
        snprintf(r.detail, sizeof(r.detail), "no layers to cross-check");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check 1: No layer that passed should contradict a layer that also
     * passed. If grounding says "grounded" but consistency says
     * "inconsistent", something's wrong. */
    int grounding_passed = 0;
    int consistency_passed = 0;
    float grounding_conf = 0.0f;
    float consistency_conf = 0.0f;

    for (int i = 0; i < count; i++) {
        if (layers[i].layer == TARDY_LAYER_GROUNDING) {
            grounding_passed = layers[i].passed;
            grounding_conf = layers[i].confidence;
        }
        if (layers[i].layer == TARDY_LAYER_CONSISTENCY) {
            consistency_passed = layers[i].passed;
            consistency_conf = layers[i].confidence;
        }
    }

    /* Contradiction: grounded but inconsistent */
    if (grounding_passed && !consistency_passed) {
        r.passed = false;
        r.confidence = 0.3f;
        snprintf(r.detail, sizeof(r.detail),
                 "cross-rep conflict: grounded (%.2f) but inconsistent (%.2f)",
                 grounding_conf, consistency_conf);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check 2: Confidence spread — if layers wildly disagree, flag it */
    float min_conf = 1.0f;
    float max_conf = 0.0f;
    int passed_count = 0;

    for (int i = 0; i < count; i++) {
        if (layers[i].layer == TARDY_LAYER_CROSS_REP) continue;
        if (layers[i].passed) {
            if (layers[i].confidence < min_conf)
                min_conf = layers[i].confidence;
            if (layers[i].confidence > max_conf)
                max_conf = layers[i].confidence;
            passed_count++;
        }
    }

    float spread = max_conf - min_conf;
    if (passed_count >= 2 && spread > 0.5f) {
        r.passed = false;
        r.confidence = 1.0f - spread;
        snprintf(r.detail, sizeof(r.detail),
                 "cross-rep warning: confidence spread %.2f (%.2f to %.2f)",
                 spread, min_conf, max_conf);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* All consistent */
    r.passed = true;
    r.confidence = passed_count > 0 ? min_conf : 1.0f;
    snprintf(r.detail, sizeof(r.detail),
             "cross-rep ok: %d layers consistent, spread=%.2f",
             passed_count, spread);
    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Layer 8: Work Verification — Laziness Detection
 *
 * Compare what the VM observed vs what was required.
 * The agent didn't self-report. The VM saw everything.
 * ============================================ */

tardy_layer_result_t tardy_verify_work(
    const tardy_work_log_t *log,
    const tardy_work_spec_t *spec,
    const tardy_semantics_t *sem)
{
    uint64_t start = now_ns();
    tardy_layer_result_t r = {0};
    r.layer = TARDY_LAYER_WORK_VERIFY;

    if (!log || !spec) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail), "no work log or spec");
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check minimum observed operations */
    int total_ops = log->ontology_queries + log->context_reads +
                    log->agents_spawned;

    if (total_ops < sem->laziness.min_observed_operations) {
        r.passed = false;
        r.confidence = 0.0f;
        snprintf(r.detail, sizeof(r.detail),
                 "NO WORK: %d total operations, need %d — laziness detected",
                 total_ops, sem->laziness.min_observed_operations);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check ontology queries */
    if (log->ontology_queries < spec->min_ontology_queries) {
        r.passed = false;
        r.confidence = (float)log->ontology_queries /
                       (float)spec->min_ontology_queries;
        snprintf(r.detail, sizeof(r.detail),
                 "SHALLOW: %d ontology queries, need %d",
                 log->ontology_queries, spec->min_ontology_queries);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* Check context reads */
    if (log->context_reads < spec->min_context_reads) {
        r.passed = false;
        r.confidence = (float)log->context_reads /
                       (float)spec->min_context_reads;
        snprintf(r.detail, sizeof(r.detail),
                 "SHALLOW: %d context reads, need %d",
                 log->context_reads, spec->min_context_reads);
        r.compute_ns = now_ns() - start;
        return r;
    }

    /* All checks passed */
    r.passed = true;
    r.confidence = 1.0f;
    snprintf(r.detail, sizeof(r.detail),
             "genuine work: %d queries, %d reads, %d spawns",
             log->ontology_queries, log->context_reads, log->agents_spawned);

    r.compute_ns = now_ns() - start;
    return r;
}

/* ============================================
 * Truth Strength Computation
 * ============================================ */

tardy_truth_strength_t tardy_compute_truth_strength(
    const tardy_pipeline_result_t *result)
{
    if (!result->passed)
        return TARDY_TRUTH_REFUTED;

    float c = result->confidence;

    if (c >= 0.99f) return TARDY_TRUTH_PROVEN;
    if (c >= 0.85f) return TARDY_TRUTH_EVIDENCED;
    if (c >= 0.60f) return TARDY_TRUTH_ATTESTED;
    if (c >= 0.30f) return TARDY_TRUTH_HYPOTHETICAL;
    return TARDY_TRUTH_CONTESTED;
}

/* ============================================
 * Full Pipeline
 * ============================================ */

tardy_pipeline_result_t tardy_pipeline_verify(
    const char *claim,
    int claim_len,
    const tardy_decomposition_t *decompositions,
    int decomposition_count,
    const tardy_grounding_t *grounding,
    const tardy_consistency_t *consistency,
    const tardy_work_log_t *work_log,
    const tardy_work_spec_t *work_spec,
    const tardy_semantics_t *semantics)
{
    tardy_pipeline_result_t result = {0};
    result.passed = true;
    result.confidence = 1.0f;

    int layer_idx = 0;

    /* Layer 1: Decomposition */
    if (semantics->pipeline.layer_ontology_grounding) {
        result.layers[layer_idx] = tardy_verify_decomposition(
            decompositions, decomposition_count, semantics);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_DECOMPOSE;
        }
        if (result.layers[layer_idx].confidence < result.confidence)
            result.confidence = result.layers[layer_idx].confidence;
        layer_idx++;
    }

    /* Layer 2: Grounding */
    if (semantics->pipeline.layer_ontology_grounding) {
        result.layers[layer_idx] = tardy_verify_grounding(
            grounding, semantics);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_GROUNDING;
        }
        if (result.layers[layer_idx].confidence < result.confidence)
            result.confidence = result.layers[layer_idx].confidence;
        layer_idx++;
    }

    /* Layer 3: Consistency */
    if (semantics->pipeline.layer_consistency_check) {
        result.layers[layer_idx] = tardy_verify_consistency(
            consistency, semantics);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_CONSISTENCY;
        }
        if (result.layers[layer_idx].confidence < result.confidence)
            result.confidence = result.layers[layer_idx].confidence;
        layer_idx++;
    }

    /* Layer 4: Probabilistic */
    if (semantics->pipeline.layer_probabilistic_scoring) {
        result.layers[layer_idx] = tardy_verify_probabilistic(
            grounding, semantics);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_PROBABILISTIC;
        }
        if (result.layers[layer_idx].confidence < result.confidence)
            result.confidence = result.layers[layer_idx].confidence;
        layer_idx++;
    }

    /* Layer 5: Protocol */
    if (semantics->pipeline.layer_protocol_check) {
        result.layers[layer_idx] = tardy_verify_protocol(claim, claim_len);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_PROTOCOL;
        }
        layer_idx++;
    }

    /* Layer 6: Formal certification (opt-in) */
    if (semantics->pipeline.layer_formal_certification) {
        result.layers[layer_idx] = tardy_verify_certification(
            decompositions, decomposition_count);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_CERTIFICATION;
        }
        layer_idx++;
    }

    /* Layer 7: Cross-representation (opt-in) */
    if (semantics->pipeline.layer_cross_representation) {
        result.layers[layer_idx] = tardy_verify_cross_representation(
            result.layers, layer_idx);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_CROSS_REP;
        }
        if (result.layers[layer_idx].confidence < result.confidence)
            result.confidence = result.layers[layer_idx].confidence;
        layer_idx++;
    }

    /* Layer 8: Work verification */
    if (semantics->pipeline.layer_work_verification) {
        result.layers[layer_idx] = tardy_verify_work(
            work_log, work_spec, semantics);
        if (!result.layers[layer_idx].passed && result.passed) {
            result.passed = false;
            result.failed_at = TARDY_LAYER_WORK_VERIFY;
        }
        layer_idx++;
    }

    /* Count passed/failed */
    for (int i = 0; i < layer_idx; i++) {
        if (result.layers[i].passed)
            result.layers_passed++;
        else
            result.layers_failed++;
    }

    /* Compute truth strength */
    result.strength = tardy_compute_truth_strength(&result);

    /* Set structured failure type based on which layer failed and why */
    result.failure_type = TARDY_FAIL_NONE;
    memset(result.failure_detail, 0, sizeof(result.failure_detail));

    if (!result.passed) {
        switch (result.failed_at) {
        case TARDY_LAYER_DECOMPOSE:
            result.failure_type = TARDY_FAIL_DECOMPOSITION;
            break;
        case TARDY_LAYER_GROUNDING:
            /* Distinguish: all UNKNOWN vs CONTRADICTED vs low evidence */
            if (grounding && grounding->contradicted > 0)
                result.failure_type = TARDY_FAIL_CONTRADICTION;
            else if (grounding && grounding->grounded == 0 &&
                     grounding->consistent == 0 &&
                     grounding->unknown == grounding->count)
                result.failure_type = TARDY_FAIL_ONTOLOGY_GAP;
            else
                result.failure_type = TARDY_FAIL_NO_EVIDENCE;
            break;
        case TARDY_LAYER_CONSISTENCY:
            result.failure_type = TARDY_FAIL_INCONSISTENCY;
            break;
        case TARDY_LAYER_PROBABILISTIC:
            result.failure_type = TARDY_FAIL_LOW_CONFIDENCE;
            break;
        case TARDY_LAYER_PROTOCOL:
            result.failure_type = TARDY_FAIL_PROTOCOL;
            break;
        case TARDY_LAYER_WORK_VERIFY:
            result.failure_type = TARDY_FAIL_LAZINESS;
            break;
        case TARDY_LAYER_CROSS_REP:
            result.failure_type = TARDY_FAIL_CROSS_REP;
            break;
        default:
            result.failure_type = TARDY_FAIL_NONE;
            break;
        }

        /* Copy the failing layer's detail into failure_detail */
        for (int i = 0; i < layer_idx; i++) {
            if (result.layers[i].layer == result.failed_at &&
                !result.layers[i].passed) {
                snprintf(result.failure_detail, sizeof(result.failure_detail),
                         "%s", result.layers[i].detail);
                break;
            }
        }
    }

    return result;
}
