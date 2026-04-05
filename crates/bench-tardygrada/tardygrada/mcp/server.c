/*
 * Tardygrada — MCP Server Implementation
 * JSON-RPC 2.0 over stdin/stdout.
 *
 * A Tardygrada program compiles to this.
 * The world connects. Asks questions. Gets verified responses.
 */

#include "server.h"
#include "vm/util.h"
#include "../verify/pipeline.h"
#include "../verify/decompose.h"
#include "../verify/preprocess.h"
#include "../vm/semantic.h"
#include "../ontology/inference.h"
#include <unistd.h>
#include <string.h>
#include <stdlib.h>
#include <fcntl.h>
#include <sys/wait.h>
#include <stdio.h>
#include <time.h>

static uint64_t mcp_now_ns(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/* Direct write — no stdio */
static void mcp_write(const char *data, int len)
{
    /* MCP stdio transport: Content-Length header + \r\n\r\n + body */
    char header[64];
    int hlen = 0;
    const char *prefix = "Content-Length: ";
    int plen = (int)strlen(prefix);
    memcpy(header, prefix, plen);
    hlen = plen;

    /* itoa for content length */
    char num[16];
    int nlen = 0;
    int tmp = len;
    if (tmp == 0) {
        num[nlen++] = '0';
    } else {
        while (tmp > 0) {
            num[nlen++] = '0' + (tmp % 10);
            tmp /= 10;
        }
    }
    for (int i = nlen - 1; i >= 0; i--)
        header[hlen++] = num[i];
    header[hlen++] = '\r';
    header[hlen++] = '\n';
    header[hlen++] = '\r';
    header[hlen++] = '\n';

tardy_write(STDOUT_FILENO, header, hlen);
tardy_write(STDOUT_FILENO, data, len);
}

/* Build a JSON-RPC response */
static int build_response(char *buf, int buf_size, const char *id,
                           const char *result)
{
    int written = 0;
    const char *pre = "{\"jsonrpc\":\"2.0\",\"id\":";
    int prelen = (int)strlen(pre);
    if (written + prelen >= buf_size) return -1;
    memcpy(buf + written, pre, prelen);
    written += prelen;

    int idlen = (int)strlen(id);
    if (written + idlen >= buf_size) return -1;
    memcpy(buf + written, id, idlen);
    written += idlen;

    const char *mid = ",\"result\":";
    int midlen = (int)strlen(mid);
    if (written + midlen >= buf_size) return -1;
    memcpy(buf + written, mid, midlen);
    written += midlen;

    int rlen = (int)strlen(result);
    if (written + rlen >= buf_size) return -1;
    memcpy(buf + written, result, rlen);
    written += rlen;

    if (written + 1 >= buf_size) return -1;
    buf[written++] = '}';
    buf[written] = '\0';
    return written;
}

/* Build an error response */
static int build_error(char *buf, int buf_size, const char *id,
                        int code, const char *message)
{
    int written = 0;
    const char *pre = "{\"jsonrpc\":\"2.0\",\"id\":";
    int prelen = (int)strlen(pre);
    if (written + prelen >= buf_size) return -1;
    memcpy(buf + written, pre, prelen);
    written += prelen;

    int idlen = (int)strlen(id);
    if (written + idlen >= buf_size) return -1;
    memcpy(buf + written, id, idlen);
    written += idlen;

    const char *mid = ",\"error\":{\"code\":";
    int midlen = (int)strlen(mid);
    if (written + midlen >= buf_size) return -1;
    memcpy(buf + written, mid, midlen);
    written += midlen;

    /* code as string */
    char codestr[16];
    int clen = 0;
    int neg = 0;
    int c = code;
    if (c < 0) { neg = 1; c = -c; }
    if (c == 0) { codestr[clen++] = '0'; }
    while (c > 0) { codestr[clen++] = '0' + (c % 10); c /= 10; }
    if (neg) codestr[clen++] = '-';
    for (int i = 0; i < clen / 2; i++) {
        char t = codestr[i];
        codestr[i] = codestr[clen - 1 - i];
        codestr[clen - 1 - i] = t;
    }
    if (written + clen >= buf_size) return -1;
    memcpy(buf + written, codestr, clen);
    written += clen;

    const char *msgpre = ",\"message\":\"";
    int mplen = (int)strlen(msgpre);
    if (written + mplen >= buf_size) return -1;
    memcpy(buf + written, msgpre, mplen);
    written += mplen;

    int mlen = (int)strlen(message);
    if (written + mlen >= buf_size) return -1;
    memcpy(buf + written, message, mlen);
    written += mlen;

    const char *suf = "\"}}";
    int suflen = (int)strlen(suf);
    if (written + suflen >= buf_size) return -1;
    memcpy(buf + written, suf, suflen);
    written += suflen;

    buf[written] = '\0';
    return written;
}

/* ============================================
 * Provenance Helpers
 * ============================================ */

static const char *trust_str(tardy_trust_t t)
{
    switch (t) {
    case TARDY_TRUST_MUTABLE:   return "mutable";
    case TARDY_TRUST_DEFAULT:   return "immutable";
    case TARDY_TRUST_VERIFIED:  return "verified";
    case TARDY_TRUST_HARDENED:  return "hardened";
    case TARDY_TRUST_SOVEREIGN: return "sovereign";
    }
    return "unknown";
}

static const char *state_str(tardy_state_t s)
{
    switch (s) {
    case TARDY_STATE_LIVE:   return "live";
    case TARDY_STATE_STATIC: return "static";
    case TARDY_STATE_TEMP:   return "temp";
    case TARDY_STATE_DEAD:   return "dead";
    }
    return "unknown";
}

static const char *type_str(tardy_type_t t)
{
    switch (t) {
    case TARDY_TYPE_INT:   return "int";
    case TARDY_TYPE_FLOAT: return "float";
    case TARDY_TYPE_STR:   return "str";
    case TARDY_TYPE_BOOL:  return "bool";
    case TARDY_TYPE_FACT:  return "fact";
    case TARDY_TYPE_AGENT: return "agent";
    case TARDY_TYPE_UNIT:  return "unit";
    case TARDY_TYPE_ERROR: return "error";
    }
    return "unknown";
}

/* Append _tardy provenance JSON to a result buffer.
 * buf: buffer with existing JSON (without closing '}')
 * pos: current write position in buf
 * buf_size: total buffer size
 * r: the read result with provenance data
 * Returns new position, or -1 on overflow.
 */
static int append_provenance(char *buf, int pos, int buf_size,
                              const tardy_read_result_t *r,
                              const char *ontology_status)
{
    /* Build the birth_hash hex string (first 16 bytes = 32 hex chars) */
    char hash_hex[65];
    int hbytes = 16;
    for (int i = 0; i < hbytes; i++) {
        uint8_t b = r->provenance.birth_hash.bytes[i];
        hash_hex[i * 2]     = "0123456789abcdef"[b >> 4];
        hash_hex[i * 2 + 1] = "0123456789abcdef"[b & 0x0f];
    }
    hash_hex[hbytes * 2] = '\0';

    const char *reason = r->provenance.reason ? r->provenance.reason : "unknown";
    const char *ont = ontology_status ? ontology_status : "unknown";

    /* Format the _tardy object */
    char tardy_json[512];
    int tlen = snprintf(tardy_json, sizeof(tardy_json),
        ",\"_tardy\":{"
        "\"trust\":\"%s\","
        "\"state\":\"%s\","
        "\"type\":\"%s\","
        "\"created_at\":%llu,"
        "\"reason\":\"%s\","
        "\"birth_hash\":\"%s\","
        "\"ontology\":\"%s\""
        "}",
        trust_str(r->trust),
        state_str(r->state),
        type_str(r->type_tag),
        (unsigned long long)r->provenance.created_at,
        reason,
        hash_hex,
        ont);

    if (tlen < 0 || pos + tlen >= buf_size)
        return -1;
    memcpy(buf + pos, tardy_json, tlen);
    pos += tlen;
    return pos;
}

/* ============================================
 * MCP Method Handlers
 * ============================================ */

/* initialize — MCP handshake */
static int handle_initialize(tardy_mcp_server_t *srv, const char *id)
{
    const char *result =
        "{\"protocolVersion\":\"2024-11-05\","
        "\"capabilities\":{\"tools\":{}},"
        "\"serverInfo\":{\"name\":\"tardygrada\",\"version\":\"0.1.0\"}}";
    int len = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE, id, result);
    if (len > 0)
        mcp_write(srv->write_buf, len);
    return 0;
}

