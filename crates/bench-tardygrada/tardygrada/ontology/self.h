/*
 * Tardygrada — Self-Hosted Ontology Engine
 *
 * The knowledge graph IS agents. No external process needed.
 * Each triple is a @sovereign agent. Grounding = semantic query.
 * Consistency = constitution invariants.
 *
 * Replaces the unix socket bridge with in-process agent lookup.
 * Same verification guarantees, zero latency, zero dependencies.
 */

#ifndef TARDY_ONTOLOGY_SELF_H
#define TARDY_ONTOLOGY_SELF_H

#include "../vm/vm.h"
#include "../verify/pipeline.h"
#include "datalog.h"
#include "frames.h"

/* ============================================
 * Self-Hosted Ontology — triples as agents
 * ============================================ */

typedef struct {
    tardy_vm_t              *vm;
    tardy_uuid_t             ontology_agent;  /* parent agent holding all triples */
    int                      triple_count;
    bool                     initialized;
    tardy_dl_program_t       datalog;         /* Datalog inference engine */
    tardy_frame_registry_t   frames;          /* Frame schemas + CRDT merge */
} tardy_self_ontology_t;

/* Initialize self-hosted ontology within a VM */
int tardy_self_ontology_init(tardy_self_ontology_t *ont, tardy_vm_t *vm);

/* Load a triple as a @sovereign agent.
 * Format: "subject predicate object" stored as agent name "s|p|o"
 * The value is the full triple text for semantic search. */
int tardy_self_ontology_add(tardy_self_ontology_t *ont,
                             const char *subject,
                             const char *predicate,
                             const char *object);

/* Load triples from a Turtle (.ttl) file.
 * Parses basic N-Triples/Turtle subset. */
int tardy_self_ontology_load_ttl(tardy_self_ontology_t *ont,
                                  const char *path);

/* Ground triples against the self-hosted ontology.
 * Same interface as the bridge — drop-in replacement. */
int tardy_self_ontology_ground(tardy_self_ontology_t *ont,
                                const tardy_triple_t *triples, int count,
                                tardy_grounding_t *out);

/* Check consistency among triples.
 * Detects: same subject+predicate with different objects. */
int tardy_self_ontology_check_consistency(tardy_self_ontology_t *ont,
                                           const tardy_triple_t *triples,
                                           int count,
                                           tardy_consistency_t *out);

/* Full verify: ground + consistency in one call */
int tardy_self_ontology_verify(tardy_self_ontology_t *ont,
                                const tardy_triple_t *triples, int count,
                                tardy_grounding_t *grounding,
                                tardy_consistency_t *consistency);

#endif /* TARDY_ONTOLOGY_SELF_H */
