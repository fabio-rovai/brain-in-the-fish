/*
 * Tardygrada — Minimal JSON Parser Implementation
 * Zero-copy: tokens point into the original buffer.
 */

#include "json.h"
#include <string.h>

static void skip_ws(tardy_json_parser_t *p)
{
    while (p->pos < p->src_len) {
        char c = p->src[p->pos];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r')
            p->pos++;
        else
            break;
    }
}

static int parse_string(tardy_json_parser_t *p)
{
    if (p->count >= TARDY_JSON_MAX_TOKENS || p->src[p->pos] != '"')
        return -1;

    p->pos++; /* skip opening quote */
    int start = p->pos;

    while (p->pos < p->src_len) {
        if (p->src[p->pos] == '\\') {
            p->pos += 2; /* skip escaped char */
            continue;
        }
        if (p->src[p->pos] == '"')
            break;
        p->pos++;
    }

    if (p->pos >= p->src_len)
        return -1;

    int idx = p->count++;
    p->tokens[idx].type     = TARDY_JSON_STRING;
    p->tokens[idx].start    = p->src + start;
    p->tokens[idx].len      = p->pos - start;
    p->tokens[idx].children = 0;

    p->pos++; /* skip closing quote */
    return idx;
}

static int parse_value(tardy_json_parser_t *p);

static int parse_number(tardy_json_parser_t *p)
{
    if (p->count >= TARDY_JSON_MAX_TOKENS)
        return -1;

    int start = p->pos;
    if (p->src[p->pos] == '-')
        p->pos++;

    while (p->pos < p->src_len &&
           p->src[p->pos] >= '0' && p->src[p->pos] <= '9')
        p->pos++;

    if (p->pos < p->src_len && p->src[p->pos] == '.') {
        p->pos++;
        while (p->pos < p->src_len &&
               p->src[p->pos] >= '0' && p->src[p->pos] <= '9')
            p->pos++;
    }

    int idx = p->count++;
    p->tokens[idx].type     = TARDY_JSON_NUMBER;
    p->tokens[idx].start    = p->src + start;
    p->tokens[idx].len      = p->pos - start;
    p->tokens[idx].children = 0;
    return idx;
}

static int parse_object(tardy_json_parser_t *p)
{
    if (p->count >= TARDY_JSON_MAX_TOKENS || p->src[p->pos] != '{')
        return -1;

    int idx = p->count++;
    p->tokens[idx].type     = TARDY_JSON_OBJECT;
    p->tokens[idx].start    = p->src + p->pos;
    p->tokens[idx].children = 0;

    p->pos++; /* skip { */
    skip_ws(p);

    while (p->pos < p->src_len && p->src[p->pos] != '}') {
        /* Parse key */
        skip_ws(p);
        if (parse_string(p) < 0)
            return -1;

        /* Skip colon */
        skip_ws(p);
        if (p->pos >= p->src_len || p->src[p->pos] != ':')
            return -1;
        p->pos++;

        /* Parse value */
        skip_ws(p);
        if (parse_value(p) < 0)
            return -1;

        p->tokens[idx].children += 2; /* key + value */

        skip_ws(p);
        if (p->pos < p->src_len && p->src[p->pos] == ',')
            p->pos++;
    }

    if (p->pos >= p->src_len)
        return -1;
    p->pos++; /* skip } */

    p->tokens[idx].len = (int)(p->src + p->pos - p->tokens[idx].start);
    return idx;
}

static int parse_array(tardy_json_parser_t *p)
{
    if (p->count >= TARDY_JSON_MAX_TOKENS || p->src[p->pos] != '[')
        return -1;

    int idx = p->count++;
    p->tokens[idx].type     = TARDY_JSON_ARRAY;
    p->tokens[idx].start    = p->src + p->pos;
    p->tokens[idx].children = 0;

    p->pos++; /* skip [ */
    skip_ws(p);

    while (p->pos < p->src_len && p->src[p->pos] != ']') {
        skip_ws(p);
        if (parse_value(p) < 0)
            return -1;
        p->tokens[idx].children++;

        skip_ws(p);
        if (p->pos < p->src_len && p->src[p->pos] == ',')
            p->pos++;
    }

    if (p->pos >= p->src_len)
        return -1;
    p->pos++; /* skip ] */

    p->tokens[idx].len = (int)(p->src + p->pos - p->tokens[idx].start);
    return idx;
}

