/*
 * Tardygrada — Lexer
 * Tokenizes .tardy source into a token stream.
 *
 * Syntax:
 *   agent Name { ... }
 *   let x: int = 5
 *   let x: int = 5 @verified
 *   let x: int = 5 @hardened
 *   let x: int = 5 @sovereign
 *   x: int = 5              // mutable (no let)
 */

#ifndef TARDY_LEXER_H
#define TARDY_LEXER_H

#include <stdbool.h>

#define TARDY_MAX_TOKENS 1024
#define TARDY_MAX_TOKEN_LEN 256

typedef enum {
    TOK_AGENT,      /* agent */
    TOK_LET,        /* let */
    TOK_FN,         /* fn */
    TOK_VERIFIED,   /* verified */
    TOK_INT,        /* int */
    TOK_FLOAT,      /* float */
    TOK_STR,        /* str */
    TOK_BOOL,       /* bool */
    TOK_FACT,       /* Fact */

    TOK_FORK,       /* fork */
    TOK_RECEIVE,    /* receive */
    TOK_EXEC,       /* exec */
    TOK_GROUNDED_IN, /* grounded_in */
    TOK_FREEZE,     /* freeze */
    TOK_COORDINATE, /* coordinate */
    TOK_ON,         /* on */
    TOK_CONSENSUS,  /* consensus */
    TOK_INVARIANT,  /* invariant */
    TOK_IDENT,      /* identifier */
    TOK_INT_LIT,    /* integer literal */
    TOK_FLOAT_LIT,  /* float literal */
    TOK_STR_LIT,    /* string literal */
    TOK_BOOL_LIT,   /* true/false */

    TOK_LBRACE,     /* { */
    TOK_RBRACE,     /* } */
    TOK_LPAREN,     /* ( */
    TOK_RPAREN,     /* ) */
    TOK_COLON,      /* : */
    TOK_EQUALS,     /* = */
    TOK_COMMA,      /* , */
    TOK_AT,         /* @ */
    TOK_DOT,        /* . */

    TOK_AT_VERIFIED,   /* @verified */
    TOK_AT_HARDENED,   /* @hardened */
    TOK_AT_SOVEREIGN,  /* @sovereign */
    TOK_AT_SEMANTICS,  /* @semantics */

    TOK_EOF,
    TOK_ERROR,
} tardy_tok_type_t;

typedef struct {
    tardy_tok_type_t type;
    char             text[TARDY_MAX_TOKEN_LEN];
    int              line;
    int              col;
} tardy_token_t;

typedef struct {
    const char    *src;
    int            src_len;
    int            pos;
    int            line;
    int            col;
    tardy_token_t  tokens[TARDY_MAX_TOKENS];
    int            count;
} tardy_lexer_t;

/* Tokenize source code. Returns 0 on success. */
int tardy_lex(tardy_lexer_t *lex, const char *src, int len);

#endif /* TARDY_LEXER_H */
