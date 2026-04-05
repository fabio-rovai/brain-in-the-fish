#include "bridge.h"
#include "../mcp/json.h"
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <string.h>
#include <stdio.h>

int tardy_bitf_connect(tardy_bitf_conn_t *conn, const char *socket_path)
{
    if (!conn) return -1;
    memset(conn, 0, sizeof(tardy_bitf_conn_t));

    conn->fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (conn->fd < 0) return -1;

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, socket_path ? socket_path : TARDY_BITF_SOCKET_PATH,
            sizeof(addr.sun_path) - 1);

    if (connect(conn->fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(conn->fd);
        conn->fd = -1;
        conn->connected = false;
        return -1;
    }

    conn->connected = true;
    return 0;
}

void tardy_bitf_disconnect(tardy_bitf_conn_t *conn)
{
    if (conn && conn->fd >= 0) {
        close(conn->fd);
        conn->fd = -1;
        conn->connected = false;
    }
}

static int bitf_rpc(tardy_bitf_conn_t *conn,
                     const char *request, int req_len,
                     char *response, int resp_size)
{
    if (!conn->connected) return -1;

    ssize_t sent = write(conn->fd, request, req_len);
    if (sent != req_len) return -1;
    sent = write(conn->fd, "\n", 1);
    (void)sent;

    int total = 0;
    while (total < resp_size - 1) {
        ssize_t n = read(conn->fd, response + total, 1);
        if (n <= 0) return -1;
        if (response[total] == '\n') break;
        total++;
    }
    response[total] = '\0';
    return total;
}

int tardy_bitf_coordinate(tardy_bitf_conn_t *conn,
                           const char *task,
                           const char **agents, int agent_count,
                           tardy_bitf_result_t *out)
{
    if (!conn || !conn->connected || !out) {
        if (out) { memset(out, 0, sizeof(tardy_bitf_result_t)); }
        return -1;
    }
    memset(out, 0, sizeof(tardy_bitf_result_t));

    /* Build request JSON */
    char request[TARDY_BITF_BUF_SIZE];
    int w = snprintf(request, sizeof(request),
                     "{\"action\":\"coordinate\",\"task\":\"%.500s\",\"agents\":[",
                     task ? task : "");
    for (int i = 0; i < agent_count && w < (int)sizeof(request) - 100; i++) {
        if (i > 0) request[w++] = ',';
        w += snprintf(request + w, sizeof(request) - w, "\"%s\"", agents[i]);
    }
    w += snprintf(request + w, sizeof(request) - w, "]}");

    char response[TARDY_BITF_BUF_SIZE];
    int resp_len = bitf_rpc(conn, request, w, response, sizeof(response));
    if (resp_len <= 0) {
        strncpy(out->error, "rpc failed", sizeof(out->error));
        return -1;
    }

    /* Parse response */
    tardy_json_parser_t parser;
    if (tardy_json_parse(&parser, response, resp_len) < 0) {
        strncpy(out->error, "json parse failed", sizeof(out->error));
        return -1;
    }

    int err_tok = tardy_json_find(&parser, 0, "error");
    if (err_tok >= 0) {
        tardy_json_str(&parser, err_tok, out->error, sizeof(out->error));
        return -1;
    }

    int conf_tok = tardy_json_find(&parser, 0, "confidence");
    if (conf_tok >= 0) {
        out->confidence = (float)tardy_json_int(&parser, conf_tok) / 100.0f;
        /* Handle decimal values */
        if (out->confidence == 0.0f) {
            /* Try parsing as string for decimal */
            char cs[32];
            tardy_json_str(&parser, conf_tok, cs, sizeof(cs));
            float f = 0.0f;
            int i = 0;
            for (; cs[i] && cs[i] != '.'; i++) f = f * 10.0f + (cs[i] - '0');
            if (cs[i] == '.') { i++; float frac = 0.1f; for (; cs[i]; i++) { f += (cs[i] - '0') * frac; frac *= 0.1f; } }
            if (f > 0.0f) out->confidence = f;
        }
    }

    int rounds_tok = tardy_json_find(&parser, 0, "rounds");
    if (rounds_tok >= 0)
        out->rounds = (int)tardy_json_int(&parser, rounds_tok);

    out->success = true;
    return 0;
}

int tardy_bitf_gate(tardy_bitf_conn_t *conn,
                     const char *claim, float threshold,
                     tardy_bitf_gate_t *out)
{
    if (!conn || !conn->connected || !out) {
        if (out) memset(out, 0, sizeof(tardy_bitf_gate_t));
        return -1;
    }
    memset(out, 0, sizeof(tardy_bitf_gate_t));

    char request[TARDY_BITF_BUF_SIZE];
    int w = snprintf(request, sizeof(request),
                     "{\"action\":\"gate\",\"claim\":\"%.500s\",\"threshold\":%.2f}",
                     claim ? claim : "", threshold);

    char response[TARDY_BITF_BUF_SIZE];
    int resp_len = bitf_rpc(conn, request, w, response, sizeof(response));
    if (resp_len <= 0) return -1;

    tardy_json_parser_t parser;
    if (tardy_json_parse(&parser, response, resp_len) < 0) return -1;

    int passed_tok = tardy_json_find(&parser, 0, "passed");
    if (passed_tok >= 0)
        out->passed = tardy_json_eq(&parser, passed_tok, "true");

    int score_tok = tardy_json_find(&parser, 0, "score");
    if (score_tok >= 0)
        out->score = (float)tardy_json_int(&parser, score_tok) / 100.0f;

    int detail_tok = tardy_json_find(&parser, 0, "detail");
    if (detail_tok >= 0)
        tardy_json_str(&parser, detail_tok, out->detail, sizeof(out->detail));

    return 0;
}
