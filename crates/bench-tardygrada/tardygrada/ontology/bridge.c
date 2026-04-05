/*
 * Tardygrada — Ontology Bridge Implementation
 *
 * Unix socket client that talks to open-ontologies.
 * Sends triples, receives grounding results.
 *
 * When the ontology engine isn't available, falls back to
 * When the ontology engine is unavailable, falls back to honest UNKNOWN status.
 * This means: without the ontology, nothing can become a Fact.
 * The system degrades to "honest uncertainty" not "false confidence."
 */

#include "bridge.h"
#include "../mcp/json.h"
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <string.h>
#include <stdio.h>

/* ============================================
 * Connection
 * ============================================ */

int tardy_ontology_connect(tardy_ontology_conn_t *conn,
                            const char *socket_path)
{
    if (!conn)
        return -1;

    memset(conn, 0, sizeof(tardy_ontology_conn_t));
    strncpy(conn->socket_path, socket_path, sizeof(conn->socket_path) - 1);

    conn->fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (conn->fd < 0)
        return -1;

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, socket_path, sizeof(addr.sun_path) - 1);

    if (connect(conn->fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(conn->fd);
        conn->fd = -1;
        conn->connected = false;
        return -1;
    }

    conn->connected = true;
    return 0;
}

void tardy_ontology_disconnect(tardy_ontology_conn_t *conn)
{
    if (conn && conn->fd >= 0) {
        close(conn->fd);
        conn->fd = -1;
        conn->connected = false;
    }
}

/* ============================================
 * Bridge Init/Shutdown
 * ============================================ */

int tardy_bridge_init(tardy_ontology_bridge_t *bridge,
                       const char *sketch_path,
                       const char *complete_path)
{
    if (!bridge)
        return -1;

    memset(bridge, 0, sizeof(tardy_ontology_bridge_t));

    /* Try to connect sketch ontology */
    int sketch_ok = tardy_ontology_connect(&bridge->sketch,
                                            sketch_path ? sketch_path :
                                            "/tmp/tardygrada-ontology-sketch.sock");

    /* Try to connect complete ontology */
    int complete_ok = tardy_ontology_connect(&bridge->complete,
                                              complete_path ? complete_path :
                                              "/tmp/tardygrada-ontology-complete.sock");

    bridge->dual_mode = (sketch_ok == 0 && complete_ok == 0);

    /* At least one must be available, or fall back to UNKNOWN */
    return (sketch_ok == 0 || complete_ok == 0) ? 0 : -1;
}

void tardy_bridge_shutdown(tardy_ontology_bridge_t *bridge)
{
    if (!bridge)
        return;
    tardy_ontology_disconnect(&bridge->sketch);
    tardy_ontology_disconnect(&bridge->complete);
}

/* ============================================
 * IRI Normalization
 *
 * The decomposer produces plain English names like "Doctor Who"
 * but the ontology stores CamelCase IRIs like "DoctorWho".
 * Bridge the gap here so grounding queries can match.
 * ============================================ */

/* Normalize a name for IRI matching:
 * "Doctor Who" -> "DoctorWho"
 * "BBC Television Centre" -> "BBCTelevisionCentre" */
static void normalize_for_iri(const char *input, char *output, int max_len)
{
    int oi = 0;
    int capitalize_next = 1; /* capitalize first char */
    for (int i = 0; input[i] && oi < max_len - 1; i++) {
        if (input[i] == ' ' || input[i] == '-' || input[i] == '_') {
            capitalize_next = 1;
            continue;
        }
        if (capitalize_next && input[i] >= 'a' && input[i] <= 'z') {
            output[oi++] = input[i] - 32; /* uppercase */
            capitalize_next = 0;
        } else {
            output[oi++] = input[i];
            capitalize_next = 0;
        }
    }
    output[oi] = '\0';
}

/* Map common decomposer predicates to schema.org predicates */
static const char *map_predicate(const char *pred)
{
    if (strcmp(pred, "created_at") == 0) return "locationCreated";
    if (strcmp(pred, "created_by") == 0) return "creator";
    if (strcmp(pred, "created_in") == 0) return "dateCreated";
    if (strcmp(pred, "born_in") == 0) return "birthPlace";
    if (strcmp(pred, "born_on") == 0) return "birthDate";
    if (strcmp(pred, "died_in") == 0) return "deathPlace";
    if (strcmp(pred, "died_on") == 0) return "deathDate";
    if (strcmp(pred, "located_in") == 0) return "address";
    if (strcmp(pred, "part_of") == 0) return "isPartOf";
    if (strcmp(pred, "type_of") == 0) return "additionalType";
    if (strcmp(pred, "is_a") == 0) return "additionalType";
    if (strcmp(pred, "is") == 0) return "description";
    if (strcmp(pred, "has") == 0) return "owns";
    if (strcmp(pred, "wrote") == 0) return "author";
    if (strcmp(pred, "directed") == 0) return "director";
    if (strcmp(pred, "produced") == 0) return "producer";
    if (strcmp(pred, "founded_by") == 0) return "founder";
    if (strcmp(pred, "founded_in") == 0) return "foundingDate";
    if (strcmp(pred, "works_for") == 0) return "worksFor";
    if (strcmp(pred, "works_at") == 0) return "workLocation";
    return pred; /* unmapped -- use as-is */
}

