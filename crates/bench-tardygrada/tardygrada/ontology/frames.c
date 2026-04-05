/*
 * Tardygrada -- Frame System + CRDT Merge
 *
 * Structural schemas (frames) sit on top of the Datalog engine.
 * Each frame declares a predicate's slot types and functional
 * dependencies. The CRDT merge function uses these constraints
 * to accept or reject new facts algebraically.
 */

#include "frames.h"
#include <string.h>
#include <stdio.h>

/* ============================================
 * Predicate normalization (duplicated from datalog.c,
 * which keeps it static)
 * ============================================ */

static int pred_norm_match(const char *a, const char *b)
{
    char na[64], nb[64];
    int ai = 0, bi = 0;
    for (int i = 0; a[i] && ai < 63; i++) {
        if (a[i] != '_')
            na[ai++] = (char)((a[i] >= 'A' && a[i] <= 'Z')
                              ? a[i] + 32 : a[i]);
    }
    na[ai] = '\0';
    for (int i = 0; b[i] && bi < 63; i++) {
        if (b[i] != '_')
            nb[bi++] = (char)((b[i] >= 'A' && b[i] <= 'Z')
                              ? b[i] + 32 : b[i]);
    }
    nb[bi] = '\0';
    return strcmp(na, nb) == 0;
}

/* ============================================
 * Frame helpers
 * ============================================ */

static void init_slot(tardy_frame_slot_t *s, const char *name,
                      const char *type, int functional, int required)
{
    memset(s, 0, sizeof(*s));
    strncpy(s->name, name, 63);  s->name[63] = '\0';
    strncpy(s->type, type, 63);  s->type[63] = '\0';
    s->functional = functional;
    s->required   = required;
}

static void add_frame(tardy_frame_registry_t *reg,
                      const char *name, const char *predicate,
                      const char *s0_name, const char *s0_type, int s0_func,
                      const char *s1_name, const char *s1_type, int s1_func,
                      int transitive, int symmetric,
                      const char *inverse)
{
    if (reg->count >= TARDY_MAX_FRAMES) return;
    tardy_frame_t *f = &reg->frames[reg->count];
    memset(f, 0, sizeof(*f));

    strncpy(f->name, name, 63);        f->name[63] = '\0';
    strncpy(f->predicate, predicate, 63); f->predicate[63] = '\0';
    f->transitive = transitive;
    f->symmetric  = symmetric;
    if (inverse) {
        strncpy(f->inverse_pred, inverse, 63);
        f->inverse_pred[63] = '\0';
    }

    init_slot(&f->slots[0], s0_name, s0_type, s0_func, 1);
    init_slot(&f->slots[1], s1_name, s1_type, s1_func, 1);
    f->slot_count = 2;

    reg->count++;
}

/* ============================================
 * Init: load 8 backbone frames
 * ============================================ */

void tardy_frames_init(tardy_frame_registry_t *reg)
{
    if (!reg) return;
    memset(reg, 0, sizeof(*reg));

    /* 1. Capital */
    add_frame(reg, "Capital", "capitalOf",
              "city", "City", 1,
              "country", "Country", 1,
              0, 0, "capitalCity");

    /* 2. Location */
    add_frame(reg, "Location", "locatedIn",
              "entity", "Thing", 0,
              "place", "Place", 0,
              1, 0, "contains");

    /* 3. Creation */
    add_frame(reg, "Creation", "creator",
              "creation", "Thing", 0,
              "agent", "Agent", 0,
              0, 0, "createdBy");

    /* 4. Founding */
    add_frame(reg, "Founding", "founder",
              "org", "Organization", 0,
              "agent", "Agent", 0,
              0, 0, "foundedBy");

    /* 5. Invention */
    add_frame(reg, "Invention", "inventor",
              "invention", "Thing", 0,
              "agent", "Agent", 0,
              0, 0, "inventedBy");

    /* 6. Temporal */
    add_frame(reg, "Temporal", "dateCreated",
              "thing", "Thing", 1,
              "date", "Date", 1,
              0, 0, NULL);

    /* 7. Description */
    add_frame(reg, "Description", "description",
              "thing", "Thing", 0,
              "text", "Text", 0,
              0, 0, NULL);

    /* 8. KnownFor */
    add_frame(reg, "KnownFor", "knownFor",
              "agent", "Agent", 0,
              "thing", "Thing", 0,
              0, 0, NULL);
}

