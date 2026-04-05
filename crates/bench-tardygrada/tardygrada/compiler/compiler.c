/*
 * Tardygrada — Compiler Implementation
 *
 * Simple recursive descent parser.
 * Emits spawn instructions for the VM.
 */

#include "compiler.h"
#include <string.h>
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>

/* ============================================
 * Parser State
 * ============================================ */

typedef struct {
    tardy_lexer_t    *lex;
    int               pos;       /* current token index */
    tardy_program_t  *prog;
} tardy_parser_t;

static tardy_token_t *current(tardy_parser_t *p)
{
    if (p->pos >= p->lex->count)
        return &p->lex->tokens[p->lex->count - 1]; /* EOF */
    return &p->lex->tokens[p->pos];
}

static tardy_token_t *advance_tok(tardy_parser_t *p)
{
    tardy_token_t *tok = current(p);
    if (tok->type != TOK_EOF)
        p->pos++;
    return tok;
}

static bool check(tardy_parser_t *p, tardy_tok_type_t type)
{
    return current(p)->type == type;
}

static bool match(tardy_parser_t *p, tardy_tok_type_t type)
{
    if (check(p, type)) {
        advance_tok(p);
        return true;
    }
    return false;
}

static void error(tardy_parser_t *p, const char *msg)
{
    tardy_token_t *tok = current(p);
    snprintf(p->prog->error, sizeof(p->prog->error),
             "line %d col %d: %s (got '%s')",
             tok->line, tok->col, msg, tok->text);
    p->prog->has_error = true;
}

static bool expect(tardy_parser_t *p, tardy_tok_type_t type, const char *msg)
{
    if (check(p, type)) {
        advance_tok(p);
        return true;
    }
    error(p, msg);
    return false;
}

static void emit_inst(tardy_parser_t *p, tardy_instruction_t inst)
{
    if (p->prog->count < TARDY_MAX_INSTRUCTIONS)
        p->prog->instructions[p->prog->count++] = inst;
}

/* ============================================
 * Parse type annotation
 * ============================================ */

static tardy_type_t parse_type(tardy_parser_t *p)
{
    tardy_token_t *tok = current(p);
    tardy_type_t type = TARDY_TYPE_UNIT;

    switch (tok->type) {
    case TOK_INT:   type = TARDY_TYPE_INT;   break;
    case TOK_FLOAT: type = TARDY_TYPE_FLOAT; break;
    case TOK_STR:   type = TARDY_TYPE_STR;   break;
    case TOK_BOOL:  type = TARDY_TYPE_BOOL;  break;
    case TOK_FACT:  type = TARDY_TYPE_FACT;  break;
    default:
        error(p, "expected type (int, float, str, bool, Fact)");
        return TARDY_TYPE_UNIT;
    }

    advance_tok(p);
    return type;
}

/* ============================================
 * Parse trust annotation (@verified, @sovereign, etc.)
 * ============================================ */

static tardy_trust_t parse_trust(tardy_parser_t *p)
{
    if (check(p, TOK_AT_VERIFIED))  { advance_tok(p); return TARDY_TRUST_VERIFIED; }
    if (check(p, TOK_AT_HARDENED))  { advance_tok(p); return TARDY_TRUST_HARDENED; }
    if (check(p, TOK_AT_SOVEREIGN)) { advance_tok(p); return TARDY_TRUST_SOVEREIGN; }
    return TARDY_TRUST_DEFAULT; /* no annotation = default immutable */
}

/* ============================================
 * Parse value binding
 *
 * let x: int = 5 @verified    (immutable)
 * x: int = 5                  (mutable)
 * ============================================ */