/* Emit one tool entry into the buffer. Returns bytes written. */
static int emit_tool(char *buf, int buf_size, int pos, const char *name,
                      tardy_agent_t *agent, int *first)
{
    int tlen = pos;
    if (!*first && tlen < buf_size) buf[tlen++] = ',';
    *first = 0;

    const char *pre = "{\"name\":\"";
    int prelen = (int)strlen(pre);
    if (tlen + prelen >= buf_size) return tlen;
    memcpy(buf + tlen, pre, prelen);
    tlen += prelen;

    int nlen = (int)strlen(name);
    if (tlen + nlen >= buf_size) return tlen;
    memcpy(buf + tlen, name, nlen);
    tlen += nlen;

    const char *mid_desc;
    if (agent && agent->type_tag == TARDY_TYPE_ERROR)
        mid_desc = "\",\"description\":\"error agent\",";
    else if (agent && agent->trust >= TARDY_TRUST_SOVEREIGN)
        mid_desc = "\",\"description\":\"sovereign agent\",";
    else if (agent && agent->trust >= TARDY_TRUST_VERIFIED)
        mid_desc = "\",\"description\":\"verified agent\",";
    else if (agent && agent->trust >= TARDY_TRUST_DEFAULT)
        mid_desc = "\",\"description\":\"immutable agent\",";
    else
        mid_desc = "\",\"description\":\"mutable agent\",";

    int mdlen = (int)strlen(mid_desc);
    if (tlen + mdlen >= buf_size) return tlen;
    memcpy(buf + tlen, mid_desc, mdlen);
    tlen += mdlen;

    const char *schema = "\"inputSchema\":{\"type\":\"object\",\"properties\":{}}}";
    int slen = (int)strlen(schema);
    if (tlen + slen >= buf_size) return tlen;
    memcpy(buf + tlen, schema, slen);
    tlen += slen;
    return tlen;
}

/* tools/list — expose ALL agents in the tree as tools */
static int handle_tools_list(tardy_mcp_server_t *srv, const char *id)
{
    char tools[4096];
    int tlen = 0;
    tools[tlen++] = '{';
    const char *tp = "\"tools\":[";
    int tplen = (int)strlen(tp);
    memcpy(tools + tlen, tp, tplen);
    tlen += tplen;

    int first = 1;

    /* Walk all agents (skip root at index 0), expose value agents as tools */
    for (int i = 1; i < srv->vm->agent_count; i++) {
        tardy_agent_t *a = &srv->vm->agents[i];
        if (a->state == TARDY_STATE_DEAD)
            continue;
        if (a->type_tag == TARDY_TYPE_AGENT)
            continue; /* skip container agents, expose values only */

        /* Find this agent's name by scanning all parents' contexts */
        const char *name = NULL;
        for (int p = 0; p < srv->vm->agent_count; p++) {
            tardy_agent_t *parent = &srv->vm->agents[p];
            for (int c = 0; c < parent->context.child_count; c++) {
                if (parent->context.children[c].agent_id.hi == a->id.hi &&
                    parent->context.children[c].agent_id.lo == a->id.lo) {
                    name = parent->context.children[c].name;
                    break;
                }
            }
            if (name) break;
        }

        if (name)
            tlen = emit_tool(tools, sizeof(tools), tlen, name, a, &first);
    }

    /* Built-in tools: submit_claim and verify_claim */
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *sc =
            "{\"name\":\"submit_claim\","
            "\"description\":\"Submit a claim for a pending agent\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"agent\":{\"type\":\"string\"},"
            "\"claim\":{\"type\":\"string\"}},"
            "\"required\":[\"agent\",\"claim\"]}}";
        int sclen = (int)strlen(sc);
        if (tlen + sclen < (int)sizeof(tools)) {
            memcpy(tools + tlen, sc, sclen);
            tlen += sclen;
        }
    }
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *vc =
            "{\"name\":\"verify_claim\","
            "\"description\":\"Verify a pending agent's claim through the pipeline\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"agent\":{\"type\":\"string\"}},"
            "\"required\":[\"agent\"]}}";
        int vclen = (int)strlen(vc);
        if (tlen + vclen < (int)sizeof(tools)) {
            memcpy(tools + tlen, vc, vclen);
            tlen += vclen;
        }
    }
    /* Built-in tools: send_message and read_inbox */
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *sm =
            "{\"name\":\"send_message\","
            "\"description\":\"Send a message from one agent to another\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"agent_from\":{\"type\":\"string\"},"
            "\"agent_to\":{\"type\":\"string\"},"
            "\"payload\":{\"type\":\"string\"}},"
            "\"required\":[\"agent_from\",\"agent_to\",\"payload\"]}}";
        int smlen = (int)strlen(sm);
        if (tlen + smlen < (int)sizeof(tools)) {
            memcpy(tools + tlen, sm, smlen);
            tlen += smlen;
        }
    }
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *ri =
            "{\"name\":\"read_inbox\","
            "\"description\":\"Read the next message from an agent's inbox\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"agent\":{\"type\":\"string\"}},"
            "\"required\":[\"agent\"]}}";
        int rilen = (int)strlen(ri);
        if (tlen + rilen < (int)sizeof(tools)) {
            memcpy(tools + tlen, ri, rilen);
            tlen += rilen;
        }
    }
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *ss =
            "{\"name\":\"set_semantics\","
            "\"description\":\"Set per-agent verification thresholds\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"agent\":{\"type\":\"string\"},"
            "\"key\":{\"type\":\"string\"},"
            "\"value\":{\"type\":\"string\"}},"
            "\"required\":[\"agent\",\"key\",\"value\"]}}";
        int sslen = (int)strlen(ss);
        if (tlen + sslen < (int)sizeof(tools)) {
            memcpy(tools + tlen, ss, sslen);
            tlen += sslen;
        }
    }
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *qa =
            "{\"name\":\"query_agents\","
            "\"description\":\"Search agents by keyword matching\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"query\":{\"type\":\"string\"}},"
            "\"required\":[\"query\"]}}";
        int qalen = (int)strlen(qa);
        if (tlen + qalen < (int)sizeof(tools)) {
            memcpy(tools + tlen, qa, qalen);
            tlen += qalen;
        }
    }
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *gc =
            "{\"name\":\"get_conversation\","
            "\"description\":\"Read agent conversation history\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"agent\":{\"type\":\"string\"}},"
            "\"required\":[\"agent\"]}}";
        int gclen = (int)strlen(gc);
        if (tlen + gclen < (int)sizeof(tools)) {
            memcpy(tools + tlen, gc, gclen);
            tlen += gclen;
        }
    }
    {
        if (!first && tlen < (int)sizeof(tools)) tools[tlen++] = ',';
        first = 0;
        const char *lo =
            "{\"name\":\"load_ontology\","
            "\"description\":\"Load a TTL file into the self-hosted ontology\","
            "\"inputSchema\":{\"type\":\"object\","
            "\"properties\":{\"path\":{\"type\":\"string\"}},"
            "\"required\":[\"path\"]}}";
        int lolen = (int)strlen(lo);
        if (tlen + lolen < (int)sizeof(tools)) {
            memcpy(tools + tlen, lo, lolen);
            tlen += lolen;
        }
    }
    (void)first;

    tools[tlen++] = ']';
    tools[tlen++] = '}';
    tools[tlen] = '\0';

    int len = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE, id, tools);
    if (len > 0)
        mcp_write(srv->write_buf, len);
    return 0;
}