static int parse_value(tardy_json_parser_t *p)
{
    skip_ws(p);
    if (p->pos >= p->src_len)
        return -1;

    char c = p->src[p->pos];

    if (c == '"')
        return parse_string(p);
    if (c == '{')
        return parse_object(p);
    if (c == '[')
        return parse_array(p);
    if (c == '-' || (c >= '0' && c <= '9'))
        return parse_number(p);

    /* true, false, null */
    if (p->count >= TARDY_JSON_MAX_TOKENS)
        return -1;

    int idx = p->count++;
    if (p->src_len - p->pos >= 4 && memcmp(p->src + p->pos, "true", 4) == 0) {
        p->tokens[idx].type  = TARDY_JSON_BOOL;
        p->tokens[idx].start = p->src + p->pos;
        p->tokens[idx].len   = 4;
        p->pos += 4;
    } else if (p->src_len - p->pos >= 5 &&
               memcmp(p->src + p->pos, "false", 5) == 0) {
        p->tokens[idx].type  = TARDY_JSON_BOOL;
        p->tokens[idx].start = p->src + p->pos;
        p->tokens[idx].len   = 5;
        p->pos += 5;
    } else if (p->src_len - p->pos >= 4 &&
               memcmp(p->src + p->pos, "null", 4) == 0) {
        p->tokens[idx].type  = TARDY_JSON_NULL;
        p->tokens[idx].start = p->src + p->pos;
        p->tokens[idx].len   = 4;
        p->pos += 4;
    } else {
        p->count--;
        return -1;
    }

    p->tokens[idx].children = 0;
    return idx;
}

int tardy_json_parse(tardy_json_parser_t *p, const char *json, int len)
{
    memset(p, 0, sizeof(tardy_json_parser_t));
    p->src     = json;
    p->src_len = len;
    p->pos     = 0;
    p->count   = 0;

    if (parse_value(p) < 0)
        return -1;
    return 0;
}

int tardy_json_find(const tardy_json_parser_t *p, int object_tok,
                    const char *key)
{
    if (object_tok < 0 || object_tok >= p->count)
        return -1;
    if (p->tokens[object_tok].type != TARDY_JSON_OBJECT)
        return -1;

    int keylen = (int)strlen(key);
    int i = object_tok + 1;
    int pairs = p->tokens[object_tok].children / 2;

    for (int pair = 0; pair < pairs; pair++) {
        if (i >= p->count)
            break;
        /* Check key */
        if (p->tokens[i].type == TARDY_JSON_STRING &&
            p->tokens[i].len == keylen &&
            memcmp(p->tokens[i].start, key, keylen) == 0) {
            return i + 1; /* return value token */
        }
        /* Skip key + value (and value's children) */
        i++; /* skip key */
        /* Skip value subtree */
        int skip = 1;
        int j = i;
        while (skip > 0 && j < p->count) {
            if (p->tokens[j].type == TARDY_JSON_OBJECT ||
                p->tokens[j].type == TARDY_JSON_ARRAY)
                skip += p->tokens[j].children;
            skip--;
            j++;
        }
        i = j;
    }
    return -1;
}

int tardy_json_str(const tardy_json_parser_t *p, int tok,
                   char *buf, int buf_size)
{
    if (tok < 0 || tok >= p->count)
        return -1;
    if (p->tokens[tok].type != TARDY_JSON_STRING)
        return -1;

    int copy = p->tokens[tok].len;
    if (copy >= buf_size)
        copy = buf_size - 1;
    memcpy(buf, p->tokens[tok].start, copy);
    buf[copy] = '\0';
    return copy;
}

long tardy_json_int(const tardy_json_parser_t *p, int tok)
{
    if (tok < 0 || tok >= p->count)
        return 0;

    long val = 0;
    int neg = 0;
    const char *s = p->tokens[tok].start;
    int len = p->tokens[tok].len;
    int i = 0;

    if (len > 0 && s[0] == '-') {
        neg = 1;
        i = 1;
    }
    for (; i < len; i++) {
        if (s[i] < '0' || s[i] > '9')
            break;
        val = val * 10 + (s[i] - '0');
    }
    return neg ? -val : val;
}

bool tardy_json_eq(const tardy_json_parser_t *p, int tok,
                   const char *str)
{
    if (tok < 0 || tok >= p->count)
        return false;
    int slen = (int)strlen(str);
    return p->tokens[tok].len == slen &&
           memcmp(p->tokens[tok].start, str, slen) == 0;
}