/* ============================================
 * Build JSON request for grounding
 * ============================================ */

static int build_ground_request(char *buf, int buf_size,
                                 const tardy_triple_t *triples, int count)
{
    int w = 0;
    w += snprintf(buf + w, buf_size - w, "{\"action\":\"ground\",\"triples\":[");

    for (int i = 0; i < count && w < buf_size - 400; i++) {
        if (i > 0)
            buf[w++] = ',';

        /* Normalize for IRI matching */
        char norm_s[TARDY_MAX_TRIPLE_LEN];
        char norm_o[TARDY_MAX_TRIPLE_LEN];
        normalize_for_iri(triples[i].subject, norm_s, TARDY_MAX_TRIPLE_LEN);
        normalize_for_iri(triples[i].object, norm_o, TARDY_MAX_TRIPLE_LEN);
        const char *norm_p = map_predicate(triples[i].predicate);

        w += snprintf(buf + w, buf_size - w,
                      "{\"s\":\"%s\",\"p\":\"%s\",\"o\":\"%s\"}",
                      norm_s, norm_p, norm_o);
    }

    w += snprintf(buf + w, buf_size - w, "]}");
    return w;
}

static int build_consistency_request(char *buf, int buf_size,
                                      const tardy_triple_t *triples, int count)
{
    int w = 0;
    w += snprintf(buf + w, buf_size - w,
                  "{\"action\":\"check_consistency\",\"triples\":[");

    for (int i = 0; i < count && w < buf_size - 400; i++) {
        if (i > 0)
            buf[w++] = ',';

        /* Normalize for IRI matching */
        char norm_s[TARDY_MAX_TRIPLE_LEN];
        char norm_o[TARDY_MAX_TRIPLE_LEN];
        normalize_for_iri(triples[i].subject, norm_s, TARDY_MAX_TRIPLE_LEN);
        normalize_for_iri(triples[i].object, norm_o, TARDY_MAX_TRIPLE_LEN);
        const char *norm_p = map_predicate(triples[i].predicate);

        w += snprintf(buf + w, buf_size - w,
                      "{\"s\":\"%s\",\"p\":\"%s\",\"o\":\"%s\"}",
                      norm_s, norm_p, norm_o);
    }

    w += snprintf(buf + w, buf_size - w, "]}");
    return w;
}

/* ============================================
 * Send request, receive response
 * ============================================ */

static int ontology_rpc(tardy_ontology_conn_t *conn,
                         const char *request, int req_len,
                         char *response, int resp_size)
{
    if (!conn->connected)
        return -1;

    /* Send */
    ssize_t sent = write(conn->fd, request, req_len);
    if (sent != req_len)
        return -1;

    /* Send newline delimiter */
    sent = write(conn->fd, "\n", 1);
    (void)sent;

    /* Receive (read until newline) */
    int total = 0;
    while (total < resp_size - 1) {
        ssize_t n = read(conn->fd, response + total, 1);
        if (n <= 0)
            return -1;
        if (response[total] == '\n')
            break;
        total++;
    }
    response[total] = '\0';
    return total;
}

/* ============================================
 * Grounding — send triples, get results
 * ============================================ */

int tardy_ontology_ground(tardy_ontology_conn_t *conn,
                           const tardy_triple_t *triples, int count,
                           tardy_grounding_t *out)
{
    memset(out, 0, sizeof(tardy_grounding_t));

    if (!conn || !conn->connected) {
        /* Fallback: everything is UNKNOWN (honest uncertainty) */
        out->count = count;
        for (int i = 0; i < count && i < TARDY_MAX_TRIPLES; i++) {
            out->results[i].triple = triples[i];
            out->results[i].status = TARDY_KNOWLEDGE_UNKNOWN;
            out->results[i].confidence = 0.0f;
            out->results[i].evidence_count = 0;
            out->unknown++;
        }
        return 0;
    }

    /* Build and send request */
    char request[TARDY_ONTOLOGY_BUF_SIZE];
    int req_len = build_ground_request(request, sizeof(request),
                                        triples, count);

    char response[TARDY_ONTOLOGY_BUF_SIZE];
    int resp_len = ontology_rpc(conn, request, req_len,
                                 response, sizeof(response));
    if (resp_len <= 0) {
        /* Connection failed — fall back to UNKNOWN */
        out->count = count;
        for (int i = 0; i < count && i < TARDY_MAX_TRIPLES; i++) {
            out->results[i].triple = triples[i];
            out->results[i].status = TARDY_KNOWLEDGE_UNKNOWN;
            out->unknown++;
        }
        return -1;
    }

    /* Parse response */
    tardy_json_parser_t parser;
    if (tardy_json_parse(&parser, response, resp_len) < 0)
        return -1;

    int results_tok = tardy_json_find(&parser, 0, "results");
    if (results_tok < 0)
        return -1;

    /* Parse each result */
    int tok = results_tok + 1;
    for (int i = 0; i < count && i < TARDY_MAX_TRIPLES && tok < parser.count; i++) {
        out->results[i].triple = triples[i];

        int status_tok = tardy_json_find(&parser, tok, "status");
        if (status_tok >= 0) {
            if (tardy_json_eq(&parser, status_tok, "grounded")) {
                out->results[i].status = TARDY_KNOWLEDGE_GROUNDED;
                out->grounded++;
            } else if (tardy_json_eq(&parser, status_tok, "contradicted")) {
                out->results[i].status = TARDY_KNOWLEDGE_CONTRADICTED;
                out->contradicted++;
            } else {
                out->results[i].status = TARDY_KNOWLEDGE_UNKNOWN;
                out->unknown++;
            }
        }

        int conf_tok = tardy_json_find(&parser, tok, "confidence");
        if (conf_tok >= 0)
            out->results[i].confidence =
                (float)tardy_json_int(&parser, conf_tok) / 100.0f;

        int ev_tok = tardy_json_find(&parser, tok, "evidence_count");
        if (ev_tok >= 0)
            out->results[i].evidence_count =
                (int)tardy_json_int(&parser, ev_tok);

        /* Skip to next result object */
        tok += parser.tokens[tok].children + 1;
        out->count++;
    }

    return 0;
}