static void parse_binding(tardy_parser_t *p, bool immutable)
{
    tardy_instruction_t inst = {0};
    inst.opcode = OP_SPAWN_VALUE;

    /* Name */
    if (!check(p, TOK_IDENT)) {
        error(p, "expected identifier");
        return;
    }
    strncpy(inst.name, current(p)->text, sizeof(inst.name) - 1);
    advance_tok(p);

    /* : type */
    if (!expect(p, TOK_COLON, "expected ':'"))
        return;

    inst.type = parse_type(p);

    /* = value */
    if (!expect(p, TOK_EQUALS, "expected '='"))
        return;

    tardy_token_t *val = current(p);
    switch (val->type) {
    case TOK_INT_LIT:
        inst.int_val = 0;
        {
            const char *s = val->text;
            int neg = 0;
            int i = 0;
            if (s[0] == '-') { neg = 1; i = 1; }
            for (; s[i]; i++)
                inst.int_val = inst.int_val * 10 + (s[i] - '0');
            if (neg) inst.int_val = -inst.int_val;
        }
        advance_tok(p);
        break;

    case TOK_FLOAT_LIT:
        /* Simple float parse */
        {
            const char *s = val->text;
            double v = 0.0;
            int neg = 0;
            int i = 0;
            if (s[0] == '-') { neg = 1; i = 1; }
            for (; s[i] && s[i] != '.'; i++)
                v = v * 10.0 + (s[i] - '0');
            if (s[i] == '.') {
                i++;
                double frac = 0.1;
                for (; s[i]; i++) {
                    v += (s[i] - '0') * frac;
                    frac *= 0.1;
                }
            }
            inst.float_val = neg ? -v : v;
        }
        advance_tok(p);
        break;

    case TOK_STR_LIT:
        strncpy(inst.str_val, val->text, sizeof(inst.str_val) - 1);
        advance_tok(p);
        break;

    case TOK_BOOL_LIT:
        inst.bool_val = (strcmp(val->text, "true") == 0);
        advance_tok(p);
        break;

    case TOK_RECEIVE:
        /* receive("prompt") — pending slot, filled via MCP */
        inst.opcode = OP_RECEIVE;
        advance_tok(p); /* skip 'receive' */
        if (!expect(p, TOK_LPAREN, "expected '(' after receive"))
            return;
        if (!check(p, TOK_STR_LIT)) {
            error(p, "expected string prompt for receive()");
            return;
        }
        strncpy(inst.str_val, current(p)->text, sizeof(inst.str_val) - 1);
        advance_tok(p);
        if (!expect(p, TOK_RPAREN, "expected ')' after prompt"))
            return;

        /* Optional: grounded_in(ontology) */
        if (check(p, TOK_GROUNDED_IN)) {
            advance_tok(p);
            if (!expect(p, TOK_LPAREN, "expected '(' after grounded_in"))
                return;
            if (check(p, TOK_IDENT) || check(p, TOK_STR_LIT)) {
                strncpy(inst.ontology, current(p)->text,
                        sizeof(inst.ontology) - 1);
                advance_tok(p);
            }
            if (!expect(p, TOK_RPAREN, "expected ')' after ontology"))
                return;
            inst.grounded = true;
        }
        break;

    case TOK_EXEC:
        /* exec("command") — fork/exec shell command, capture stdout */
        inst.opcode = OP_EXEC;
        advance_tok(p); /* skip 'exec' */
        if (!expect(p, TOK_LPAREN, "expected '(' after exec"))
            return;
        if (!check(p, TOK_STR_LIT)) {
            error(p, "expected command string for exec()");
            return;
        }
        strncpy(inst.str_val, current(p)->text, sizeof(inst.str_val) - 1);
        advance_tok(p);
        if (!expect(p, TOK_RPAREN, "expected ')' after command"))
            return;
        /* Optional: grounded_in(ontology) */
        if (check(p, TOK_GROUNDED_IN)) {
            advance_tok(p);
            if (!expect(p, TOK_LPAREN, "expected '(' after grounded_in"))
                return;
            if (check(p, TOK_IDENT) || check(p, TOK_STR_LIT)) {
                strncpy(inst.ontology, current(p)->text,
                        sizeof(inst.ontology) - 1);
                advance_tok(p);
            }
            if (!expect(p, TOK_RPAREN, "expected ')' after ontology"))
                return;
            inst.grounded = true;
        }
        break;

    default:
        error(p, "expected value (int, float, string, bool, receive(), exec())");
        return;
    }

    /* Optional trust annotation */
    if (immutable)
        inst.trust = parse_trust(p);
    else
        inst.trust = TARDY_TRUST_MUTABLE;

    emit_inst(p, inst);
}

