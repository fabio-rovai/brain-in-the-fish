/*
 * Tardygrada — Executor Implementation
 *
 * Walks the instruction list, spawns agents on the VM.
 * Then starts the MCP server so the world can connect.
 */

#include "exec.h"
#include "vm/util.h"
#include "terraform.h"
#include "../verify/pipeline.h"
#include "../coordinate/bridge.h"
#include <sys/mman.h>
#include <sys/wait.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static void exec_print(const char *s)
{
tardy_write(STDERR_FILENO, s, strlen(s));
}

int tardy_exec(tardy_vm_t *vm, const tardy_program_t *prog)
{
    if (!vm || !prog)
        return -1;

    tardy_uuid_t current_agent = vm->root_id;

    for (int i = 0; i < prog->count; i++) {
        const tardy_instruction_t *inst = &prog->instructions[i];

        switch (inst->opcode) {
        case OP_SPAWN_AGENT: {
            /* Create a named agent in root context */
            int64_t zero = 0;
            tardy_uuid_t id = tardy_vm_spawn(vm, vm->root_id,
                                              inst->name,
                                              TARDY_TYPE_AGENT,
                                              inst->trust,
                                              &zero, sizeof(int64_t));
            tardy_agent_t *agent = tardy_vm_find(vm, id);
            if (agent)
                current_agent = id;

            exec_print("[tardygrada] agent ");
            exec_print(inst->name);
            exec_print(" spawned\n");
            break;
        }

        case OP_SPAWN_VALUE: {
            /* Create a value agent in current agent's context */
            switch (inst->type) {
            case TARDY_TYPE_INT: {
                int64_t val = inst->int_val;
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_INT, inst->trust,
                              &val, sizeof(int64_t));
                break;
            }
            case TARDY_TYPE_FLOAT: {
                double val = inst->float_val;
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_FLOAT, inst->trust,
                              &val, sizeof(double));
                break;
            }
            case TARDY_TYPE_STR: {
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_STR, inst->trust,
                              inst->str_val, strlen(inst->str_val) + 1);
                break;
            }
            case TARDY_TYPE_BOOL: {
                int64_t val = inst->bool_val ? 1 : 0;
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_BOOL, inst->trust,
                              &val, sizeof(int64_t));
                break;
            }
            default:
                break;
            }

            exec_print("[tardygrada]   ");
            if (inst->trust >= TARDY_TRUST_DEFAULT)
                exec_print("let ");
            exec_print(inst->name);
            exec_print(" = ...");
            if (inst->trust == TARDY_TRUST_VERIFIED)
                exec_print(" @verified");
            else if (inst->trust == TARDY_TRUST_HARDENED)
                exec_print(" @hardened");
            else if (inst->trust == TARDY_TRUST_SOVEREIGN)
                exec_print(" @sovereign");
            exec_print("\n");
            break;
        }

        case OP_RECEIVE: {
            exec_print("[tardygrada]   receive(\"");
            exec_print(inst->str_val);
            exec_print("\")");
            if (inst->grounded) {
                exec_print(" grounded_in(");
                exec_print(inst->ontology);
                exec_print(")");
            }
            exec_print(" -> pending\n");

            /* Spawn empty mutable agent (will be filled via MCP submit_claim) */
            const char *empty = "";
            tardy_vm_spawn(vm, current_agent, inst->name,
                          TARDY_TYPE_STR, TARDY_TRUST_MUTABLE,
                          empty, 1);
            break;
        }

        case OP_EXEC: {
            exec_print("[tardygrada]   exec(\"");
            exec_print(inst->str_val);
            exec_print("\")\n");

            /* Fork and exec the command, capture stdout */
            int pipefd[2];
            if (pipe(pipefd) < 0) {
                exec_print("[tardygrada]     pipe failed\n");
                const char *err = "exec: pipe failed";
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_STR, TARDY_TRUST_MUTABLE,
                              err, strlen(err) + 1);
                break;
            }

            pid_t pid = fork();
            if (pid < 0) {
                close(pipefd[0]);
                close(pipefd[1]);
                const char *err = "exec: fork failed";
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_STR, TARDY_TRUST_MUTABLE,
                              err, strlen(err) + 1);
                break;
            }

            if (pid == 0) {
                /* Child: redirect stdout to pipe, exec shell */
                close(pipefd[0]);
                dup2(pipefd[1], STDOUT_FILENO);
                close(pipefd[1]);
                /* Redirect stderr to /dev/null */
                int devnull = open("/dev/null", O_WRONLY);
                if (devnull >= 0) { dup2(devnull, STDERR_FILENO); close(devnull); }
                execl("/bin/sh", "sh", "-c", inst->str_val, (char *)NULL);
                _exit(127);
            }

            /* Parent: read output from pipe */
            close(pipefd[1]);
            char output[4096];
            int total = 0;
            ssize_t n;
            while ((n = read(pipefd[0], output + total,
                             sizeof(output) - (size_t)total - 1)) > 0)
                total += (int)n;
            output[total] = '\0';
            close(pipefd[0]);

            int wstatus;
            waitpid(pid, &wstatus, 0);
            int exit_code = WIFEXITED(wstatus) ? WEXITSTATUS(wstatus) : -1;

            /* Trim trailing newline */
            while (total > 0 && (output[total - 1] == '\n' ||
                                 output[total - 1] == '\r'))
                output[--total] = '\0';

            /* Log output preview */
            exec_print("[tardygrada]     -> \"");
            {
                size_t preview_len = (total > 0 && total < 80) ? (size_t)total : 80;
                if (preview_len > (size_t)total && total > 0) preview_len = (size_t)total;
                char preview[81];
                memcpy(preview, output, preview_len);
                preview[preview_len] = '\0';
                exec_print(preview);
                if (total > 80) exec_print("...");
            }
            exec_print("\" (exit ");
            {
                char ec[8];
                ec[0] = '0' + (char)(exit_code % 10);
                ec[1] = '\0';
                exec_print(ec);
            }
            exec_print(")\n");

            if (exit_code != 0 || total == 0) {
                /* Command failed — spawn as mutable with error info */
                char err_msg[256];
                int elen = snprintf(err_msg, sizeof(err_msg),
                                    "exec failed (exit %d): %.200s",
                                    exit_code, output);
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_STR, TARDY_TRUST_MUTABLE,
                              err_msg, (size_t)elen + 1);
            } else {
                /* Command succeeded — spawn with output */
                tardy_trust_t trust = inst->grounded ?
                    (inst->trust >= TARDY_TRUST_DEFAULT ?
                     inst->trust : TARDY_TRUST_MUTABLE) :
                    TARDY_TRUST_MUTABLE;
                tardy_vm_spawn(vm, current_agent, inst->name,
                              TARDY_TYPE_STR, trust,
                              output, (size_t)total + 1);
            }

            /* Record in conversation */
            tardy_agent_t *spawned = tardy_vm_find_by_name(
                vm, current_agent, inst->name);
            if (spawned) {
                char conv[512];
                snprintf(conv, sizeof(conv), "exec: %.400s", inst->str_val);
                tardy_vm_converse(vm, spawned->id, "system", conv);
            }

            break;
        }

        case OP_FORK:
            tardy_fork(vm, current_agent, inst->str_val,
                       inst->name[0] ? inst->name : NULL);
            break;

        case OP_FREEZE: {
            exec_print("[tardygrada]   freeze ");
            exec_print(inst->name);
            if (inst->trust == TARDY_TRUST_VERIFIED) exec_print(" @verified");
            else if (inst->trust == TARDY_TRUST_HARDENED) exec_print(" @hardened");
            else if (inst->trust == TARDY_TRUST_SOVEREIGN) exec_print(" @sovereign");
            exec_print("\n");

            tardy_agent_t *target = tardy_vm_find_by_name(vm, current_agent, inst->name);
            if (target) {
                tardy_uuid_t frozen = tardy_vm_freeze(vm, target->id, inst->trust);
                if (frozen.hi == 0 && frozen.lo == 0) {
                    exec_print("[tardygrada]     freeze failed (already immutable?)\n");
                }
            } else {
                exec_print("[tardygrada]     agent not found: ");
                exec_print(inst->name);
                exec_print("\n");
            }
            break;
        }

        case OP_SET_SEMANTICS: {
            exec_print("[tardygrada]   @semantics(");
            exec_print(inst->sem_key);
            exec_print(": ");
            exec_print(inst->sem_value);
            exec_print(")\n");

            /* Apply semantics override to current agent */
            const tardy_semantics_t *current_sem =
                tardy_vm_get_semantics(vm, current_agent);
            tardy_semantics_t updated = *current_sem;

            /* Parse key and set value */
            if (strcmp(inst->sem_key, "truth.min_confidence") == 0) {
                float f = 0.0f;
                const char *s = inst->sem_value;
                int i = 0;
                for (; s[i] && s[i] != '.'; i++) f = f * 10.0f + (s[i] - '0');
                if (s[i] == '.') {
                    i++;
                    float frac = 0.1f;
                    for (; s[i]; i++) { f += (s[i] - '0') * frac; frac *= 0.1f; }
                }
                updated.truth.min_confidence = f;
            } else if (strcmp(inst->sem_key, "truth.min_consensus_agents") == 0) {
                updated.truth.min_consensus_agents = (int)inst->int_val;
                if (updated.truth.min_consensus_agents == 0) {
                    /* Parse from string */
                    int v = 0;
                    const char *s = inst->sem_value;
                    for (int i = 0; s[i]; i++) v = v * 10 + (s[i] - '0');
                    updated.truth.min_consensus_agents = v;
                }
            } else if (strcmp(inst->sem_key, "pipeline.min_passing_layers") == 0) {
                int v = 0;
                const char *s = inst->sem_value;
                for (int i = 0; s[i]; i++) v = v * 10 + (s[i] - '0');
                updated.pipeline.min_passing_layers = v;
            }

            tardy_vm_set_semantics(vm, current_agent, &updated);
            break;
        }

        case OP_COORDINATE: {
            exec_print("[tardygrada]   coordinate {");
            exec_print(inst->coord_agents);
            exec_print("} on(\"");
            exec_print(inst->coord_task);
            exec_print("\")\n");

            /* Parse comma-separated agent names */
            char names[256];
            strncpy(names, inst->coord_agents, sizeof(names) - 1);
            names[sizeof(names) - 1] = '\0';

            tardy_uuid_t agent_ids[16];
            int agent_count = 0;
            char *tok = names;
            char *next;
            while (tok && agent_count < 16) {
                next = strchr(tok, ',');
                if (next) *next = '\0';
                /* Trim whitespace */
                while (*tok == ' ') tok++;
                char *end = tok + strlen(tok) - 1;
                while (end > tok && *end == ' ') *end-- = '\0';

                if (*tok) {
                    tardy_agent_t *a = tardy_vm_find_by_name(vm, current_agent, tok);
                    if (a) {
                        agent_ids[agent_count++] = a->id;
                    } else {
                        exec_print("[tardygrada]     agent not found: ");
                        exec_print(tok);
                        exec_print("\n");
                    }
                }
                tok = next ? next + 1 : NULL;
            }

            if (agent_count < 2) {
                exec_print("[tardygrada]     need at least 2 agents for coordination\n");
                break;
            }

            /* Try brain-in-the-fish coordination engine first */
            tardy_bitf_conn_t bitf;
            int bitf_ok = tardy_bitf_connect(&bitf, TARDY_BITF_SOCKET_PATH);

            if (bitf_ok == 0) {
                exec_print("[tardygrada]     brain-in-the-fish connected\n");

                /* Collect agent name strings */
                char names2[256];
                strncpy(names2, inst->coord_agents, sizeof(names2) - 1);
                names2[sizeof(names2) - 1] = '\0';
                const char *agent_strs[16];
                int asc = 0;
                char *t2 = names2;
                char *n2;
                while (t2 && asc < 16) {
                    n2 = strchr(t2, ',');
                    if (n2) *n2 = '\0';
                    while (*t2 == ' ') t2++;
                    if (*t2) agent_strs[asc++] = t2;
                    t2 = n2 ? n2 + 1 : NULL;
                }

                tardy_bitf_result_t result;
                tardy_bitf_coordinate(&bitf, inst->coord_task,
                                       agent_strs, asc, &result);
                tardy_bitf_disconnect(&bitf);

                if (result.success) {
                    char msg[128];
                    snprintf(msg, sizeof(msg),
                             "[tardygrada]     consensus: confidence=%.0f%% rounds=%d\n",
                             result.confidence * 100, result.rounds);
                    exec_print(msg);
                } else {
                    exec_print("[tardygrada]     coordination failed: ");
                    exec_print(result.error);
                    exec_print("\n");
                }
            } else {
                /* Fallback: send task to agent inboxes */
                for (int ci = 0; ci < agent_count; ci++) {
                    tardy_vm_send(vm, current_agent, agent_ids[ci],
                                   inst->coord_task,
                                   strlen(inst->coord_task) + 1,
                                   TARDY_TYPE_STR);
                }

                exec_print("[tardygrada]     dispatched to ");
                char cntstr[8];
                cntstr[0] = '0' + (char)agent_count;
                cntstr[1] = '\0';
                exec_print(cntstr);
                exec_print(" agents (bitf offline)\n");
            }

            break;
        }

        case OP_ADD_INVARIANT: {
            exec_print("[tardygrada]   invariant(");
            /* Print invariant type */
            switch (inst->invariant_type) {
            case 0: exec_print("type_check"); break;
            case 1: exec_print("range"); break;
            case 2: exec_print("non_empty"); break;
            case 3: exec_print("trust_min"); break;
            default: exec_print("unknown"); break;
            }
            exec_print(")\n");

            /* Add invariant to current agent's constitution */
            tardy_agent_t *agent = tardy_vm_find(vm, current_agent);
            if (agent) {
                tardy_invariant_t inv = {0};
                inv.type = (tardy_invariant_type_t)inst->invariant_type;
                inv.min_val = inst->inv_min;
                inv.max_val = inst->inv_max;
                inv.trust_arg = inst->inv_trust;
                tardy_constitution_add(&agent->constitution, inv);
            }
            break;
        }

        case OP_HALT:
            exec_print("[tardygrada] program loaded\n");
            return 0;
        }
    }

    return 0;
}

int tardy_exec_file(const char *path)
{
    /* Compile */
    tardy_program_t prog;
    if (tardy_compile_file(&prog, path) != 0) {
        exec_print("[tardygrada] compile error: ");
        exec_print(prog.error);
        exec_print("\n");
        return 1;
    }

    /* Create VM */
    tardy_vm_t *vm = (tardy_vm_t *)mmap(NULL, sizeof(tardy_vm_t),
                                         PROT_READ | PROT_WRITE,
                                         MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (vm == MAP_FAILED)
        return 1;

    tardy_vm_init(vm, NULL);

    /* Execute program */
    tardy_exec(vm, &prog);

    /* Start MCP server */
    exec_print("[tardygrada] MCP server starting on stdio\n");
    tardy_mcp_server_t srv;
    tardy_mcp_init(&srv, vm);
    tardy_mcp_run(&srv);

    /* Cleanup */
    tardy_vm_shutdown(vm);
    munmap(vm, sizeof(tardy_vm_t));
    return 0;
}