/* ============================================
 * Find frame by predicate or inverse
 * ============================================ */

const tardy_frame_t *tardy_frames_find(const tardy_frame_registry_t *reg,
                                        const char *predicate)
{
    if (!reg || !predicate) return NULL;

    for (int i = 0; i < reg->count; i++) {
        if (pred_norm_match(predicate, reg->frames[i].predicate))
            return &reg->frames[i];
        if (reg->frames[i].inverse_pred[0] &&
            pred_norm_match(predicate, reg->frames[i].inverse_pred))
            return &reg->frames[i];
    }
    return NULL;
}

/* ============================================
 * CRDT dry merge
 * ============================================ */

tardy_merge_result_t tardy_crdt_dry_merge(
    const tardy_frame_registry_t *frames,
    const tardy_dl_program_t *datalog,
    const char *pred, const char *arg1, const char *arg2)
{
    if (!datalog || !pred) return TARDY_MERGE_OK;

    /* 1. Already exists or derivable? */
    int exists = tardy_dl_query(datalog, pred, arg1, arg2);
    if (exists == 1) return TARDY_MERGE_DUPLICATE;
    if (exists == -1) return TARDY_MERGE_CONFLICT;

    /* 2. Find the frame for this predicate */
    const tardy_frame_t *frame = frames ? tardy_frames_find(frames, pred) : NULL;
    if (!frame) return TARDY_MERGE_OK;

    /* 3. Check functional dependencies.
     * slot[1].functional: for the same arg1 (subject), only one arg2 (object).
     * slot[0].functional: for the same arg2 (object), only one arg1 (subject). */
    if (frame->slots[1].functional) {
        for (int i = 0; i < datalog->fact_count; i++) {
            if (pred_norm_match(datalog->facts[i].pred, frame->predicate) &&
                pred_norm_match(datalog->facts[i].arg1, arg1) &&
                !pred_norm_match(datalog->facts[i].arg2, arg2)) {
                return TARDY_MERGE_CONFLICT;
            }
        }
    }

    if (frame->slots[0].functional) {
        for (int i = 0; i < datalog->fact_count; i++) {
            if (pred_norm_match(datalog->facts[i].pred, frame->predicate) &&
                pred_norm_match(datalog->facts[i].arg2, arg2) &&
                !pred_norm_match(datalog->facts[i].arg1, arg1)) {
                return TARDY_MERGE_CONFLICT;
            }
        }
    }

    return TARDY_MERGE_OK;
}

/* ============================================
 * CRDT merge: dry-run then commit
 * ============================================ */

tardy_merge_result_t tardy_crdt_merge(
    const tardy_frame_registry_t *frames,
    tardy_dl_program_t *datalog,
    const char *pred, const char *arg1, const char *arg2)
{
    tardy_merge_result_t result =
        tardy_crdt_dry_merge(frames, datalog, pred, arg1, arg2);
    if (result == TARDY_MERGE_OK) {
        tardy_dl_add_fact(datalog, pred, arg1, arg2);
        tardy_frames_learn_types(frames, datalog, pred, arg1, arg2);
        tardy_dl_evaluate(datalog);
    }
    return result;
}

/* ============================================
 * Type learning from frame slots
 * ============================================ */

void tardy_frames_learn_types(const tardy_frame_registry_t *frames,
                               tardy_dl_program_t *datalog,
                               const char *pred, const char *arg1,
                               const char *arg2)
{
    if (!frames || !datalog || !pred) return;

    const tardy_frame_t *frame = tardy_frames_find(frames, pred);
    if (!frame) return;

    /* Assign types from frame slots */
    if (frame->slots[0].type[0] && arg1 && arg1[0]) {
        if (tardy_dl_query(datalog, "type", arg1, frame->slots[0].type) != 1)
            tardy_dl_add_fact(datalog, "type", arg1, frame->slots[0].type);
    }
    if (frame->slot_count > 1 && frame->slots[1].type[0] && arg2 && arg2[0]) {
        if (tardy_dl_query(datalog, "type", arg2, frame->slots[1].type) != 1)
            tardy_dl_add_fact(datalog, "type", arg2, frame->slots[1].type);
    }
}