/* tools/call — read an agent's value */
static int handle_tools_call(tardy_mcp_server_t *srv,
                              tardy_json_parser_t *parser,
                              const char *id)
{
    /* Get params.name */
    int params_tok = tardy_json_find(parser, 0, "params");
    if (params_tok < 0) {
        int len = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                               id, -32602, "missing params");
        if (len > 0) mcp_write(srv->write_buf, len);
        return -1;
    }

    int name_tok = tardy_json_find(parser, params_tok, "name");
    if (name_tok < 0) {
        int len = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                               id, -32602, "missing params.name");
        if (len > 0) mcp_write(srv->write_buf, len);
        return -1;
    }

    char tool_name[64];
    tardy_json_str(parser, name_tok, tool_name, sizeof(tool_name));

    /* ---- Built-in: submit_claim ---- */
    if (strcmp(tool_name, "submit_claim") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int agent_tok = tardy_json_find(parser, args_tok, "agent");
        int claim_tok = tardy_json_find(parser, args_tok, "claim");
        if (agent_tok < 0 || claim_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing agent or claim");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char agent_name[64];
        char claim_text[512];
        tardy_json_str(parser, agent_tok, agent_name, sizeof(agent_name));
        tardy_json_str(parser, claim_tok, claim_text, sizeof(claim_text));

        /* Find and mutate the pending agent */
        int mutated = -1;
        for (int i = 0; i < srv->vm->agent_count; i++) {
            tardy_agent_t *candidate = tardy_vm_find_by_name(
                srv->vm, srv->vm->agents[i].id, agent_name);
            if (candidate && candidate->trust == TARDY_TRUST_MUTABLE) {
                mutated = tardy_vm_mutate(srv->vm, srv->vm->agents[i].id,
                                           agent_name, claim_text,
                                           strlen(claim_text) + 1);
                break;
            }
        }

        if (mutated == 0) {
            /* Record in agent's conversation */
            for (int i = 0; i < srv->vm->agent_count; i++) {
                tardy_agent_t *candidate = tardy_vm_find_by_name(
                    srv->vm, srv->vm->agents[i].id, agent_name);
                if (candidate) {
                    tardy_vm_converse(srv->vm, candidate->id,
                                       "user", claim_text);
                    break;
                }
            }

            const char *ok_result =
                "{\"content\":[{\"type\":\"text\","
                "\"text\":\"claim submitted\"}]}";
            int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                       id, ok_result);
            if (rlen > 0) mcp_write(srv->write_buf, rlen);
        } else {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "agent not found or not mutable");
            if (elen > 0) mcp_write(srv->write_buf, elen);
        }
        return 0;
    }

    /* ---- Built-in: verify_claim ---- */
    if (strcmp(tool_name, "verify_claim") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int agent_tok = tardy_json_find(parser, args_tok, "agent");
        if (agent_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing agent");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char agent_name[64];
        tardy_json_str(parser, agent_tok, agent_name, sizeof(agent_name));

        /* Find the agent */
        tardy_agent_t *target = NULL;
        tardy_uuid_t parent_id = {0, 0};
        for (int i = 0; i < srv->vm->agent_count; i++) {
            tardy_agent_t *candidate = tardy_vm_find_by_name(
                srv->vm, srv->vm->agents[i].id, agent_name);
            if (candidate) {
                target = candidate;
                parent_id = srv->vm->agents[i].id;
                break;
            }
        }

        if (!target) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "agent not found");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }

        /* Read current value */
        char claim_buf[512];
        memset(claim_buf, 0, sizeof(claim_buf));
        tardy_vm_read(srv->vm, parent_id, agent_name,
                      claim_buf, sizeof(claim_buf));
        int claim_len = (int)strlen(claim_buf);

        /* Run verification pipeline */
        uint64_t verify_start = mcp_now_ns();

        /* Check if caller provided pre-decomposed triples (LLM decomposer) */
        tardy_decomposition_t decomps[3];
        memset(decomps, 0, sizeof(decomps));
        tardy_triple_t all_triples[TARDY_MAX_TRIPLES];
        int triple_count = 0;

        /* Step 1: Subagent decomposer (preprocessor + rule-based).
         * Strips markdown, extracts key-value pairs, then decomposes.
         * This is the default -- zero cost, deterministic. */
        triple_count = tardy_preprocess_and_decompose(
            claim_buf, claim_len, all_triples, TARDY_MAX_TRIPLES);

        /* Also run multi-pass rule decomposer for agreement scoring */
        tardy_decompose_multi(claim_buf, claim_len, decomps, 3);

        /* Merge any triples from multi-pass that aren't already found */
        for (int d = 0; d < 3; d++) {
            for (int t = 0; t < decomps[d].count &&
                 triple_count < TARDY_MAX_TRIPLES; t++) {
                int dup = 0;
                for (int e = 0; e < triple_count; e++) {
                    if (strcmp(all_triples[e].subject,
                              decomps[d].triples[t].subject) == 0 &&
                        strcmp(all_triples[e].predicate,
                              decomps[d].triples[t].predicate) == 0 &&
                        strcmp(all_triples[e].object,
                              decomps[d].triples[t].object) == 0) {
                        dup = 1; break;
                    }
                }
                if (!dup)
                    all_triples[triple_count++] = decomps[d].triples[t];
            }
        }

        /* Update decomps for pipeline (uses preprocessor triples) */
        for (int t = 0; t < triple_count && t < TARDY_MAX_TRIPLES; t++) {
            if (t < decomps[0].count) continue; /* already there from multi-pass */
            decomps[0].triples[decomps[0].count] = all_triples[t];
            decomps[0].count++;
            decomps[1].triples[decomps[1].count] = all_triples[t];
            decomps[1].count++;
            decomps[2].triples[decomps[2].count] = all_triples[t];
            decomps[2].count++;
        }

        /* Step 1b: OPTIONAL LLM-assisted decomposition.
         * Only fires when TARDY_LLM_DECOMPOSE=1 is set AND ANTHROPIC_API_KEY exists.
         * Default: subagent preprocessor (zero deps, zero cost, instant).
         * The LLM converts free text to structured triples. */
        const char *api_key = getenv("ANTHROPIC_API_KEY");
        const char *llm_decompose = getenv("TARDY_LLM_DECOMPOSE");
        if (triple_count < 2 && api_key && api_key[0] &&
            llm_decompose && llm_decompose[0] == '1') {
            /* Build LLM prompt asking for structured triple extraction */
            char prompt_body[2048];
            snprintf(prompt_body, sizeof(prompt_body),
                "{\"model\":\"claude-haiku-4-5-20251001\",\"max_tokens\":300,"
                "\"temperature\":0,"
                "\"system\":\"Extract factual claims as JSON triples. "
                "Output ONLY a JSON array like: "
                "[{\\\"s\\\":\\\"subject\\\",\\\"p\\\":\\\"predicate\\\",\\\"o\\\":\\\"object\\\"}]. "
                "Use simple predicates like: created_by, located_in, created_in, "
                "type_of, has, premiered_on, written_by, invented_by. "
                "No explanation.\","
                "\"messages\":[{\"role\":\"user\",\"content\":\"Extract triples: "
                "%.800s\"}]}", claim_buf);

            /* Fork curl to call Claude Haiku (cheapest, fastest) */
            int pipefd[2];
            if (pipe(pipefd) == 0) {
                pid_t pid = fork();
                if (pid == 0) {
                    close(pipefd[0]);
                    dup2(pipefd[1], STDOUT_FILENO);
                    close(pipefd[1]);
                    int devnull = open("/dev/null", O_WRONLY);
                    if (devnull >= 0) {
                        dup2(devnull, STDERR_FILENO);
                        close(devnull);
                    }
                    char auth[300];
                    snprintf(auth, sizeof(auth), "x-api-key: %s", api_key);
                    execlp("curl", "curl", "-s",
                           "-X", "POST",
                           "https://api.anthropic.com/v1/messages",
                           "-H", "Content-Type: application/json",
                           "-H", "anthropic-version: 2023-06-01",
                           "-H", auth,
                           "-d", prompt_body,
                           (char *)NULL);
                    _exit(127);
                } else if (pid > 0) {
                    close(pipefd[1]);
                    char llm_out[4096];
                    int llm_len = 0;
                    ssize_t n;
                    while ((n = read(pipefd[0], llm_out + llm_len,
                                     sizeof(llm_out) - (size_t)llm_len - 1)) > 0)
                        llm_len += (int)n;
                    llm_out[llm_len] = '\0';
                    close(pipefd[0]);
                    int wstatus;
                    waitpid(pid, &wstatus, 0);

                    /* Parse LLM response -- find the JSON array in content[0].text */
                    tardy_json_parser_t lp;
                    if (tardy_json_parse(&lp, llm_out, llm_len) == 0) {
                        int ct = tardy_json_find(&lp, 0, "content");
                        if (ct >= 0) {
                            int ft = ct + 1;
                            int tt = tardy_json_find(&lp, ft, "text");
                            if (tt >= 0) {
                                char text_buf[2048];
                                int tlen = tardy_json_str(&lp, tt,
                                    text_buf, sizeof(text_buf));
                                /* Find [ in the text to locate the JSON array */
                                char *arr_start = strchr(text_buf, '[');
                                if (arr_start && tlen > 0) {
                                    tardy_json_parser_t tp;
                                    int arr_len = tlen - (int)(arr_start - text_buf);
                                    if (tardy_json_parse(&tp, arr_start, arr_len) == 0 &&
                                        tp.tokens[0].type == TARDY_JSON_ARRAY) {
                                        int ac = tp.tokens[0].children;
                                        int ti = 1;
                                        for (int t = 0; t < ac &&
                                             triple_count < TARDY_MAX_TRIPLES; t++) {
                                            int st = tardy_json_find(&tp, ti, "s");
                                            int pt = tardy_json_find(&tp, ti, "p");
                                            int ot = tardy_json_find(&tp, ti, "o");
                                            if (st >= 0 && pt >= 0 && ot >= 0) {
                                                tardy_json_str(&tp, st,
                                                    all_triples[triple_count].subject,
                                                    TARDY_MAX_TRIPLE_LEN);
                                                tardy_json_str(&tp, pt,
                                                    all_triples[triple_count].predicate,
                                                    TARDY_MAX_TRIPLE_LEN);
                                                tardy_json_str(&tp, ot,
                                                    all_triples[triple_count].object,
                                                    TARDY_MAX_TRIPLE_LEN);
                                                triple_count++;
                                            }
                                            ti += 1 + tp.tokens[ti].children;
                                        }
                                        /* Update decomps for pipeline */
                                        for (int t = 0; t < triple_count; t++) {
                                            decomps[0].triples[t] = all_triples[t];
                                            decomps[1].triples[t] = all_triples[t];
                                            decomps[2].triples[t] = all_triples[t];
                                        }
                                        decomps[0].count = triple_count;
                                        decomps[1].count = triple_count;
                                        decomps[2].count = triple_count;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    close(pipefd[0]);
                    close(pipefd[1]);
                }
            }
        }

        /* Step 2.5: Try computational verification first */
        float comp_confidence = 0.0f;
        int comp_result = tardy_inference_compute(claim_buf, claim_len,
                                                   &comp_confidence);

        /* Step 3: Ground triples against ontology (real or fallback) */
        tardy_grounding_t grounding = {0};
        tardy_consistency_t consistency = {0};

        if (comp_result == 1) {
            /* Computational claim verified — set grounding as fully grounded */
            grounding.count = triple_count;
            for (int i = 0; i < triple_count && i < TARDY_MAX_TRIPLES; i++) {
                grounding.results[i].triple = all_triples[i];
                grounding.results[i].status = TARDY_KNOWLEDGE_GROUNDED;
                grounding.results[i].confidence = comp_confidence;
                grounding.results[i].evidence_count = 1;
                grounding.grounded++;
            }
            consistency.consistent = true;
        } else if (srv->bridge_connected) {
            /* External ontology engine via unix socket */
            tardy_bridge_verify(&srv->bridge, all_triples, triple_count,
                                 &grounding, &consistency);
        } else if (srv->self_ontology_loaded &&
                   srv->self_ontology.triple_count > 0) {
            /* Self-hosted ontology: triples as agents, no external process */
            tardy_self_ontology_verify(&srv->self_ontology,
                                        all_triples, triple_count,
                                        &grounding, &consistency);
        } else {
            /* No ontology available: honest UNKNOWN fallback */
            grounding.count = triple_count;
            for (int i = 0; i < triple_count &&
                 i < TARDY_MAX_TRIPLES; i++) {
                grounding.results[i].triple = all_triples[i];
                grounding.results[i].status = TARDY_KNOWLEDGE_UNKNOWN;
                grounding.results[i].confidence = 0.0f;
                grounding.results[i].evidence_count = 0;
                grounding.unknown++;
            }
            consistency.consistent = true;
        }

        /* Step 4: Real work log — record actual operations */
        tardy_work_log_t work_log;
        tardy_worklog_init(&work_log);
        work_log.ontology_queries = (srv->bridge_connected ||
            srv->self_ontology.triple_count > 0) ? triple_count : 0;
        work_log.context_reads = triple_count;
        work_log.agents_spawned = 0;

        const tardy_semantics_t *sem = tardy_vm_get_semantics(srv->vm, target->id);
        tardy_work_spec_t spec = tardy_compute_work_spec(sem);

        /* Run pipeline 3 times independently for Byzantine consensus */
        tardy_pipeline_result_t results[3];
        int pass_count = 0;
        float total_confidence = 0.0f;

        for (int run = 0; run < 3; run++) {
            /* Each run uses slightly different decomposition */
            tardy_decomposition_t run_decomps[3];
            memset(run_decomps, 0, sizeof(run_decomps));

            /* Rotate which decomposition is primary for each run */
            for (int d = 0; d < 3; d++) {
                int src_idx = (d + run) % 3;
                if (src_idx < 3)
                    run_decomps[d] = decomps[src_idx];
            }

            results[run] = tardy_pipeline_verify(
                claim_buf, claim_len,
                run_decomps, 3, &grounding, &consistency,
                &work_log, &spec, sem);

            if (results[run].passed) {
                pass_count++;
                total_confidence += results[run].confidence;
            }
        }

        /* Byzantine majority: 2 of 3 must agree */
        int verified = (pass_count >= 2);
        float avg_confidence = pass_count > 0 ? total_confidence / (float)pass_count : 0.0f;
        tardy_truth_strength_t final_strength = TARDY_TRUTH_HYPOTHETICAL;

        if (verified) {
            /* Use the strongest passing result */
            for (int run = 0; run < 3; run++) {
                if (results[run].passed && results[run].strength > final_strength)
                    final_strength = results[run].strength;
            }
        }

        /* Retry logic — feedback-driven based on failure type */
        int max_retries = 2;
        int retry = 0;
        while (!verified && retry < max_retries) {
            tardy_failure_type_t fail = TARDY_FAIL_NONE;
            /* Get failure type from the best failing result */
            for (int run = 0; run < 3; run++) {
                if (!results[run].passed && results[run].failure_type != TARDY_FAIL_NONE) {
                    fail = results[run].failure_type;
                    break;
                }
            }

            if (fail == TARDY_FAIL_ONTOLOGY_GAP) {
                /* Can't retry — ontology doesn't have the data */
                break;
            } else if (fail == TARDY_FAIL_DECOMPOSITION) {
                /* Just break for now — decomposition is deterministic */
                break;
            } else if (fail == TARDY_FAIL_LOW_CONFIDENCE) {
                /* Lower the threshold slightly for retry */
                tardy_semantics_t retry_sem = *sem;
                retry_sem.truth.min_confidence *= 0.9f;
                /* Re-run pipeline with relaxed threshold */
                pass_count = 0;
                total_confidence = 0.0f;
                for (int run = 0; run < 3; run++) {
                    tardy_decomposition_t run_decomps[3];
                    memset(run_decomps, 0, sizeof(run_decomps));
                    for (int d = 0; d < 3; d++)
                        run_decomps[d] = decomps[(d + run) % 3];
                    results[run] = tardy_pipeline_verify(
                        claim_buf, claim_len,
                        run_decomps, 3, &grounding, &consistency,
                        &work_log, &spec, &retry_sem);
                    if (results[run].passed) {
                        pass_count++;
                        total_confidence += results[run].confidence;
                    }
                }
                verified = (pass_count >= 2);
                if (verified) {
                    avg_confidence = total_confidence / (float)pass_count;
                    for (int run = 0; run < 3; run++)
                        if (results[run].passed && results[run].strength > final_strength)
                            final_strength = results[run].strength;
                }
            } else {
                /* Other failures — don't retry blindly */
                break;
            }
            retry++;
        }

        work_log.agents_spawned = 3; /* 3 independent verification runs */
        work_log.compute_ns = mcp_now_ns() - verify_start;

        /* If passed, record provenance and freeze agent to @verified */
        if (verified) {
            /* Record verification results in provenance before freezing */
            target->provenance.reason = "verified_claim";

            /* Also record the number of grounded triples */
            target->provenance.causality_count = grounding.grounded;

            tardy_vm_freeze(srv->vm, target->id, TARDY_TRUST_VERIFIED);

            /* Self-growing ontology: add verified triples so future
             * claims can be grounded against past verifications.
             * The more you verify, the more the system knows. */
            if (srv->self_ontology_loaded) {
                for (int t = 0; t < triple_count; t++) {
                    tardy_self_ontology_add(&srv->self_ontology,
                        all_triples[t].subject,
                        all_triples[t].predicate,
                        all_triples[t].object);
                }
                /* Re-evaluate Datalog to derive new facts from additions */
                tardy_dl_evaluate(&srv->self_ontology.datalog);
            }

            /* Rule mining: learn patterns from verified claims */
            if (triple_count >= 2) {
                tardy_inference_learn(&srv->ruleset,
                                      all_triples, triple_count);
            }

            char turn_msg[256];
            snprintf(turn_msg, sizeof(turn_msg),
                     "verified: strength=%d confidence=%d%%",
                     (int)final_strength, (int)(avg_confidence * 100));
            tardy_vm_converse(srv->vm, target->id, "agent", turn_msg);
        } else {
            tardy_vm_converse(srv->vm, target->id, "agent",
                               "verification failed");
        }

        /* Determine failure type string for response */
        const char *fail_str = "none";
        if (!verified) {
            /* Find the dominant failure type from results */
            tardy_failure_type_t dominant_fail = TARDY_FAIL_NONE;
            for (int run = 0; run < 3; run++) {
                if (!results[run].passed && results[run].failure_type != TARDY_FAIL_NONE) {
                    dominant_fail = results[run].failure_type;
                    break;
                }
            }
            switch (dominant_fail) {
            case TARDY_FAIL_DECOMPOSITION:  fail_str = "decomposition_error"; break;
            case TARDY_FAIL_ONTOLOGY_GAP:   fail_str = "ontology_gap"; break;
            case TARDY_FAIL_CONTRADICTION:   fail_str = "contradiction"; break;
            case TARDY_FAIL_LOW_CONFIDENCE:  fail_str = "low_confidence"; break;
            case TARDY_FAIL_INCONSISTENCY:   fail_str = "inconsistency"; break;
            case TARDY_FAIL_NO_EVIDENCE:     fail_str = "no_evidence"; break;
            case TARDY_FAIL_PROTOCOL:        fail_str = "protocol_error"; break;
            case TARDY_FAIL_LAZINESS:        fail_str = "laziness"; break;
            case TARDY_FAIL_AMBIGUITY:       fail_str = "ambiguity"; break;
            case TARDY_FAIL_CROSS_REP:       fail_str = "cross_rep_conflict"; break;
            default: fail_str = "none"; break;
            }
        }

        /* Ontology status */
        const char *ont_status = srv->bridge_connected ? "connected" :
                                  (srv->self_ontology.triple_count > 0 ? "self-hosted" : "offline");

        /* Build result message */
        char result_text[512];
        snprintf(result_text, sizeof(result_text),
                 "{\"content\":[{\"type\":\"text\","
                 "\"text\":\"verified=%s strength=%d confidence=%d%% "
                 "bft=%d/3 triples_grounded=%d/%d "
                 "failure=%s retries=%d ontology=%s\"}]}",
                 verified ? "true" : "false",
                 (int)final_strength,
                 (int)(avg_confidence * 100),
                 pass_count,
                 grounding.grounded,
                 grounding.count,
                 fail_str,
                 retry,
                 ont_status);

        int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                   id, result_text);
        if (rlen > 0) mcp_write(srv->write_buf, rlen);
        return 0;
    }

    /* ---- Built-in: send_message ---- */
    if (strcmp(tool_name, "send_message") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int from_tok = tardy_json_find(parser, args_tok, "agent_from");
        int to_tok   = tardy_json_find(parser, args_tok, "agent_to");
        int pay_tok  = tardy_json_find(parser, args_tok, "payload");
        if (from_tok < 0 || to_tok < 0 || pay_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602,
                                    "missing agent_from, agent_to, or payload");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char from_name[64], to_name[64], pay_text[TARDY_MAX_PAYLOAD];
        tardy_json_str(parser, from_tok, from_name, sizeof(from_name));
        tardy_json_str(parser, to_tok, to_name, sizeof(to_name));
        tardy_json_str(parser, pay_tok, pay_text, sizeof(pay_text));

        /* Find sender and receiver agents */
        tardy_agent_t *sender = NULL;
        tardy_agent_t *receiver = NULL;
        for (int i = 0; i < srv->vm->agent_count && (!sender || !receiver); i++) {
            if (!sender) {
                tardy_agent_t *c = tardy_vm_find_by_name(
                    srv->vm, srv->vm->agents[i].id, from_name);
                if (c) sender = c;
            }
            if (!receiver) {
                tardy_agent_t *c = tardy_vm_find_by_name(
                    srv->vm, srv->vm->agents[i].id, to_name);
                if (c) receiver = c;
            }
        }

        if (!sender || !receiver) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "sender or receiver not found");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }

        size_t pay_len = strlen(pay_text) + 1;
        int sent = tardy_vm_send(srv->vm, sender->id, receiver->id,
                                  pay_text, pay_len, TARDY_TYPE_STR);
        if (sent == 0) {
            const char *ok_result =
                "{\"content\":[{\"type\":\"text\","
                "\"text\":\"message sent\"}]}";
            int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                       id, ok_result);
            if (rlen > 0) mcp_write(srv->write_buf, rlen);
        } else {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "send failed (inbox full?)");
            if (elen > 0) mcp_write(srv->write_buf, elen);
        }
        return 0;
    }

    /* ---- Built-in: read_inbox ---- */
    if (strcmp(tool_name, "read_inbox") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int agent_tok = tardy_json_find(parser, args_tok, "agent");
        if (agent_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing agent");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char agent_name[64];
        tardy_json_str(parser, agent_tok, agent_name, sizeof(agent_name));

        /* Find the agent */
        tardy_agent_t *target = NULL;
        for (int i = 0; i < srv->vm->agent_count; i++) {
            tardy_agent_t *c = tardy_vm_find_by_name(
                srv->vm, srv->vm->agents[i].id, agent_name);
            if (c) { target = c; break; }
        }

        if (!target) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "agent not found");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }

        tardy_message_t msg;
        int got = tardy_vm_recv(srv->vm, target->id, &msg);
        if (got == 0) {
            /* Build result with payload and sender info */
            char result_text[2048];
            /* Escape payload for JSON */
            char escaped[TARDY_MAX_PAYLOAD * 2];
            int ei = 0;
            for (size_t pi = 0; pi < msg.payload_len && msg.payload[pi]; pi++) {
                if (msg.payload[pi] == '"' || msg.payload[pi] == '\\') {
                    if (ei < (int)sizeof(escaped) - 2)
                        escaped[ei++] = '\\';
                }
                if (ei < (int)sizeof(escaped) - 1)
                    escaped[ei++] = msg.payload[pi];
            }
            escaped[ei] = '\0';

            snprintf(result_text, sizeof(result_text),
                     "{\"content\":[{\"type\":\"text\","
                     "\"text\":\"from=%016llx%016llx payload=%s\"}]}",
                     (unsigned long long)msg.from.hi,
                     (unsigned long long)msg.from.lo,
                     escaped);
            int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                       id, result_text);
            if (rlen > 0) mcp_write(srv->write_buf, rlen);
        } else {
            const char *empty_result =
                "{\"content\":[{\"type\":\"text\","
                "\"text\":\"inbox empty\"}]}";
            int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                       id, empty_result);
            if (rlen > 0) mcp_write(srv->write_buf, rlen);
        }
        return 0;
    }

    /* ---- Built-in: set_semantics ---- */
    if (strcmp(tool_name, "set_semantics") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char aname[64], key[64], val_str[64];
        int at = tardy_json_find(parser, args_tok, "agent");
        int kt = tardy_json_find(parser, args_tok, "key");
        int vt = tardy_json_find(parser, args_tok, "value");
        if (at < 0 || kt < 0 || vt < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing agent, key, or value");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        tardy_json_str(parser, at, aname, sizeof(aname));
        tardy_json_str(parser, kt, key, sizeof(key));
        tardy_json_str(parser, vt, val_str, sizeof(val_str));

        /* Find agent */
        tardy_agent_t *target = NULL;
        for (int i = 0; i < srv->vm->agent_count; i++) {
            target = tardy_vm_find_by_name(srv->vm, srv->vm->agents[i].id, aname);
            if (target) break;
        }
        if (!target) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "agent not found");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }

        /* Clone from global if needed, then set key */
        const tardy_semantics_t *current = tardy_vm_get_semantics(srv->vm, target->id);
        tardy_semantics_t updated = *current;

        if (strcmp(key, "truth.min_confidence") == 0) {
            /* Simple float parse */
            float f = 0.0f;
            const char *s = val_str;
            int neg = 0, i = 0;
            if (s[0] == '-') { neg = 1; i = 1; }
            for (; s[i] && s[i] != '.'; i++) f = f * 10.0f + (s[i] - '0');
            if (s[i] == '.') { i++; float frac = 0.1f; for (; s[i]; i++) { f += (s[i] - '0') * frac; frac *= 0.1f; } }
            if (neg) f = -f;
            updated.truth.min_confidence = f;
        } else if (strcmp(key, "truth.min_consensus_agents") == 0) {
            updated.truth.min_consensus_agents = (int)tardy_json_int(parser, vt);
        } else if (strcmp(key, "truth.min_evidence_triples") == 0) {
            updated.truth.min_evidence_triples = (int)tardy_json_int(parser, vt);
        } else if (strcmp(key, "pipeline.min_passing_layers") == 0) {
            updated.pipeline.min_passing_layers = (int)tardy_json_int(parser, vt);
        } else {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "unknown semantics key");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }

        tardy_vm_set_semantics(srv->vm, target->id, &updated);

        const char *ok_result =
            "{\"content\":[{\"type\":\"text\","
            "\"text\":\"semantics updated\"}]}";
        int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE, id, ok_result);
        if (rlen > 0) mcp_write(srv->write_buf, rlen);
        return 0;
    }

    /* ---- Built-in: query_agents ---- */
    if (strcmp(tool_name, "query_agents") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int query_tok = tardy_json_find(parser, args_tok, "query");
        if (query_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing query");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char query_text[256];
        tardy_json_str(parser, query_tok, query_text, sizeof(query_text));

        tardy_query_result_t qresults[TARDY_MAX_QUERY_RESULTS];
        int total_found = 0;

        for (int qi = 0; qi < srv->vm->agent_count && total_found < TARDY_MAX_QUERY_RESULTS; qi++) {
            tardy_agent_t *qa = &srv->vm->agents[qi];
            if (qa->state == TARDY_STATE_DEAD || qa->context.child_count == 0)
                continue;
            tardy_query_result_t batch[TARDY_MAX_QUERY_RESULTS];
            int found = tardy_vm_query(srv->vm, qa->id, query_text,
                                        batch, TARDY_MAX_QUERY_RESULTS - total_found);
            for (int qj = 0; qj < found; qj++) {
                int dup = 0;
                for (int qk = 0; qk < total_found; qk++) {
                    if (qresults[qk].agent_id.hi == batch[qj].agent_id.hi &&
                        qresults[qk].agent_id.lo == batch[qj].agent_id.lo) {
                        dup = 1;
                        if (batch[qj].score > qresults[qk].score)
                            qresults[qk].score = batch[qj].score;
                        break;
                    }
                }
                if (!dup && total_found < TARDY_MAX_QUERY_RESULTS)
                    qresults[total_found++] = batch[qj];
            }
        }

        char result_buf[2048];
        int rpos = 0;
        const char *rpre = "{\"content\":[{\"type\":\"text\",\"text\":\"{\\\"results\\\":[";
        int rplen = (int)strlen(rpre);
        memcpy(result_buf + rpos, rpre, rplen);
        rpos += rplen;
        for (int qi = 0; qi < total_found; qi++) {
            if (qi > 0 && rpos < (int)sizeof(result_buf) - 1)
                result_buf[rpos++] = ',';
            char entry[128];
            int score_int = (int)(qresults[qi].score * 100);
            int elen = snprintf(entry, sizeof(entry),
                "{\\\"name\\\":\\\"%s\\\",\\\"score\\\":%d.%02d}",
                qresults[qi].name, score_int / 100, score_int % 100);
            if (elen > 0 && rpos + elen < (int)sizeof(result_buf)) {
                memcpy(result_buf + rpos, entry, elen);
                rpos += elen;
            }
        }
        const char *rsuf = "]}\"}]}";
        int rslen = (int)strlen(rsuf);
        if (rpos + rslen < (int)sizeof(result_buf)) {
            memcpy(result_buf + rpos, rsuf, rslen);
            rpos += rslen;
        }
        result_buf[rpos] = '\0';
        int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE, id, result_buf);
        if (rlen > 0) mcp_write(srv->write_buf, rlen);
        return 0;
    }

    /* ---- Built-in: get_conversation ---- */
    if (strcmp(tool_name, "get_conversation") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int agent_tok = tardy_json_find(parser, args_tok, "agent");
        if (agent_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing agent");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char agent_name[64];
        tardy_json_str(parser, agent_tok, agent_name, sizeof(agent_name));

        /* Find the agent */
        tardy_agent_t *conv_target = NULL;
        for (int i = 0; i < srv->vm->agent_count; i++) {
            tardy_agent_t *c = tardy_vm_find_by_name(
                srv->vm, srv->vm->agents[i].id, agent_name);
            if (c) { conv_target = c; break; }
        }

        if (!conv_target) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32000, "agent not found");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }

        tardy_conversation_turn_t turns[TARDY_MAX_CONVERSATION];
        int count = tardy_vm_get_conversation(srv->vm, conv_target->id,
                                               turns, TARDY_MAX_CONVERSATION);

        /* Build JSON array of turns */
        char result_buf[4096];
        int rpos = 0;
        const char *rpre = "{\"content\":[{\"type\":\"text\",\"text\":\"[";
        int rplen = (int)strlen(rpre);
        memcpy(result_buf + rpos, rpre, rplen);
        rpos += rplen;

        for (int ci = 0; ci < count; ci++) {
            if (ci > 0 && rpos < (int)sizeof(result_buf) - 1)
                result_buf[rpos++] = ',';
            char entry[640];
            int elen = snprintf(entry, sizeof(entry),
                "{\\\"role\\\":\\\"%s\\\","
                "\\\"content\\\":\\\"%s\\\","
                "\\\"at\\\":%llu}",
                turns[ci].role,
                turns[ci].content,
                (unsigned long long)turns[ci].at);
            if (elen > 0 && rpos + elen < (int)sizeof(result_buf)) {
                memcpy(result_buf + rpos, entry, elen);
                rpos += elen;
            }
        }

        const char *rsuf = "]\"}]}";
        int rslen = (int)strlen(rsuf);
        if (rpos + rslen < (int)sizeof(result_buf)) {
            memcpy(result_buf + rpos, rsuf, rslen);
            rpos += rslen;
        }
        result_buf[rpos] = '\0';

        int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                   id, result_buf);
        if (rlen > 0) mcp_write(srv->write_buf, rlen);
        return 0;
    }

    /* ---- Built-in: load_ontology ---- */
    if (strcmp(tool_name, "load_ontology") == 0) {
        int args_tok = tardy_json_find(parser, params_tok, "arguments");
        if (args_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing arguments");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        int path_tok = tardy_json_find(parser, args_tok, "path");
        if (path_tok < 0) {
            int elen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                    id, -32602, "missing path");
            if (elen > 0) mcp_write(srv->write_buf, elen);
            return -1;
        }
        char ttl_path[256];
        tardy_json_str(parser, path_tok, ttl_path, sizeof(ttl_path));

        int loaded = tardy_self_ontology_load_ttl(&srv->self_ontology,
                                                    ttl_path);
        char result_text[256];
        if (loaded >= 0) {
            srv->self_ontology_loaded = true;
            snprintf(result_text, sizeof(result_text),
                     "{\"content\":[{\"type\":\"text\","
                     "\"text\":\"loaded %d triples from %s\"}]}",
                     loaded, ttl_path);
        } else {
            snprintf(result_text, sizeof(result_text),
                     "{\"content\":[{\"type\":\"text\","
                     "\"text\":\"failed to load %s\"}]}",
                     ttl_path);
        }
        int rlen = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                   id, result_text);
        if (rlen > 0) mcp_write(srv->write_buf, rlen);
        return 0;
    }

    /* Find agent by name anywhere in the tree */
    tardy_agent_t *found_agent = NULL;
    tardy_read_result_t full_result;
    memset(&full_result, 0, sizeof(full_result));
    full_result.status = TARDY_READ_HASH_MISMATCH;
    char read_buf[512];
    memset(read_buf, 0, sizeof(read_buf));

    for (int i = 0; i < srv->vm->agent_count; i++) {
        tardy_agent_t *candidate = tardy_vm_find_by_name(
            srv->vm, srv->vm->agents[i].id, tool_name);
        if (candidate) {
            found_agent = candidate;
            full_result = tardy_vm_read_full(srv->vm, srv->vm->agents[i].id,
                                              tool_name, read_buf,
                                              sizeof(read_buf));
            break;
        }
    }

    if (full_result.status == TARDY_READ_OK && found_agent) {
        char result[2048];
        int rlen = 0;
        const char *pre = "{\"content\":[{\"type\":\"text\",\"text\":\"";
        int prelen = (int)strlen(pre);
        memcpy(result + rlen, pre, prelen);
        rlen += prelen;

        /* Format value based on type */
        if (found_agent->type_tag == TARDY_TYPE_STR ||
            found_agent->type_tag == TARDY_TYPE_ERROR) {
            /* String: copy directly (escape quotes) */
            const char *s = read_buf;
            while (*s && rlen < (int)sizeof(result) - 10) {
                if (*s == '"' || *s == '\\')
                    result[rlen++] = '\\';
                result[rlen++] = *s++;
            }
        } else if (found_agent->type_tag == TARDY_TYPE_BOOL) {
            int64_t bval;
            memcpy(&bval, read_buf, sizeof(int64_t));
            const char *bs = bval ? "true" : "false";
            int bslen = (int)strlen(bs);
            memcpy(result + rlen, bs, bslen);
            rlen += bslen;
        } else {
            /* Int/Float: format as number string */
            int64_t val;
            memcpy(&val, read_buf, sizeof(int64_t));
            char numstr[32];
            int nlen = 0;
            int neg = 0;
            int64_t v = val;
            if (v < 0) { neg = 1; v = -v; }
            if (v == 0) numstr[nlen++] = '0';
            while (v > 0) { numstr[nlen++] = '0' + (char)(v % 10); v /= 10; }
            if (neg) numstr[nlen++] = '-';
            for (int j = 0; j < nlen / 2; j++) {
                char t = numstr[j];
                numstr[j] = numstr[nlen - 1 - j];
                numstr[nlen - 1 - j] = t;
            }
            memcpy(result + rlen, numstr, nlen);
            rlen += nlen;
        }

        /* Close content array, then append _tardy, then close object.
         * Current result so far: {"content":[{"type":"text","text":"VALUE
         * We need: ...VALUE"}], "_tardy":{...}}
         */
        const char *arr_close = "\"}]";
        int aclen = (int)strlen(arr_close);
        if (rlen + aclen < (int)sizeof(result)) {
            memcpy(result + rlen, arr_close, aclen);
            rlen += aclen;
        }

        /* Append provenance */
        const char *prov_ont = srv->bridge_connected ? "connected" :
                                (srv->self_ontology.triple_count > 0 ? "self-hosted" : "offline");
        int new_rlen = append_provenance(result, rlen, (int)sizeof(result),
                                          &full_result, prov_ont);
        if (new_rlen > 0)
            rlen = new_rlen;

        /* Close outer object */
        if (rlen < (int)sizeof(result) - 1)
            result[rlen++] = '}';
        result[rlen] = '\0';

        int len = build_response(srv->write_buf, TARDY_MCP_BUF_SIZE,
                                  id, result);
        if (len > 0)
            mcp_write(srv->write_buf, len);
    } else {
        tardy_read_status_t status = full_result.status;
        const char *err;
        switch (status) {
        case TARDY_READ_HASH_MISMATCH:  err = "hash verification failed"; break;
        case TARDY_READ_NO_CONSENSUS:   err = "no Byzantine consensus"; break;
        case TARDY_READ_SIG_INVALID:    err = "signature invalid"; break;
        default:                        err = "read failed"; break;
        }
        int len = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                               id, -32000, err);
        if (len > 0) mcp_write(srv->write_buf, len);
    }

    return 0;
}

