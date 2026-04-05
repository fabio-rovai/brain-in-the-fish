#ifndef TARDY_FRAMES_H
#define TARDY_FRAMES_H

#include "datalog.h"
#include <stdbool.h>

#define TARDY_MAX_FRAMES 64
#define TARDY_MAX_FRAME_SLOTS 8

typedef struct {
    char name[64];       /* slot name: "city", "country" */
    char type[64];       /* type: "City", "Country", "Person", "Date", "Thing" */
    int  functional;     /* 1 = one value per key (capitalOf: one capital per country) */
    int  required;       /* 1 = must be filled */
} tardy_frame_slot_t;

typedef struct {
    char name[64];                        /* "Capital", "Creation", "Location" */
    char predicate[64];                   /* which predicate this covers */
    tardy_frame_slot_t slots[TARDY_MAX_FRAME_SLOTS];
    int  slot_count;                      /* always 2 for binary predicates */
    int  transitive;                      /* locatedIn(X,Y) + locatedIn(Y,Z) -> locatedIn(X,Z) */
    int  symmetric;                       /* relatedTo(X,Y) -> relatedTo(Y,X) */
    char inverse_pred[64];                /* inverse: capitalOf <-> capitalCity */
} tardy_frame_t;

typedef struct {
    tardy_frame_t frames[TARDY_MAX_FRAMES];
    int           count;
} tardy_frame_registry_t;

/* CRDT merge results */
typedef enum {
    TARDY_MERGE_OK,         /* merges cleanly */
    TARDY_MERGE_DUPLICATE,  /* already exists */
    TARDY_MERGE_CONFLICT,   /* violates functional dependency */
    TARDY_MERGE_DERIVED     /* already derivable */
} tardy_merge_result_t;

/* Initialize registry with backbone frames */
void tardy_frames_init(tardy_frame_registry_t *reg);

/* Find the frame for a predicate (case-insensitive, underscore-tolerant) */
const tardy_frame_t *tardy_frames_find(const tardy_frame_registry_t *reg,
                                        const char *predicate);

/* Dry-run merge: would this fact conflict with existing state? */
tardy_merge_result_t tardy_crdt_dry_merge(
    const tardy_frame_registry_t *frames,
    const tardy_dl_program_t *datalog,
    const char *pred, const char *arg1, const char *arg2);

/* Actual merge: add fact if consistent, reject if conflict */
tardy_merge_result_t tardy_crdt_merge(
    const tardy_frame_registry_t *frames,
    tardy_dl_program_t *datalog,
    const char *pred, const char *arg1, const char *arg2);

/* Learn type from a fact + its frame (adds type facts to datalog) */
void tardy_frames_learn_types(const tardy_frame_registry_t *frames,
                               tardy_dl_program_t *datalog,
                               const char *pred, const char *arg1,
                               const char *arg2);

#endif
