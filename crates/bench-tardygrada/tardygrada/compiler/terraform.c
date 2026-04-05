/*
 * Tardygrada — Terraform/Fork Implementation
 *
 * Forks a .tardy file: compile, verify trust, execute under parent.
 */

#include "terraform.h"
#include "vm/util.h"
#include "exec.h"
#include <string.h>
#include <unistd.h>

static void tf_print(const char *s)
{
tardy_write(STDERR_FILENO, s, strlen(s));
}

int tardy_fork(tardy_vm_t *vm, tardy_uuid_t parent_id,
                const char *path, const char *alias)
{
    if (!vm || !path)
        return -1;

    tf_print("[tardygrada] fork \"");
    tf_print(path);
    tf_print("\"");
    if (alias) {
        tf_print(" as ");
        tf_print(alias);
    }
    tf_print("\n");

    /* Get parent's trust level — forked code can't exceed it */
    tardy_agent_t *parent = tardy_vm_find(vm, parent_id);
    if (!parent)
        return -1;
    tardy_trust_t max_trust = parent->trust;

    /* Compile the forked file */
    tardy_program_t prog;
    if (tardy_compile_file(&prog, path) != 0) {
        tf_print("[tardygrada] fork compile error: ");
        tf_print(prog.error);
        tf_print("\n");
        return -1;
    }

    /* Check: no instruction escalates trust beyond parent */
    for (int i = 0; i < prog.count; i++) {
        if (prog.instructions[i].trust > max_trust) {
            tf_print("[tardygrada] fork rejected: trust escalation\n");
            return -1;
        }
    }

    /* Execute the forked program under parent_id (not root).
     * Save and restore root_id to run under parent context. */
    tardy_uuid_t saved_root = vm->root_id;
    vm->root_id = parent_id;

    int result = tardy_exec(vm, &prog);

    vm->root_id = saved_root;

    if (result == 0) {
        tf_print("[tardygrada] fork complete\n");
    }

    (void)alias; /* alias reserved for future name-resolution */

    return result;
}