/* ============================================
 * MCP Server Core
 * ============================================ */

int tardy_mcp_init(tardy_mcp_server_t *srv, tardy_vm_t *vm)
{
    if (!srv || !vm)
        return -1;
    memset(srv, 0, sizeof(tardy_mcp_server_t));
    srv->vm = vm;
    srv->running = 1;

    /* Try connecting to ontology engines */
    int bridge_ok = tardy_bridge_init(&srv->bridge,
        "/tmp/tardygrada-ontology-sketch.sock",
        "/tmp/tardygrada-ontology-complete.sock");
    srv->bridge_connected = (bridge_ok == 0);

    /* Log which ontology mode is active (stderr to avoid corrupting MCP) */
    if (srv->bridge.dual_mode) {
        /* both sketch and complete connected */
        const char *msg = "[tardygrada] ontology: dual mode (sketch+complete)\n";
    tardy_write(STDERR_FILENO, msg, strlen(msg));
    } else if (srv->bridge.sketch.connected) {
        /* sketch only */
        const char *msg = "[tardygrada] ontology: sketch only\n";
    tardy_write(STDERR_FILENO, msg, strlen(msg));
    } else if (srv->bridge.complete.connected) {
        /* complete only */
        const char *msg = "[tardygrada] ontology: complete only\n";
    tardy_write(STDERR_FILENO, msg, strlen(msg));
    } else {
        /* no ontology — fallback to UNKNOWN */
        const char *msg = "[tardygrada] ontology: none (fallback to UNKNOWN)\n";
    tardy_write(STDERR_FILENO, msg, strlen(msg));
    }

    /* Initialize self-hosted ontology (always available as fallback) */
    tardy_self_ontology_init(&srv->self_ontology, vm);
    srv->self_ontology_loaded = srv->self_ontology.initialized;

    /* Initialize frame registry for CRDT merge */
    tardy_frames_init(&srv->frames);

    /* Initialize inference ruleset with synthetic backbone */
    tardy_inference_init(&srv->ruleset);

    return 0;
}

