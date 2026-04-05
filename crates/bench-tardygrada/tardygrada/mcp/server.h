/*
 * Tardygrada — MCP Server
 * A Tardygrada program IS an MCP server.
 * This is the compilation target, not a feature.
 *
 * Protocol: JSON-RPC 2.0 over stdin/stdout (MCP standard)
 */

#ifndef TARDY_MCP_SERVER_H
#define TARDY_MCP_SERVER_H

#include "../vm/vm.h"
#include "../ontology/bridge.h"
#include "../ontology/self.h"
#include "../ontology/frames.h"
#include "../ontology/inference.h"
#include "json.h"

#define TARDY_MCP_BUF_SIZE 8192

/* ============================================
 * Ontology Mode (TARDY_ONTOLOGY env var)
 * ============================================ */

typedef enum {
    TARDY_ONTOLOGY_BOTH,    /* default: bridge first, self-hosted fallback */
    TARDY_ONTOLOGY_BRIDGE,  /* bridge only, fail if unavailable            */
    TARDY_ONTOLOGY_SELF,    /* self-hosted only, skip bridge               */
    TARDY_ONTOLOGY_NONE     /* skip ontology entirely                      */
} tardy_ontology_mode_t;

/* ============================================
 * MCP Server
 * ============================================ */

typedef struct {
    tardy_vm_t              *vm;
    tardy_ontology_bridge_t  bridge;
    bool                     bridge_connected;
    tardy_self_ontology_t    self_ontology;
    bool                     self_ontology_loaded;
    tardy_ontology_mode_t    ontology_mode;
    tardy_frame_registry_t   frames;
    tardy_ruleset_t          ruleset;
    char                     read_buf[TARDY_MCP_BUF_SIZE];
    char                     write_buf[TARDY_MCP_BUF_SIZE];
    int                      running;
} tardy_mcp_server_t;

/* Initialize MCP server wrapping a VM */
int tardy_mcp_init(tardy_mcp_server_t *srv, tardy_vm_t *vm);

/* Run the MCP server — blocks, reads stdin, writes stdout */
int tardy_mcp_run(tardy_mcp_server_t *srv);

/* Process a single JSON-RPC request, write response */
int tardy_mcp_handle(tardy_mcp_server_t *srv, const char *request, int len);

/* Stop the server */
void tardy_mcp_stop(tardy_mcp_server_t *srv);

#endif /* TARDY_MCP_SERVER_H */
