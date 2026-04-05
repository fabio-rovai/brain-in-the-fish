/*
 * Tardygrada — Terraform/Fork Module System
 *
 * No imports. Programs "terraform" (fork) other .tardy files.
 * The fork is compiled and verified independently.
 * A forked module can't escalate trust beyond its parent.
 */

#ifndef TARDY_TERRAFORM_H
#define TARDY_TERRAFORM_H

#include "../vm/vm.h"
#include "compiler.h"

/* Fork a .tardy file into the current VM context.
 * 1. Reads and compiles the file
 * 2. Executes it in the current VM under parent_id
 * 3. Verifies all spawned agents don't exceed parent's trust
 * Returns 0 on success, -1 on failure. */
int tardy_fork(tardy_vm_t *vm, tardy_uuid_t parent_id,
                const char *path, const char *alias);

#endif /* TARDY_TERRAFORM_H */