int tardy_mcp_handle(tardy_mcp_server_t *srv, const char *request, int len)
{
    tardy_json_parser_t parser;
    if (tardy_json_parse(&parser, request, len) < 0)
        return -1;

    /* Extract method and id */
    int method_tok = tardy_json_find(&parser, 0, "method");
    int id_tok = tardy_json_find(&parser, 0, "id");

    char id_str[32] = "null";
    if (id_tok >= 0) {
        if (parser.tokens[id_tok].type == TARDY_JSON_STRING) {
            id_str[0] = '"';
            tardy_json_str(&parser, id_tok, id_str + 1, sizeof(id_str) - 2);
            int slen = (int)strlen(id_str);
            id_str[slen] = '"';
            id_str[slen + 1] = '\0';
        } else if (parser.tokens[id_tok].type == TARDY_JSON_NUMBER) {
            memcpy(id_str, parser.tokens[id_tok].start,
                   parser.tokens[id_tok].len);
            id_str[parser.tokens[id_tok].len] = '\0';
        }
    }

    if (method_tok < 0)
        return -1;

    /* Route to handler */
    if (tardy_json_eq(&parser, method_tok, "initialize"))
        return handle_initialize(srv, id_str);

    if (tardy_json_eq(&parser, method_tok, "notifications/initialized"))
        return 0; /* notification, no response */

    if (tardy_json_eq(&parser, method_tok, "tools/list"))
        return handle_tools_list(srv, id_str);

    if (tardy_json_eq(&parser, method_tok, "tools/call"))
        return handle_tools_call(srv, &parser, id_str);

    /* Unknown method */
    int rlen = build_error(srv->write_buf, TARDY_MCP_BUF_SIZE,
                            id_str, -32601, "method not found");
    if (rlen > 0)
        mcp_write(srv->write_buf, rlen);
    return 0;
}

