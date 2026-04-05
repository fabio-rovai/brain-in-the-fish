/*
 * Tardygrada VM — Implementation
 * The root agent. The incorruptible judge. Pure C.
 */

#include "vm.h"
#include "heal.h"
#include "constitution.h"
#include "persist.h"
#include <string.h>
#include <stdint.h>
#include <time.h>
#include <sys/mman.h>

/* Forward declarations from context.c */
extern tardy_uuid_t tardy_uuid_gen(void);
extern bool tardy_uuid_eq(const tardy_uuid_t *a, const tardy_uuid_t *b);

static tardy_timestamp_t now_ns_vm(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static const tardy_uuid_t ZERO_UUID = {0, 0};

/* ============================================
 * VM Init / Shutdown
 * ============================================ */

int tardy_vm_init(tardy_vm_t *vm, const tardy_semantics_t *semantics)
{
    if (!vm)
        return -1;

    memset(vm, 0, sizeof(tardy_vm_t));

    /* Apply semantics (or defaults) */
    if (semantics)
        vm->semantics = *semantics;
    else
        vm->semantics = TARDY_DEFAULT_SEMANTICS;

    /* Generate root identity */
    vm->root_id = tardy_uuid_gen();
    tardy_keygen(&vm->root_key);

    /* Create root agent — the VM itself is agent 0 */
    tardy_agent_t *root = &vm->agents[0];
    root->id        = vm->root_id;
    root->state     = TARDY_STATE_LIVE;
    root->type_tag  = TARDY_TYPE_AGENT;
    root->trust     = TARDY_TRUST_SOVEREIGN;
    root->ref_count = 1; /* self-reference, never GC'd */
    root->last_accessed = now_ns_vm();
    tardy_constitution_init(&root->constitution);
    vm->agent_count = 1;

    /* Initialize root agent's inbox */
    tardy_mq_init(&root->context.inbox);

    vm->boot_time = now_ns_vm();
    vm->running   = true;

    return 0;
}

void tardy_vm_shutdown(tardy_vm_t *vm)
{
    if (!vm)
        return;

    vm->running = false;

    /* Free all agent memory (skip root at index 0) */
    for (int i = 1; i < vm->agent_count; i++) {
        tardy_agent_t *a = &vm->agents[i];

        /* If this is a TARDY_TYPE_AGENT with a non-zero int64 value,
         * it holds a child VM pointer — shut it down and munmap it */
        if (a->type_tag == TARDY_TYPE_AGENT &&
            a->state == TARDY_STATE_LIVE) {
            int64_t child_ptr = 0;
            tardy_page_read(&a->memory.primary, &child_ptr,
                            sizeof(int64_t));
            if (child_ptr != 0) {
                tardy_vm_t *child = (tardy_vm_t *)(intptr_t)child_ptr;
                tardy_vm_shutdown(child);
                munmap(child, sizeof(tardy_vm_t));
            }
        }

        if (a->state == TARDY_STATE_LIVE)
            tardy_mem_free(&a->memory);
        if (a->mutations) {
            /* Mutation log freed by OS on process exit */
        }
        if (a->custom_semantics) {
            munmap(a->custom_semantics, 4096);
            a->custom_semantics = NULL;
        }
    }
}

/* ============================================
 * Agent Lookup
 * ============================================ */

tardy_agent_t *tardy_vm_find(tardy_vm_t *vm, tardy_uuid_t id)
{
    for (int i = 0; i < vm->agent_count; i++) {
        if (tardy_uuid_eq(&vm->agents[i].id, &id) &&
            vm->agents[i].state != TARDY_STATE_DEAD)
            return &vm->agents[i];
    }
    return NULL;
}

tardy_agent_t *tardy_vm_find_by_name(tardy_vm_t *vm,
                                      tardy_uuid_t parent_id,
                                      const char *name)
{
    tardy_uuid_t current = parent_id;
    int depth = 0;
    while (depth < 32) { /* max scope depth to prevent infinite loops */
        tardy_agent_t *parent = tardy_vm_find(vm, current);
        if (!parent)
            break;

        /* Check children of current scope */
        for (int i = 0; i < parent->context.child_count; i++) {
            if (strncmp(parent->context.children[i].name, name,
                        TARDY_CTX_MAX_NAME) == 0) {
                return tardy_vm_find(vm,
                                     parent->context.children[i].agent_id);
            }
        }

        /* Walk up to parent's parent */
        tardy_uuid_t grandparent = parent->provenance.created_by;
        if (tardy_uuid_eq(&grandparent, &current))
            break; /* root agent points to itself — stop */
        if (grandparent.hi == 0 && grandparent.lo == 0)
            break; /* no parent */
        current = grandparent;
        depth++;
    }
    return NULL;
}

/* ============================================
 * Spawn — create a new agent in parent's context
 *
 * This is `let x: int = 5` in Tardygrada.
 * A new agent is born. It holds the value.
 * It lives in the parent's context.
 * ============================================ */

tardy_uuid_t tardy_vm_spawn(tardy_vm_t *vm,
                             tardy_uuid_t parent_id,
                             const char *name,
                             tardy_type_t type,
                             tardy_trust_t trust,
                             const void *data, size_t len)
{
    if (!vm || !vm->running)
        return ZERO_UUID;

    if (vm->agent_count >= TARDY_MAX_AGENTS)
        return ZERO_UUID;

    /* Find parent */
    tardy_agent_t *parent = tardy_vm_find(vm, parent_id);
    if (!parent)
        return ZERO_UUID;

    /* Check parent has room for children */
    if (parent->context.child_count >= TARDY_CTX_MAX_CHILDREN)
        return ZERO_UUID;

    /* Determine replica count from semantics */
    int replicas = 0;
    if (trust == TARDY_TRUST_HARDENED)
        replicas = vm->semantics.immutability.hardened_replica_count;
    else if (trust == TARDY_TRUST_SOVEREIGN)
        replicas = vm->semantics.immutability.sovereign_replica_count;

    /* Allocate the new agent */
    int idx = vm->agent_count;
    tardy_agent_t *agent = &vm->agents[idx];
    memset(agent, 0, sizeof(tardy_agent_t));

    agent->id        = tardy_uuid_gen();
    agent->state     = TARDY_STATE_LIVE;
    agent->type_tag  = type;
    agent->trust     = trust;
    agent->data_size = len;
    agent->ref_count = 1;
    agent->last_accessed = now_ns_vm();

    /* Allocate and initialize memory */
    agent->memory = tardy_mem_alloc(len, trust, replicas);
    tardy_mem_init(&agent->memory, data, len, &vm->root_key);

    /* Initialize inbox */
    tardy_mq_init(&agent->context.inbox);

    /* Inherit parent's constitution */
    agent->constitution = parent->constitution;

    /* Set provenance */
    agent->provenance.created_by = parent_id;
    agent->provenance.created_at = now_ns_vm();
    agent->provenance.reason     = "spawn";
    if (agent->memory.has_hash)
        agent->provenance.birth_hash = agent->memory.birth_hash;

    /* Register in parent's context */
    tardy_named_child_t *child =
        &parent->context.children[parent->context.child_count];
    strncpy(child->name, name, TARDY_CTX_MAX_NAME - 1);
    child->name[TARDY_CTX_MAX_NAME - 1] = '\0';
    child->agent_id = agent->id;
    parent->context.child_count++;

    vm->agent_count++;
    return agent->id;
}

/* ============================================
 * Spawn Error — create an error agent explaining what went wrong
 * Error agents are immutable (TARDY_TRUST_DEFAULT) so they
 * cannot be tampered with. They are children of the parent
 * that caused the error.
 * ============================================ */

tardy_uuid_t tardy_vm_spawn_error(tardy_vm_t *vm,
                                   tardy_uuid_t parent_id,
                                   const char *name,
                                   const char *message)
{
    if (!vm || !message)
        return ZERO_UUID;

    size_t msg_len = strlen(message) + 1;
    tardy_uuid_t err_id = tardy_vm_spawn(vm, parent_id, name,
                                          TARDY_TYPE_ERROR,
                                          TARDY_TRUST_DEFAULT,
                                          message, msg_len);
    if (err_id.hi == 0 && err_id.lo == 0)
        return ZERO_UUID;

    /* Set provenance reason to "error" */
    tardy_agent_t *err_agent = tardy_vm_find(vm, err_id);
    if (err_agent)
        err_agent->provenance.reason = "error";

    return err_id;
}

/* ============================================
 * Kill — destroy a mutable agent
 * Immutable agents CANNOT be killed.
 * ============================================ */

int tardy_vm_kill(tardy_vm_t *vm, tardy_uuid_t agent_id)
{
    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (!agent)
        return -1;

    /* Cannot kill immutable agents */
    if (agent->trust >= TARDY_TRUST_DEFAULT)
        return -1;

    /* Create tombstone */
    if (vm->tombstone_count < TARDY_MAX_TOMBSTONES) {
        tardy_tombstone_t *tomb = &vm->tombstones[vm->tombstone_count++];
        tomb->id         = agent->id;
        tomb->was_type   = agent->type_tag;
        tomb->birth_hash = agent->provenance.birth_hash;
        tomb->died_at    = now_ns_vm();
        /* Compute final hash */
        if (agent->state == TARDY_STATE_LIVE) {
            int64_t val;
            tardy_page_read(&agent->memory.primary, &val, sizeof(int64_t));
            tardy_sha256(&val, sizeof(int64_t), &tomb->final_hash);
        }
    }

    /* Free memory */
    tardy_mem_free(&agent->memory);
    agent->state = TARDY_STATE_DEAD;

    return 0;
}

/* ============================================
 * Read — read an agent's value by name
 * ============================================ */

tardy_read_status_t tardy_vm_read(tardy_vm_t *vm,
                                   tardy_uuid_t parent_id,
                                   const char *name,
                                   void *out, size_t len)
{
    tardy_agent_t *agent = tardy_vm_find_by_name(vm, parent_id, name);
    if (!agent)
        return TARDY_READ_HASH_MISMATCH;

    agent->last_accessed = now_ns_vm();

    /* If static, return the cached value directly */
    if (agent->state == TARDY_STATE_STATIC) {
        if (agent->type_tag == TARDY_TYPE_STR) {
            size_t copy = agent->data_size;
            if (copy > len) copy = len;
            memcpy(out, agent->static_str, copy);
        } else {
            memcpy(out, &agent->static_value,
                   len < sizeof(int64_t) ? len : sizeof(int64_t));
        }
        /* Check constitution before returning */
        if (tardy_constitution_check(&agent->constitution,
                                     agent->type_tag, agent->trust,
                                     out, len) != 0) {
            tardy_vm_spawn_error(vm, parent_id, "_error",
                                  "constitution check failed");
            return TARDY_READ_HASH_MISMATCH;
        }
        return TARDY_READ_OK;
    }

    /* Live/Temp: read using the original data size for correct hash verification */
    size_t read_size = agent->data_size > 0 ? agent->data_size : len;
    if (read_size > len) read_size = len;
    tardy_read_status_t status = tardy_mem_read(&agent->memory, out, read_size);
    if (status != TARDY_READ_OK) {
        /* Try self-heal before giving up */
        if (tardy_heal(vm, agent->id, TARDY_HEAL_REVERIFY) == 0 ||
            tardy_heal(vm, agent->id, TARDY_HEAL_RECONSTRUCT) == 0) {
            /* Retry the read */
            status = tardy_mem_read(&agent->memory, out, read_size);
        }
    }
    if (status != TARDY_READ_OK)
        return status;

    /* Check constitution before returning */
    if (tardy_constitution_check(&agent->constitution,
                                 agent->type_tag, agent->trust,
                                 out, read_size) != 0) {
        tardy_vm_spawn_error(vm, parent_id, "_error",
                              "constitution check failed");
        return TARDY_READ_HASH_MISMATCH;
    }

    return TARDY_READ_OK;
}

/* ============================================
 * Read Full — read value + provenance + proof
 * ============================================ */

tardy_read_result_t tardy_vm_read_full(tardy_vm_t *vm,
                                        tardy_uuid_t parent_id,
                                        const char *name,
                                        void *out, size_t len)
{
    tardy_read_result_t result;
    memset(&result, 0, sizeof(result));

    tardy_agent_t *agent = tardy_vm_find_by_name(vm, parent_id, name);
    if (!agent) {
        result.status = TARDY_READ_HASH_MISMATCH;
        return result;
    }

    /* Perform the normal read */
    result.status = tardy_vm_read(vm, parent_id, name, out, len);

    /* Copy provenance and metadata from the agent */
    result.provenance = agent->provenance;
    result.trust      = agent->trust;
    result.state      = agent->state;
    result.type_tag   = agent->type_tag;
    result.data_size  = agent->data_size;

    /* Determine truth strength from trust level */
    if (agent->trust >= TARDY_TRUST_SOVEREIGN)
        result.strength = TARDY_TRUTH_AXIOMATIC;
    else if (agent->trust >= TARDY_TRUST_HARDENED)
        result.strength = TARDY_TRUTH_PROVEN;
    else if (agent->trust >= TARDY_TRUST_VERIFIED)
        result.strength = TARDY_TRUTH_EVIDENCED;
    else if (agent->trust >= TARDY_TRUST_DEFAULT)
        result.strength = TARDY_TRUTH_ATTESTED;
    else
        result.strength = TARDY_TRUTH_HYPOTHETICAL;

    return result;
}

/* ============================================
 * Mutate — change a mutable agent's value
 * ============================================ */

int tardy_vm_mutate(tardy_vm_t *vm,
                     tardy_uuid_t parent_id,
                     const char *name,
                     const void *data, size_t len)
{
    tardy_agent_t *agent = tardy_vm_find_by_name(vm, parent_id, name);
    if (!agent)
        return -1;

    if (agent->trust != TARDY_TRUST_MUTABLE) {
        tardy_vm_spawn_error(vm, parent_id, "_error",
                              "mutation rejected: agent is immutable");
        return -1;
    }

    /* Check constitution BEFORE committing the mutation */
    if (tardy_constitution_check(&agent->constitution,
                                 agent->type_tag, agent->trust,
                                 data, len) != 0) {
        tardy_vm_spawn_error(vm, parent_id, "_error",
                              "mutation rejected: constitution check failed");
        return -1;
    }

    /* Record mutation in conversation history */
    tardy_vm_converse(vm, agent->id, "system", "value mutated");

    agent->last_accessed = now_ns_vm();
    agent->data_size = len;
    return tardy_mem_mutate(&agent->memory, data, len);
}

/* ============================================
 * Freeze — promote mutable to immutable
 * The critical operation: a pending value becomes a Fact.
 * ============================================ */

tardy_uuid_t tardy_vm_freeze(tardy_vm_t *vm,
                              tardy_uuid_t agent_id,
                              tardy_trust_t new_trust)
{
    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (!agent)
        return ZERO_UUID;

    if (agent->trust != TARDY_TRUST_MUTABLE)
        return ZERO_UUID; /* already immutable */

    if (new_trust < TARDY_TRUST_DEFAULT)
        return ZERO_UUID; /* can't freeze to mutable */

    /* Read current value (use data_size for proper length) */
    size_t dsize = agent->data_size > 0 ? agent->data_size : sizeof(int64_t);
    char current_buf[4096];
    if (dsize > sizeof(current_buf)) dsize = sizeof(current_buf);
    tardy_page_read(&agent->memory.primary, current_buf, dsize);

    /* Free old mutable memory */
    tardy_mem_free(&agent->memory);

    /* Determine replica count */
    int replicas = 0;
    if (new_trust == TARDY_TRUST_HARDENED)
        replicas = vm->semantics.immutability.hardened_replica_count;
    else if (new_trust == TARDY_TRUST_SOVEREIGN)
        replicas = vm->semantics.immutability.sovereign_replica_count;

    /* Reallocate with new trust level */
    agent->memory = tardy_mem_alloc(dsize, new_trust, replicas);
    tardy_mem_init(&agent->memory, current_buf, dsize, &vm->root_key);

    agent->trust = new_trust;
    agent->provenance.reason = "frozen";

    return agent->id;
}

/* ============================================
 * Per-Agent Semantics
 * ============================================ */

const tardy_semantics_t *tardy_vm_get_semantics(tardy_vm_t *vm,
                                                  tardy_uuid_t agent_id)
{
    if (!vm)
        return NULL;
    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (agent && agent->custom_semantics)
        return agent->custom_semantics;
    return &vm->semantics;
}

int tardy_vm_set_semantics(tardy_vm_t *vm, tardy_uuid_t agent_id,
                            const tardy_semantics_t *sem)
{
    if (!vm || !sem)
        return -1;
    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (!agent)
        return -1;
    if (!agent->custom_semantics) {
        void *p = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (p == MAP_FAILED)
            return -1;
        agent->custom_semantics = (tardy_semantics_t *)p;
    }
    memcpy(agent->custom_semantics, sem, sizeof(tardy_semantics_t));
    return 0;
}

/* ============================================
 * GC — Garbage Collection
 * Demote idle agents. Collect dead ones.
 * @sovereign agents: never collected, dump to disk when idle.
 * ============================================ */

int tardy_vm_gc(tardy_vm_t *vm)
{
    if (!vm)
        return -1;

    tardy_timestamp_t now = now_ns_vm();
    uint64_t demotion_threshold_ns =
        (uint64_t)vm->semantics.lifecycle.demotion_idle_ms * 1000000ULL;
    int collected = 0;

    for (int i = 1; i < vm->agent_count; i++) { /* skip root */
        tardy_agent_t *a = &vm->agents[i];

        if (a->state == TARDY_STATE_DEAD)
            continue;

        /* @sovereign: never collect, never demote — dump to disk when idle */
        if (a->trust == TARDY_TRUST_SOVEREIGN) {
            uint64_t idle = now - a->last_accessed;
            uint64_t sov_threshold =
                (uint64_t)vm->semantics.lifecycle.sovereign_dump_idle_ms
                * 1000000ULL;
            if (idle > sov_threshold) {
                tardy_persist_dump(a, TARDY_PERSIST_DIR);
                /* Don't demote — just persist for recovery */
            }
            continue;
        }

        uint64_t idle = now - a->last_accessed;

        /* Demote idle Live agents to Static */
        if (a->state == TARDY_STATE_LIVE && idle > demotion_threshold_ns) {
            tardy_vm_demote(vm, a->id);
            collected++;
            continue;
        }

        /* Expire Temp agents back to Static */
        if (a->state == TARDY_STATE_TEMP) {
            uint64_t ttl_ns = a->temp_ttl_ms * 1000000ULL;
            if (idle > ttl_ns) {
                tardy_vm_demote(vm, a->id);
                collected++;
                continue;
            }
        }

        /* Collect unreferenced Static agents */
        if (a->state == TARDY_STATE_STATIC && a->ref_count == 0) {
            /* Create tombstone */
            if (vm->tombstone_count < TARDY_MAX_TOMBSTONES) {
                tardy_tombstone_t *tomb =
                    &vm->tombstones[vm->tombstone_count++];
                tomb->id         = a->id;
                tomb->was_type   = a->type_tag;
                tomb->birth_hash = a->snapshot.value_hash;
                tomb->died_at    = now;
                tardy_sha256(&a->static_value, sizeof(int64_t),
                             &tomb->final_hash);
            }
            a->state = TARDY_STATE_DEAD;
            collected++;
        }
    }

    return collected;
}

/* ============================================
 * Demote — Live agent becomes Static
 * Free the heavy memory. Keep just the value + snapshot.
 * ============================================ */

int tardy_vm_demote(tardy_vm_t *vm, tardy_uuid_t agent_id)
{
    tardy_agent_t *a = tardy_vm_find(vm, agent_id);
    if (!a || a->state != TARDY_STATE_LIVE)
        return -1;

    /* Cannot demote @sovereign */
    if (a->trust == TARDY_TRUST_SOVEREIGN)
        return -1;

    /* Save the raw value */
    tardy_page_read(&a->memory.primary, &a->static_value, sizeof(int64_t));

    /* Create snapshot */
    a->snapshot.original_id = a->id;
    a->snapshot.created_by  = a->provenance.created_by;
    a->snapshot.created_at  = a->provenance.created_at;
    a->snapshot.trust       = a->trust;
    a->snapshot.type_tag    = a->type_tag;
    tardy_sha256(&a->static_value, sizeof(int64_t), &a->snapshot.value_hash);

    /* Free the heavy memory */
    tardy_mem_free(&a->memory);

    a->state = TARDY_STATE_STATIC;
    return 0;
}

/* ============================================
 * Promote — Static agent becomes Temp
 * Resurrect with a fresh agent memory.
 * ============================================ */

int tardy_vm_promote(tardy_vm_t *vm, tardy_uuid_t agent_id)
{
    tardy_agent_t *a = tardy_vm_find(vm, agent_id);
    if (!a || a->state != TARDY_STATE_STATIC)
        return -1;

    /* Verify the static value hasn't been corrupted */
    tardy_hash_t check;
    tardy_sha256(&a->static_value, sizeof(int64_t), &check);
    if (!tardy_hash_eq(&check, &a->snapshot.value_hash))
        return -1; /* corruption detected */

    /* Reallocate as mutable temp agent */
    a->memory = tardy_mem_alloc(sizeof(int64_t), TARDY_TRUST_MUTABLE, 0);
    tardy_mem_init(&a->memory, &a->static_value, sizeof(int64_t), NULL);

    a->state        = TARDY_STATE_TEMP;
    a->temp_ttl_ms  = vm->semantics.lifecycle.temp_ttl_ms;
    a->last_accessed = now_ns_vm();

    return 0;
}

/* ============================================
 * Send — deliver a message to another agent's inbox
 * ============================================ */

int tardy_vm_send(tardy_vm_t *vm, tardy_uuid_t from, tardy_uuid_t to,
                   const void *payload, size_t len, tardy_type_t type)
{
    if (!vm || !payload)
        return -1;

    if (len > TARDY_MAX_PAYLOAD)
        return -1;

    /* Verify sender exists */
    tardy_agent_t *sender = tardy_vm_find(vm, from);
    if (!sender)
        return -1;

    /* Find target agent */
    tardy_agent_t *target = tardy_vm_find(vm, to);
    if (!target)
        return -1;

    /* Build the message */
    tardy_message_t msg;
    memset(&msg, 0, sizeof(msg));
    msg.from         = from;
    msg.to           = to;
    msg.payload_type = type;
    memcpy(msg.payload, payload, len);
    msg.payload_len  = len;
    msg.sent_at      = now_ns_vm();

    /* Hash the payload for integrity */
    tardy_sha256(payload, len, &msg.hash);

    /* Push to target's inbox */
    return tardy_mq_push(&target->context.inbox, &msg);
}

/* ============================================
 * Recv — pop the next message from an agent's inbox
 * ============================================ */

int tardy_vm_recv(tardy_vm_t *vm, tardy_uuid_t agent_id,
                   tardy_message_t *out)
{
    if (!vm || !out)
        return -1;

    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (!agent)
        return -1;

    return tardy_mq_pop(&agent->context.inbox, out);
}

/* ============================================
 * Agent Conversation
 * ============================================ */

int tardy_vm_converse(tardy_vm_t *vm, tardy_uuid_t agent_id,
                       const char *role, const char *content)
{
    tardy_agent_t *a = tardy_vm_find(vm, agent_id);
    if (!a || a->conversation_count >= TARDY_MAX_CONVERSATION)
        return -1;
    tardy_conversation_turn_t *turn = &a->conversation[a->conversation_count++];
    strncpy(turn->role, role, sizeof(turn->role) - 1);
    turn->role[sizeof(turn->role) - 1] = '\0';
    strncpy(turn->content, content, sizeof(turn->content) - 1);
    turn->content[sizeof(turn->content) - 1] = '\0';
    turn->at = now_ns_vm();
    return 0;
}

int tardy_vm_get_conversation(tardy_vm_t *vm, tardy_uuid_t agent_id,
                               tardy_conversation_turn_t *out, int max_turns)
{
    tardy_agent_t *a = tardy_vm_find(vm, agent_id);
    if (!a) return 0;
    int count = a->conversation_count < max_turns ? a->conversation_count : max_turns;
    memcpy(out, a->conversation, (size_t)count * sizeof(tardy_conversation_turn_t));
    return count;
}

/* ============================================
 * VM Nesting — child VMs as agents
 * ============================================ */

tardy_uuid_t tardy_vm_spawn_child(tardy_vm_t *parent_vm,
                                   tardy_uuid_t parent_agent,
                                   const char *name,
                                   const tardy_semantics_t *child_semantics)
{
    if (!parent_vm || !parent_vm->running || !name)
        return ZERO_UUID;

    /* Allocate child VM via mmap */
    tardy_vm_t *child = (tardy_vm_t *)mmap(NULL, sizeof(tardy_vm_t),
                                            PROT_READ | PROT_WRITE,
                                            MAP_PRIVATE | MAP_ANONYMOUS,
                                            -1, 0);
    if (child == MAP_FAILED)
        return ZERO_UUID;

    /* Initialize with child_semantics or inherit parent's */
    const tardy_semantics_t *sem = child_semantics
        ? child_semantics
        : &parent_vm->semantics;

    if (tardy_vm_init(child, sem) != 0) {
        munmap(child, sizeof(tardy_vm_t));
        return ZERO_UUID;
    }

    /* Inherit parent's root key */
    child->root_key = parent_vm->root_key;

    /* Store child VM pointer as int64 in parent agent */
    int64_t ptr_val = (int64_t)(intptr_t)child;
    tardy_uuid_t agent_id = tardy_vm_spawn(parent_vm, parent_agent, name,
                                            TARDY_TYPE_AGENT,
                                            TARDY_TRUST_DEFAULT,
                                            &ptr_val, sizeof(int64_t));

    if (agent_id.hi == 0 && agent_id.lo == 0) {
        tardy_vm_shutdown(child);
        munmap(child, sizeof(tardy_vm_t));
        return ZERO_UUID;
    }

    return agent_id;
}

tardy_vm_t *tardy_vm_get_child(tardy_vm_t *vm, tardy_uuid_t agent_id)
{
    if (!vm)
        return NULL;

    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (!agent)
        return NULL;

    if (agent->type_tag != TARDY_TYPE_AGENT)
        return NULL;

    if (agent->state != TARDY_STATE_LIVE)
        return NULL;

    int64_t ptr_val = 0;
    tardy_page_read(&agent->memory.primary, &ptr_val, sizeof(int64_t));

    if (ptr_val == 0)
        return NULL;

    return (tardy_vm_t *)(intptr_t)ptr_val;
}
