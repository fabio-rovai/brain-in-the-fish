/*
 * Tardygrada — Ontology Bridge
 *
 * Connects the VM to the open-ontologies OWL reasoning engine
 * via unix socket. The ontology engine is a separate process —
 * it's an agent itself. The VM doesn't care what language it's in.
 *
 * Protocol: JSON over unix socket.
 * Request: {"action": "ground", "triples": [...]}
 * Response: {"results": [...], "consistent": true/false}
 *
 * Two ontologies run in parallel:
 *   - Sketch: fast, permissive (fewer triples, faster reasoning)
 *   - Complete: slow, strict (full knowledge graph)
 */

#ifndef TARDY_ONTOLOGY_BRIDGE_H
#define TARDY_ONTOLOGY_BRIDGE_H

#include "../verify/pipeline.h"

#define TARDY_ONTOLOGY_SOCKET_PATH "/tmp/tardygrada-ontology.sock"
#define TARDY_ONTOLOGY_BUF_SIZE    4096

/* ============================================
 * Ontology Connection
 * ============================================ */

typedef struct {
    int  fd;                   /* unix socket fd */
    bool connected;
    char socket_path[256];
    char buf[TARDY_ONTOLOGY_BUF_SIZE];
} tardy_ontology_conn_t;

/* ============================================
 * Dual Ontology — sketch + complete
 * ============================================ */

typedef struct {
    tardy_ontology_conn_t sketch;     /* fast, permissive */
    tardy_ontology_conn_t complete;   /* slow, strict */
    bool dual_mode;                   /* true = check both */
} tardy_ontology_bridge_t;

/* ============================================
 * Connection Management
 * ============================================ */

/* Connect to ontology engine via unix socket */
int tardy_ontology_connect(tardy_ontology_conn_t *conn,
                            const char *socket_path);

/* Disconnect */
void tardy_ontology_disconnect(tardy_ontology_conn_t *conn);

/* Initialize dual-mode bridge */
int tardy_bridge_init(tardy_ontology_bridge_t *bridge,
                       const char *sketch_path,
                       const char *complete_path);

/* Shutdown bridge */
void tardy_bridge_shutdown(tardy_ontology_bridge_t *bridge);

/* ============================================
 * Grounding Operations
 * ============================================ */

/* Ground a set of triples against the ontology.
 * Sends triples to the ontology engine, gets back grounding results.
 */
int tardy_ontology_ground(tardy_ontology_conn_t *conn,
                           const tardy_triple_t *triples, int count,
                           tardy_grounding_t *out);

/* Check consistency of a set of triples.
 * Sends triples to the OWL reasoner, gets back consistency result.
 */
int tardy_ontology_check_consistency(tardy_ontology_conn_t *conn,
                                      const tardy_triple_t *triples, int count,
                                      tardy_consistency_t *out);

/* Full dual-mode verification:
 * Grounds against both sketch and complete ontologies.
 * Both must pass (if dual_mode is true).
 */
int tardy_bridge_verify(tardy_ontology_bridge_t *bridge,
                         const tardy_triple_t *triples, int count,
                         tardy_grounding_t *grounding,
                         tardy_consistency_t *consistency);

#endif /* TARDY_ONTOLOGY_BRIDGE_H */
