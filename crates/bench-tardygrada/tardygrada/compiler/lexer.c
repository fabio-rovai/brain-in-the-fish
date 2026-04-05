/*
 * Tardygrada — Lexer Implementation
 */

#include "lexer.h"
#include <string.h>

static bool is_alpha(char c)
{
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}

static bool is_digit(char c)
{
    return c >= '0' && c <= '9';
}

static bool is_alnum(char c)
{
    return is_alpha(c) || is_digit(c);
}

static char peek(tardy_lexer_t *lex)
{
    if (lex->pos >= lex->src_len)
        return '\0';
    return lex->src[lex->pos];
}

static char advance(tardy_lexer_t *lex)
{
    char c = lex->src[lex->pos++];
    if (c == '\n') {
        lex->line++;
        lex->col = 1;
    } else {
        lex->col++;
    }
    return c;
}

static void skip_whitespace(tardy_lexer_t *lex)
{
    while (lex->pos < lex->src_len) {
        char c = peek(lex);
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
            advance(lex);
        } else if (c == '/' && lex->pos + 1 < lex->src_len &&
                   lex->src[lex->pos + 1] == '/') {
            /* Line comment */
            while (lex->pos < lex->src_len && peek(lex) != '\n')
                advance(lex);
        } else {
            break;
        }
    }
}

static tardy_token_t make_token(tardy_tok_type_t type, const char *text,
                                 int line, int col)
{
    tardy_token_t tok = {0};
    tok.type = type;
    tok.line = line;
    tok.col = col;
    if (text) {
        int len = (int)strlen(text);
        if (len >= TARDY_MAX_TOKEN_LEN)
            len = TARDY_MAX_TOKEN_LEN - 1;
        memcpy(tok.text, text, len);
        tok.text[len] = '\0';
    }
    return tok;
}

static void emit(tardy_lexer_t *lex, tardy_token_t tok)
{
    if (lex->count < TARDY_MAX_TOKENS)
        lex->tokens[lex->count++] = tok;
}

/* Keyword lookup */
static tardy_tok_type_t keyword_type(const char *word)
{
    if (strcmp(word, "agent") == 0)    return TOK_AGENT;
    if (strcmp(word, "let") == 0)      return TOK_LET;
    if (strcmp(word, "fn") == 0)       return TOK_FN;
    if (strcmp(word, "verified") == 0) return TOK_VERIFIED;
    if (strcmp(word, "int") == 0)      return TOK_INT;
    if (strcmp(word, "float") == 0)    return TOK_FLOAT;
    if (strcmp(word, "str") == 0)      return TOK_STR;
    if (strcmp(word, "bool") == 0)     return TOK_BOOL;
    if (strcmp(word, "Fact") == 0)        return TOK_FACT;
    if (strcmp(word, "true") == 0)       return TOK_BOOL_LIT;
    if (strcmp(word, "false") == 0)      return TOK_BOOL_LIT;
    if (strcmp(word, "fork") == 0)        return TOK_FORK;
    if (strcmp(word, "receive") == 0)     return TOK_RECEIVE;
    if (strcmp(word, "exec") == 0)       return TOK_EXEC;
    if (strcmp(word, "grounded_in") == 0) return TOK_GROUNDED_IN;
    if (strcmp(word, "freeze") == 0)      return TOK_FREEZE;
    if (strcmp(word, "coordinate") == 0)  return TOK_COORDINATE;
    if (strcmp(word, "on") == 0)          return TOK_ON;
    if (strcmp(word, "consensus") == 0)   return TOK_CONSENSUS;
    if (strcmp(word, "invariant") == 0)  return TOK_INVARIANT;
    return TOK_IDENT;
}

/* @ annotation lookup */
static tardy_tok_type_t annotation_type(const char *word)
{
    if (strcmp(word, "verified") == 0)   return TOK_AT_VERIFIED;
    if (strcmp(word, "hardened") == 0)   return TOK_AT_HARDENED;
    if (strcmp(word, "sovereign") == 0)  return TOK_AT_SOVEREIGN;
    if (strcmp(word, "semantics") == 0)  return TOK_AT_SEMANTICS;
    return TOK_ERROR;
}

