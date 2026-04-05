#ifndef TARDY_COORDINATE_BRIDGE_H
#define TARDY_COORDINATE_BRIDGE_H

#include <stdbool.h>

#define TARDY_BITF_SOCKET_PATH "/tmp/tardygrada-bitf.sock"
#define TARDY_BITF_BUF_SIZE    4096
#define TARDY_BITF_MAX_AGENTS  16

typedef struct {
    bool  success;
    float confidence;
    int   rounds;
    int   agent_count;
    char  agent_names[TARDY_BITF_MAX_AGENTS][64];
    float agent_scores[TARDY_BITF_MAX_AGENTS];
    char  error[256];
} tardy_bitf_result_t;

typedef struct {
    bool  passed;
    float score;
    char  detail[256];
} tardy_bitf_gate_t;

typedef struct {
    int   fd;
    bool  connected;
    char  buf[TARDY_BITF_BUF_SIZE];
} tardy_bitf_conn_t;

int tardy_bitf_connect(tardy_bitf_conn_t *conn, const char *socket_path);
void tardy_bitf_disconnect(tardy_bitf_conn_t *conn);

int tardy_bitf_coordinate(tardy_bitf_conn_t *conn,
                           const char *task,
                           const char **agents, int agent_count,
                           tardy_bitf_result_t *out);

int tardy_bitf_gate(tardy_bitf_conn_t *conn,
                     const char *claim, float threshold,
                     tardy_bitf_gate_t *out);

#endif
