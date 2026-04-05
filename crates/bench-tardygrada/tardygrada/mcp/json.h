/*
 * Tardygrada — Minimal JSON Parser
 * No dependencies. Parses MCP protocol messages.
 * Read-only: parses into tokens, doesn't allocate.
 */

#ifndef TARDY_JSON_H
#define TARDY_JSON_H

#include <stddef.h>
#include <stdbool.h>

#define TARDY_JSON_MAX_TOKENS 128

typedef enum {
    TARDY_JSON_STRING,
    TARDY_JSON_NUMBER,
    TARDY_JSON_BOOL,
    TARDY_JSON_NULL,
    TARDY_JSON_OBJECT,
    TARDY_JSON_ARRAY,
} tardy_json_type_t;

typedef struct {
    tardy_json_type_t type;
    const char       *start;  /* pointer into original buffer */
    int               len;
    int               children; /* for objects/arrays: number of child tokens */
} tardy_json_token_t;

typedef struct {
    tardy_json_token_t tokens[TARDY_JSON_MAX_TOKENS];
    int                count;
    const char        *src;
    int                src_len;
    int                pos;
} tardy_json_parser_t;

/* Parse a JSON string. Returns 0 on success, -1 on error. */
int tardy_json_parse(tardy_json_parser_t *p, const char *json, int len);

/* Find a key in an object token. Returns token index or -1. */
int tardy_json_find(const tardy_json_parser_t *p, int object_tok,
                    const char *key);

/* Extract a string value (copies into buf). Returns length or -1. */
int tardy_json_str(const tardy_json_parser_t *p, int tok,
                   char *buf, int buf_size);

/* Extract an integer value. */
long tardy_json_int(const tardy_json_parser_t *p, int tok);

/* Compare a token's string value. */
bool tardy_json_eq(const tardy_json_parser_t *p, int tok,
                   const char *str);

#endif /* TARDY_JSON_H */