int tardy_lex(tardy_lexer_t *lex, const char *src, int len)
{
    if (!lex || !src)
        return -1;

    memset(lex, 0, sizeof(tardy_lexer_t));
    lex->src = src;
    lex->src_len = len;
    lex->line = 1;
    lex->col = 1;

    while (lex->pos < lex->src_len) {
        skip_whitespace(lex);
        if (lex->pos >= lex->src_len)
            break;

        int line = lex->line;
        int col = lex->col;
        char c = peek(lex);

        /* Single-char tokens */
        if (c == '{') { advance(lex); emit(lex, make_token(TOK_LBRACE, "{", line, col)); continue; }
        if (c == '}') { advance(lex); emit(lex, make_token(TOK_RBRACE, "}", line, col)); continue; }
        if (c == '(') { advance(lex); emit(lex, make_token(TOK_LPAREN, "(", line, col)); continue; }
        if (c == ')') { advance(lex); emit(lex, make_token(TOK_RPAREN, ")", line, col)); continue; }
        if (c == ':') { advance(lex); emit(lex, make_token(TOK_COLON, ":", line, col)); continue; }
        if (c == '=') { advance(lex); emit(lex, make_token(TOK_EQUALS, "=", line, col)); continue; }
        if (c == ',') { advance(lex); emit(lex, make_token(TOK_COMMA, ",", line, col)); continue; }
        if (c == '.') { advance(lex); emit(lex, make_token(TOK_DOT, ".", line, col)); continue; }

        /* @ annotations */
        if (c == '@') {
            advance(lex); /* skip @ */
            char word[TARDY_MAX_TOKEN_LEN];
            int wlen = 0;
            while (lex->pos < lex->src_len && is_alnum(peek(lex)) &&
                   wlen < TARDY_MAX_TOKEN_LEN - 1)
                word[wlen++] = advance(lex);
            word[wlen] = '\0';

            tardy_tok_type_t type = annotation_type(word);
            if (type == TOK_ERROR) {
                /* Unknown annotation — emit as @ + ident */
                emit(lex, make_token(TOK_AT, "@", line, col));
                emit(lex, make_token(TOK_IDENT, word, line, col + 1));
            } else {
                char full[TARDY_MAX_TOKEN_LEN];
                full[0] = '@';
                memcpy(full + 1, word, wlen + 1);
                emit(lex, make_token(type, full, line, col));
            }
            continue;
        }

        /* String literal */
        if (c == '"') {
            advance(lex); /* skip opening " */
            char str[TARDY_MAX_TOKEN_LEN];
            int slen = 0;
            while (lex->pos < lex->src_len && peek(lex) != '"' &&
                   slen < TARDY_MAX_TOKEN_LEN - 1) {
                if (peek(lex) == '\\') {
                    advance(lex); /* skip backslash */
                    if (lex->pos < lex->src_len)
                        str[slen++] = advance(lex);
                } else {
                    str[slen++] = advance(lex);
                }
            }
            str[slen] = '\0';
            if (lex->pos < lex->src_len)
                advance(lex); /* skip closing " */
            emit(lex, make_token(TOK_STR_LIT, str, line, col));
            continue;
        }

        /* Number literal */
        if (is_digit(c) || (c == '-' && lex->pos + 1 < lex->src_len &&
                            is_digit(lex->src[lex->pos + 1]))) {
            char num[TARDY_MAX_TOKEN_LEN];
            int nlen = 0;
            bool is_float = false;

            if (c == '-')
                num[nlen++] = advance(lex);
            while (lex->pos < lex->src_len && is_digit(peek(lex)) &&
                   nlen < TARDY_MAX_TOKEN_LEN - 1)
                num[nlen++] = advance(lex);
            if (lex->pos < lex->src_len && peek(lex) == '.') {
                is_float = true;
                num[nlen++] = advance(lex);
                while (lex->pos < lex->src_len && is_digit(peek(lex)) &&
                       nlen < TARDY_MAX_TOKEN_LEN - 1)
                    num[nlen++] = advance(lex);
            }
            num[nlen] = '\0';

            emit(lex, make_token(is_float ? TOK_FLOAT_LIT : TOK_INT_LIT,
                                 num, line, col));
            continue;
        }

        /* Identifier / keyword */
        if (is_alpha(c)) {
            char word[TARDY_MAX_TOKEN_LEN];
            int wlen = 0;
            while (lex->pos < lex->src_len && is_alnum(peek(lex)) &&
                   wlen < TARDY_MAX_TOKEN_LEN - 1)
                word[wlen++] = advance(lex);
            word[wlen] = '\0';

            tardy_tok_type_t type = keyword_type(word);
            emit(lex, make_token(type, word, line, col));
            continue;
        }

        /* Unknown character — skip */
        advance(lex);
    }

    emit(lex, make_token(TOK_EOF, "", lex->line, lex->col));
    return 0;
}
