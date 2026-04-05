/*
 * Tardygrada — Compiler
 *
 * Parses .tardy source and emits VM operations.
 * The compiler produces a sequence of spawn instructions
 * that the VM executes to create the agent society.
 *
 * Syntax:
 *   agent Name {
 *       let x: int = 5 @verified
 *       y: str = "hello"
 *   }
 */

#ifndef TARDY_COMPILER_H
#define TARDY_COMPILER_H

#include "lexer.h"
#include "../vm/types.h"

#define TARDY_MAX_INSTRUCTIONS 256

/* ============================================
 * VM Instructions — what the compiler emits
 * ============================================ */

typedef enum {
    OP_SPAWN_AGENT,    /* create a named agent (parent context) */
    OP_SPAWN_VALUE,    /* create a value agent in current agent */
    OP_RECEIVE,        /* receive() — spawn pending agent, filled via MCP */
    OP_EXEC,           /* exec() — fork/exec shell command, capture stdout */
    OP_FREEZE,         /* freeze mutable agent to immutable with trust level */
    OP_FORK,           /* fork a .tardy file into current context */
    OP_SET_SEMANTICS,  /* set per-agent semantics override */
    OP_COORDINATE,     /* coordinate [agents] on(task) consensus(method) */
    OP_ADD_INVARIANT,  /* add invariant to agent's constitution */
    OP_HALT,           /* end of program */
} tardy_opcode_t;

typedef struct {
    tardy_opcode_t opcode;
    char           name[64];
    tardy_type_t   type;
    tardy_trust_t  trust;
    int64_t        int_val;
    double         float_val;
    char           str_val[256];   /* also used as prompt for OP_RECEIVE */
    bool           bool_val;
    char           ontology[128];  /* grounded_in() ontology ref */
    bool           grounded;       /* whether grounded_in() was specified */
    /* @semantics() key-value pairs */
    char           sem_key[64];
    char           sem_value[64];
    /* coordinate */
    char           coord_agents[256]; /* comma-separated agent names */
    char           coord_task[256];   /* task description */
    /* invariant */
    int            invariant_type;    /* 0=type_check, 1=range, 2=non_empty, 3=trust_min */
    int64_t        inv_min;
    int64_t        inv_max;
    tardy_trust_t  inv_trust;
} tardy_instruction_t;

typedef struct {
    tardy_instruction_t instructions[TARDY_MAX_INSTRUCTIONS];
    int                 count;
    char                agent_name[64];  /* top-level agent name */
    char                error[256];
    bool                has_error;
} tardy_program_t;

/* ============================================
 * Compiler API
 * ============================================ */

/* Compile .tardy source into a program */
int tardy_compile(tardy_program_t *prog, const char *src, int len);

/* Compile from a file path */
int tardy_compile_file(tardy_program_t *prog, const char *path);

#endif /* TARDY_COMPILER_H */