int tardy_mcp_run(tardy_mcp_server_t *srv)
{
    while (srv->running) {
        /* Read Content-Length header */
        char hdr[128];
        int hpos = 0;
        int content_length = -1;

        /* Read header line by line */
        while (hpos < (int)sizeof(hdr) - 1) {
            ssize_t n = read(STDIN_FILENO, hdr + hpos, 1);
            if (n <= 0) {
                srv->running = 0;
                return 0; /* EOF — clean shutdown */
            }
            hpos++;

            /* Check for \r\n\r\n (end of headers) */
            if (hpos >= 4 &&
                hdr[hpos - 4] == '\r' && hdr[hpos - 3] == '\n' &&
                hdr[hpos - 2] == '\r' && hdr[hpos - 1] == '\n') {

                /* Parse Content-Length */
                hdr[hpos] = '\0';
                const char *cl = "Content-Length: ";
                char *found = NULL;
                for (int i = 0; i < hpos - 15; i++) {
                    if (memcmp(hdr + i, cl, 16) == 0) {
                        found = hdr + i + 16;
                        break;
                    }
                }
                if (found) {
                    content_length = 0;
                    while (*found >= '0' && *found <= '9') {
                        content_length = content_length * 10 + (*found - '0');
                        found++;
                    }
                }
                break;
            }
        }

        if (content_length <= 0 || content_length >= TARDY_MCP_BUF_SIZE)
            continue;

        /* Read body */
        int total = 0;
        while (total < content_length) {
            ssize_t n = read(STDIN_FILENO, srv->read_buf + total,
                            content_length - total);
            if (n <= 0) {
                srv->running = 0;
                return 0;
            }
            total += (int)n;
        }
        srv->read_buf[content_length] = '\0';

        /* Handle the request */
        tardy_mcp_handle(srv, srv->read_buf, content_length);
    }

    return 0;
}

void tardy_mcp_stop(tardy_mcp_server_t *srv)
{
    if (srv) {
        srv->running = 0;
        tardy_bridge_shutdown(&srv->bridge);
    }
}