/* ============================================
 * Parse agent body
 *
 * agent Name {
 *     let x: int = 5
 *     y: str = "hello"
 * }
 * ============================================ */

static void parse_agent(tardy_parser_t *p)
{
    /* agent keyword already consumed */

    /* Name */
    if (!check(p, TOK_IDENT)) {
        error(p, "expected agent name");
        return;
    }

    tardy_instruction_t agent_inst = {0};
    agent_inst.opcode = OP_SPAWN_AGENT;
    strncpy(agent_inst.name, current(p)->text, sizeof(agent_inst.name) - 1);
    strncpy(p->prog->agent_name, current(p)->text,
            sizeof(p->prog->agent_name) - 1);
    advance_tok(p);

    /* Optional trust annotation on the agent itself */
    agent_inst.trust = parse_trust(p);

    emit_inst(p, agent_inst);

    /* Optional @semantics(key: value, ...) block */
    if (check(p, TOK_AT_SEMANTICS)) {
        advance_tok(p); /* skip @semantics */
        if (!expect(p, TOK_LPAREN, "expected '(' after @semantics"))
            return;
        while (!check(p, TOK_RPAREN) && !check(p, TOK_EOF) && !p->prog->has_error) {
            /* Parse key: value pairs */
            tardy_instruction_t sem_inst = {0};
            sem_inst.opcode = OP_SET_SEMANTICS;

            /* Key is like truth.min_confidence — idents + dots */
            char key[64];
            int klen = 0;
            while ((check(p, TOK_IDENT) || check(p, TOK_DOT)) &&
                   klen < 62 && !p->prog->has_error) {
                const char *t = current(p)->text;
                int tlen = (int)strlen(t);
                if (klen + tlen < 63) {
                    memcpy(key + klen, t, tlen);
                    klen += tlen;
                }
                advance_tok(p);
            }
            key[klen] = '\0';
            strncpy(sem_inst.sem_key, key, sizeof(sem_inst.sem_key) - 1);

            if (!expect(p, TOK_COLON, "expected ':' in @semantics"))
                return;

            /* Value: number or ident */
            if (check(p, TOK_INT_LIT) || check(p, TOK_FLOAT_LIT)) {
                strncpy(sem_inst.sem_value, current(p)->text,
                        sizeof(sem_inst.sem_value) - 1);
                advance_tok(p);
            } else {
                error(p, "expected number value in @semantics");
                return;
            }

            emit_inst(p, sem_inst);

            /* Optional comma */
            if (check(p, TOK_COMMA))
                advance_tok(p);
        }
        if (!expect(p, TOK_RPAREN, "expected ')' after @semantics"))
            return;
    }

    /* Body */
    if (!expect(p, TOK_LBRACE, "expected '{'"))
        return;

    while (!check(p, TOK_RBRACE) && !check(p, TOK_EOF) && !p->prog->has_error) {
        if (match(p, TOK_LET)) {
            /* Immutable binding */
            parse_binding(p, true);
        } else if (match(p, TOK_FORK)) {
            /* fork "path/to/module.tardy" as ModuleName */
            tardy_instruction_t finst = {0};
            finst.opcode = OP_FORK;
            if (!check(p, TOK_STR_LIT)) {
                error(p, "expected file path string after fork");
                return;
            }
            strncpy(finst.str_val, current(p)->text,
                    sizeof(finst.str_val) - 1);
            advance_tok(p);
            /* Optional: as Name */
            if (check(p, TOK_IDENT) &&
                strcmp(current(p)->text, "as") == 0) {
                advance_tok(p);
                if (check(p, TOK_IDENT)) {
                    strncpy(finst.name, current(p)->text,
                            sizeof(finst.name) - 1);
                    advance_tok(p);
                }
            }
            emit_inst(p, finst);
        } else if (match(p, TOK_COORDINATE)) {
            /* coordinate [a, b, c] on("task") consensus(ProofWeight) */
            tardy_instruction_t cinst = {0};
            cinst.opcode = OP_COORDINATE;

            /* [agent_list] */
            if (!expect(p, TOK_LBRACE, "expected '[' or '{' after coordinate"))
                return;
            char agents[256];
            int alen = 0;
            while (!check(p, TOK_RBRACE) && !check(p, TOK_EOF)) {
                if (check(p, TOK_IDENT)) {
                    const char *name = current(p)->text;
                    int nlen = (int)strlen(name);
                    if (alen > 0 && alen < 254) agents[alen++] = ',';
                    if (alen + nlen < 255) {
                        memcpy(agents + alen, name, nlen);
                        alen += nlen;
                    }
                }
                advance_tok(p);
                if (check(p, TOK_COMMA)) advance_tok(p);
            }
            agents[alen] = '\0';
            strncpy(cinst.coord_agents, agents, sizeof(cinst.coord_agents) - 1);
            if (!expect(p, TOK_RBRACE, "expected ']' or '}'"))
                return;

            /* on("task description") */
            if (check(p, TOK_ON)) {
                advance_tok(p);
                if (!expect(p, TOK_LPAREN, "expected '(' after on"))
                    return;
                if (check(p, TOK_STR_LIT)) {
                    strncpy(cinst.coord_task, current(p)->text,
                            sizeof(cinst.coord_task) - 1);
                    advance_tok(p);
                }
                if (!expect(p, TOK_RPAREN, "expected ')' after task"))
                    return;
            }

            /* Optional: consensus(method) — just consume for now */
            if (check(p, TOK_CONSENSUS)) {
                advance_tok(p);
                if (check(p, TOK_LPAREN)) {
                    advance_tok(p);
                    if (check(p, TOK_IDENT)) advance_tok(p);
                    if (check(p, TOK_RPAREN)) advance_tok(p);
                }
            }

            emit_inst(p, cinst);
        } else if (check(p, TOK_INVARIANT)) {
            advance_tok(p);
            tardy_instruction_t iinst = {0};
            iinst.opcode = OP_ADD_INVARIANT;

            if (!expect(p, TOK_LPAREN, "expected '(' after invariant"))
                return;

            /* Parse invariant type */
            if (check(p, TOK_IDENT)) {
                const char *itype = current(p)->text;
                if (strcmp(itype, "trust_min") == 0) {
                    iinst.invariant_type = 3; /* TARDY_INVARIANT_TRUST_MIN */
                    advance_tok(p);
                    if (!expect(p, TOK_COLON, "expected ':'"))
                        return;
                    /* Parse trust level */
                    if (check(p, TOK_AT_VERIFIED)) {
                        iinst.inv_trust = TARDY_TRUST_VERIFIED;
                        advance_tok(p);
                    } else if (check(p, TOK_AT_HARDENED)) {
                        iinst.inv_trust = TARDY_TRUST_HARDENED;
                        advance_tok(p);
                    } else if (check(p, TOK_AT_SOVEREIGN)) {
                        iinst.inv_trust = TARDY_TRUST_SOVEREIGN;
                        advance_tok(p);
                    } else {
                        error(p, "expected trust level (@verified, @hardened, @sovereign)");
                        return;
                    }
                } else if (strcmp(itype, "non_empty") == 0) {
                    iinst.invariant_type = 2; /* TARDY_INVARIANT_NON_EMPTY */
                    advance_tok(p);
                } else if (strcmp(itype, "range") == 0) {
                    iinst.invariant_type = 1; /* TARDY_INVARIANT_RANGE */
                    advance_tok(p);
                    if (!expect(p, TOK_COLON, "expected ':'"))
                        return;
                    if (check(p, TOK_INT_LIT)) {
                        /* parse min */
                        iinst.inv_min = 0;
                        const char *s = current(p)->text;
                        for (int ii = 0; s[ii]; ii++)
                            iinst.inv_min = iinst.inv_min * 10 + (s[ii] - '0');
                        advance_tok(p);
                    }
                    if (check(p, TOK_COMMA)) advance_tok(p);
                    if (check(p, TOK_INT_LIT)) {
                        /* parse max */
                        iinst.inv_max = 0;
                        const char *s = current(p)->text;
                        for (int ii = 0; s[ii]; ii++)
                            iinst.inv_max = iinst.inv_max * 10 + (s[ii] - '0');
                        advance_tok(p);
                    }
                } else {
                    error(p, "unknown invariant type");
                    return;
                }
            }

            if (!expect(p, TOK_RPAREN, "expected ')'"))
                return;

            emit_inst(p, iinst);
        } else if (match(p, TOK_FREEZE)) {
            tardy_instruction_t finst = {0};
            finst.opcode = OP_FREEZE;
            if (!check(p, TOK_IDENT)) {
                error(p, "expected agent name after freeze");
                return;
            }
            strncpy(finst.name, current(p)->text, sizeof(finst.name) - 1);
            advance_tok(p);
            finst.trust = parse_trust(p);
            if (finst.trust < TARDY_TRUST_DEFAULT)
                finst.trust = TARDY_TRUST_VERIFIED; /* default freeze to @verified */
            emit_inst(p, finst);
        } else if (check(p, TOK_IDENT)) {
            /* Mutable binding */
            parse_binding(p, false);
        } else {
            error(p, "expected 'let', 'fork', 'coordinate', 'invariant', 'freeze', or identifier");
            return;
        }
    }

    if (!expect(p, TOK_RBRACE, "expected '}'"))
        return;

    /* Emit halt */
    tardy_instruction_t halt = {0};
    halt.opcode = OP_HALT;
    emit_inst(p, halt);
}