/* ============================================
 * Consistency Check
 * ============================================ */

int tardy_ontology_check_consistency(tardy_ontology_conn_t *conn,
                                      const tardy_triple_t *triples, int count,
                                      tardy_consistency_t *out)
{
    memset(out, 0, sizeof(tardy_consistency_t));

    if (!conn || !conn->connected) {
        /* Fallback: assume consistent (can't prove inconsistency without data) */
        out->consistent = true;
        out->contradiction_count = 0;
        return 0;
    }

    char request[TARDY_ONTOLOGY_BUF_SIZE];
    int req_len = build_consistency_request(request, sizeof(request),
                                             triples, count);

    char response[TARDY_ONTOLOGY_BUF_SIZE];
    int resp_len = ontology_rpc(conn, request, req_len,
                                 response, sizeof(response));
    if (resp_len <= 0) {
        out->consistent = true; /* can't prove otherwise */
        return -1;
    }

    tardy_json_parser_t parser;
    if (tardy_json_parse(&parser, response, resp_len) < 0)
        return -1;

    int consistent_tok = tardy_json_find(&parser, 0, "consistent");
    if (consistent_tok >= 0)
        out->consistent = tardy_json_eq(&parser, consistent_tok, "true");

    int contra_tok = tardy_json_find(&parser, 0, "contradiction_count");
    if (contra_tok >= 0)
        out->contradiction_count = (int)tardy_json_int(&parser, contra_tok);

    int explain_tok = tardy_json_find(&parser, 0, "explanation");
    if (explain_tok >= 0)
        tardy_json_str(&parser, explain_tok,
                       out->explanation, sizeof(out->explanation));

    return 0;
}

/* ============================================
 * Full Dual-Mode Verification
 * ============================================ */

int tardy_bridge_verify(tardy_ontology_bridge_t *bridge,
                         const tardy_triple_t *triples, int count,
                         tardy_grounding_t *grounding,
                         tardy_consistency_t *consistency)
{
    if (!bridge)
        return -1;

    if (bridge->dual_mode) {
        /* Ground against sketch first (fast) */
        tardy_grounding_t sketch_grounding;
        tardy_ontology_ground(&bridge->sketch, triples, count,
                               &sketch_grounding);

        /* If sketch catches contradictions, fail fast */
        if (sketch_grounding.contradicted > 0) {
            *grounding = sketch_grounding;
            consistency->consistent = false;
            consistency->contradiction_count = sketch_grounding.contradicted;
            snprintf(consistency->explanation,
                     sizeof(consistency->explanation),
                     "sketch ontology found %d contradictions",
                     sketch_grounding.contradicted);
            return 0;
        }

        /* Ground against complete (slow, thorough) */
        tardy_ontology_ground(&bridge->complete, triples, count, grounding);

        /* Consistency check on complete ontology */
        tardy_ontology_check_consistency(&bridge->complete, triples, count,
                                          consistency);
    } else if (bridge->complete.connected) {
        tardy_ontology_ground(&bridge->complete, triples, count, grounding);
        tardy_ontology_check_consistency(&bridge->complete, triples, count,
                                          consistency);
    } else if (bridge->sketch.connected) {
        tardy_ontology_ground(&bridge->sketch, triples, count, grounding);
        tardy_ontology_check_consistency(&bridge->sketch, triples, count,
                                          consistency);
    } else {
        /* No ontology available — everything UNKNOWN */
        tardy_ontology_ground(NULL, triples, count, grounding);
        consistency->consistent = true;
        return -1;
    }

    return 0;
}
