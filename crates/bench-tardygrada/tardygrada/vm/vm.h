/*
 * Tardygrada VM — The Agent Society
 * The VM manages all agents. It IS the root agent.
 * It's the incorruptible judge — deterministic C, not an LLM.
 */

#ifndef TARDY_VM_H
#define TARDY_VM_H

#include "types.h"
#include "context.h"
#include "semantics.h"
#include "crypto.h"

/* ============================================
 * VM Configuration
 * ============================================ */

#define TARDY_MAX_AGENTS     65536
#define TARDY_MAX_TOMBSTONES 16384

/* ============================================
 * The VM
 * ============================================ */

typedef struct {
    /* Agent society — all living agents */
    tardy_agent_t      agents[TARDY_MAX_AGENTS];
    int                agent_count;

    /* Graveyard — tombstones of dead agents */
    tardy_tombstone_t  tombstones[TARDY_MAX_TOMBSTONES];
    int                tombstone_count;

    /* Root agent identity */
    tardy_uuid_t       root_id;
    tardy_keypair_t    root_key;  /* VM's signing key */

    /* Semantics — tunable thresholds */
    tardy_semantics_t  semantics;

    /* Monotonic clock base */
    tardy_timestamp_t  boot_time;

    /* Running flag */
    bool               running;
} tardy_vm_t;

/* ============================================
 * VM Lifecycle
 * ============================================ */

/* Initialize the VM — creates root agent, generates keys */
int tardy_vm_init(tardy_vm_t *vm, const tardy_semantics_t *semantics);

/* Shutdown — free all agents, dump sovereigns to disk */
void tardy_vm_shutdown(tardy_vm_t *vm);

/* ============================================
 * Agent Management
 * ============================================ */

/* Spawn a new agent — the core operation
 * parent: who's spawning this agent
 * name: name in parent's context
 * type: what type of value
 * trust: immutability level
 * data/len: initial value
 * Returns agent ID or zero UUID on failure
 */
tardy_uuid_t tardy_vm_spawn(tardy_vm_t *vm,
                             tardy_uuid_t parent_id,
                             const char *name,
                             tardy_type_t type,
                             tardy_trust_t trust,
                             const void *data, size_t len);

/* Spawn an error agent — queryable explanation of what went wrong
 * parent: agent context where the error occurred
 * name: error agent name (e.g. "_error")
 * message: human-readable error description
 * Returns error agent ID or zero UUID on failure
 */
tardy_uuid_t tardy_vm_spawn_error(tardy_vm_t *vm,
                                   tardy_uuid_t parent_id,
                                   const char *name,
                                   const char *message);

/* Kill a mutable agent — immutable agents cannot be killed */
int tardy_vm_kill(tardy_vm_t *vm, tardy_uuid_t agent_id);

/* Read an agent's value by name from parent's context */
tardy_read_status_t tardy_vm_read(tardy_vm_t *vm,
                                   tardy_uuid_t parent_id,
                                   const char *name,
                                   void *out, size_t len);

/* Mutate a mutable agent's value */
int tardy_vm_mutate(tardy_vm_t *vm,
                     tardy_uuid_t parent_id,
                     const char *name,
                     const void *data, size_t len);

/* Freeze a mutable agent into an immutable one */
tardy_uuid_t tardy_vm_freeze(tardy_vm_t *vm,
                              tardy_uuid_t agent_id,
                              tardy_trust_t new_trust);

/* ============================================
 * Agent-to-Agent Messaging
 * ============================================ */

/* Send a message from one agent to another.
 * Finds the target agent, hashes the payload, pushes to target's inbox.
 * Returns 0 on success, -1 on failure.
 */
int tardy_vm_send(tardy_vm_t *vm, tardy_uuid_t from, tardy_uuid_t to,
                   const void *payload, size_t len, tardy_type_t type);

/* Receive the next message from an agent's inbox.
 * Returns 0 on success, -1 if inbox is empty.
 */
int tardy_vm_recv(tardy_vm_t *vm, tardy_uuid_t agent_id,
                   tardy_message_t *out);

/* ============================================
 * Agent Lookup
 * ============================================ */

/* ============================================
 * Full Read Result — value + provenance + proof
 * ============================================ */

typedef struct {
    tardy_read_status_t    status;
    tardy_provenance_t     provenance;
    tardy_trust_t          trust;
    tardy_truth_strength_t strength;
    tardy_state_t          state;
    tardy_type_t           type_tag;
    size_t                 data_size;
} tardy_read_result_t;

/* Read an agent's value and full provenance by name */
tardy_read_result_t tardy_vm_read_full(tardy_vm_t *vm,
                                        tardy_uuid_t parent_id,
                                        const char *name,
                                        void *out, size_t len);

/* ============================================
 * Per-Agent Semantics
 * ============================================ */

const tardy_semantics_t *tardy_vm_get_semantics(tardy_vm_t *vm,
                                                  tardy_uuid_t agent_id);

int tardy_vm_set_semantics(tardy_vm_t *vm, tardy_uuid_t agent_id,
                            const tardy_semantics_t *sem);

/* ============================================
 * Agent Lookup
 * ============================================ */

/* Find agent by ID */
tardy_agent_t *tardy_vm_find(tardy_vm_t *vm, tardy_uuid_t id);

/* Find agent by name in parent's context */
tardy_agent_t *tardy_vm_find_by_name(tardy_vm_t *vm,
                                      tardy_uuid_t parent_id,
                                      const char *name);

/* ============================================
 * Agent Conversation
 * ============================================ */

/* Append a turn to an agent's conversation */
int tardy_vm_converse(tardy_vm_t *vm, tardy_uuid_t agent_id,
                       const char *role, const char *content);

/* Read an agent's conversation history */
int tardy_vm_get_conversation(tardy_vm_t *vm, tardy_uuid_t agent_id,
                               tardy_conversation_turn_t *out, int max_turns);

/* ============================================
 * VM Nesting — child VMs as agents
 * ============================================ */

/* Spawn a child VM as an agent inside parent_vm.
 * The child VM is allocated via mmap, initialized with child_semantics
 * (or parent's semantics if NULL), and inherits the parent's root_key.
 * Returns agent ID in parent VM (zero UUID on failure).
 */
tardy_uuid_t tardy_vm_spawn_child(tardy_vm_t *parent_vm,
                                   tardy_uuid_t parent_agent,
                                   const char *name,
                                   const tardy_semantics_t *child_semantics);

/* Retrieve the child VM pointer from a TARDY_TYPE_AGENT agent.
 * Returns NULL if agent not found or not a child VM.
 */
tardy_vm_t *tardy_vm_get_child(tardy_vm_t *vm, tardy_uuid_t agent_id);

/* ============================================
 * Garbage Collection
 * ============================================ */

/* Run one GC cycle — demote idle agents, collect dead ones */
int tardy_vm_gc(tardy_vm_t *vm);

/* Demote a live agent to static */
int tardy_vm_demote(tardy_vm_t *vm, tardy_uuid_t agent_id);

/* Promote a static agent back to temp */
int tardy_vm_promote(tardy_vm_t *vm, tardy_uuid_t agent_id);

#endif /* TARDY_VM_H */