/* ============================================
 * Compiler Entry Point
 * ============================================ */

int tardy_compile(tardy_program_t *prog, const char *src, int len)
{
    if (!prog || !src)
        return -1;

    memset(prog, 0, sizeof(tardy_program_t));

    /* Lex */
    tardy_lexer_t lex;
    if (tardy_lex(&lex, src, len) != 0) {
        snprintf(prog->error, sizeof(prog->error), "lexer error");
        prog->has_error = true;
        return -1;
    }

    /* Parse */
    tardy_parser_t parser = {0};
    parser.lex = &lex;
    parser.pos = 0;
    parser.prog = prog;

    while (!check(&parser, TOK_EOF) && !prog->has_error) {
        if (match(&parser, TOK_AGENT)) {
            parse_agent(&parser);
        } else {
            error(&parser, "expected 'agent'");
            break;
        }
    }

    return prog->has_error ? -1 : 0;
}

/* ============================================
 * Compile from file
 * ============================================ */

int tardy_compile_file(tardy_program_t *prog, const char *path)
{
    int fd = open(path, O_RDONLY);
    if (fd < 0) {
        snprintf(prog->error, sizeof(prog->error),
                 "cannot open file: %s", path);
        prog->has_error = true;
        return -1;
    }

    struct stat st;
    if (fstat(fd, &st) < 0) {
        close(fd);
        return -1;
    }

    if (st.st_size > 1024 * 1024) { /* 1MB max source file */
        close(fd);
        snprintf(prog->error, sizeof(prog->error), "file too large");
        prog->has_error = true;
        return -1;
    }

    char buf[1024 * 1024];
    ssize_t n = read(fd, buf, st.st_size);
    close(fd);

    if (n <= 0)
        return -1;

    return tardy_compile(prog, buf, (int)n);
}
