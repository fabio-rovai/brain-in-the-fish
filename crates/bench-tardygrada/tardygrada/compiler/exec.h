/*
 * Tardygrada — Executor
 * Takes a compiled program and runs it on the VM.
 * Spawns agents, sets values, starts MCP server.
 */

#ifndef TARDY_EXEC_H
#define TARDY_EXEC_H

#include "compiler.h"
#include "../vm/vm.h"
#include "../mcp/server.h"

/* Execute a compiled program on the VM.
 * Spawns all agents, then starts MCP server.
 */
int tardy_exec(tardy_vm_t *vm, const tardy_program_t *prog);

/* Execute and serve — compile file, run on VM, start MCP server */
int tardy_exec_file(const char *path);

#endif /* TARDY_EXEC_H */
