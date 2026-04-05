/*
 * Tardygrada VM — Context Pointers
 * Not memory pointers. Context pointers.
 * Every dereference is a verified access to an agent.
 */

#ifndef TARDY_CONTEXT_H
#define TARDY_CONTEXT_H

#include "types.h"
#include "memory.h"
#include "constitution.h"
#include "message.h"
#include "semantics.h"

/* ============================================
 * Context Pointer — the fundamental reference
 *
 * In C: int *x = &val;   // address of a byte
 * In Tardygrada: ctx_ptr x = &agent; // address of an agent
 *
 * The pointer itself carries metadata.
 * Two CPU instructions for default trust.
 * ============================================ */

typedef struct {
    tardy_agent_memory_t *memory;      /* where the agent lives */
    tardy_uuid_t          agent_id;    /* who this agent is */
    tardy_trust_t         trust;       /* what level of verification on read */
    tardy_type_t          type_tag;    /* what type this points to */
    tardy_uuid_t          owner_id;    /* which parent agent owns this */
} tardy_ctx_ptr_t;

/* ============================================
 * Provenance — who made this, when, why, from what
 * ============================================ */

typedef struct tardy_provenance {
    tardy_uuid_t          created_by;   /* parent agent ID */
    tardy_timestamp_t     created_at;
    const char           *reason;       /* "let binding", "llm response", etc */
    tardy_uuid_t         *causality;    /* agents that contributed to this value */
    int                   causality_count;
    tardy_hash_t          birth_hash;   /* hash of value at creation */
} tardy_provenance_t;

/* ============================================
 * Mutation Record — for mutable agents
 * ============================================ */

typedef struct {
    tardy_hash_t      from_hash;    /* hash of old value */
    tardy_hash_t      to_hash;      /* hash of new value */
    tardy_timestamp_t at;
    tardy_uuid_t      by;           /* who mutated */
    const char       *reason;
} tardy_mutation_t;

/* ============================================
 * Agent Context — the searchable namespace
 * A parent agent's context is like a vector DB.
 * Children live here. Named lookup + semantic query.
 * ============================================ */

#define TARDY_CTX_MAX_CHILDREN 256
#define TARDY_CTX_MAX_NAME     64

typedef struct {
    char           name[TARDY_CTX_MAX_NAME];
    tardy_uuid_t   agent_id;
} tardy_named_child_t;

typedef struct {
    tardy_named_child_t children[TARDY_CTX_MAX_CHILDREN];
    int                 child_count;
    tardy_message_queue_t inbox;  /* messages from other agents */
    /* Semantic query uses keyword matching (see semantic.c).
     * Vector embeddings can be added here for O(log n) lookups. */
} tardy_agent_context_t;

/* ============================================
 * Agent Snapshot — lightweight dump for Static state
 * ~100 bytes. Kept when agent demotes from Live.
 * ============================================ */

typedef struct {
    tardy_uuid_t      original_id;
    tardy_hash_t      value_hash;
    tardy_uuid_t      created_by;
    tardy_timestamp_t created_at;
    tardy_trust_t     trust;
    tardy_type_t      type_tag;
} tardy_snapshot_t;

/* ============================================
 * Tombstone — remains after agent death
 * The agent is gone but proof of existence survives.
 * ============================================ */

typedef struct {
    tardy_uuid_t      id;
    tardy_type_t      was_type;
    tardy_hash_t      birth_hash;
    tardy_hash_t      final_hash;
    tardy_timestamp_t died_at;
} tardy_tombstone_t;

/* ============================================
 * Conversation History — agent body is a conversation
 * ============================================ */

#define TARDY_MAX_CONVERSATION 32

typedef struct {
    char              role[16];     /* "system", "user", "agent" */
    char              content[512];
    tardy_timestamp_t at;
} tardy_conversation_turn_t;

/* ============================================
 * The Agent — the fundamental unit of everything
 * ============================================ */

typedef struct tardy_agent {
    tardy_uuid_t           id;
    tardy_state_t          state;
    tardy_type_t           type_tag;
    tardy_trust_t          trust;

    /* Live state */
    tardy_agent_memory_t   memory;
    tardy_provenance_t     provenance;
    tardy_agent_context_t  context;

    /* Mutable: mutation log */
    tardy_mutation_t      *mutations;
    int                    mutation_count;
    int                    mutation_cap;

    /* Data size tracking */
    size_t                 data_size;     /* size of stored value in bytes */

    /* Static state (when demoted) */
    int64_t                static_value;  /* the raw value (int/float/bool) */
    char                   static_str[256]; /* string value when static */
    tardy_snapshot_t       snapshot;

    /* GC tracking */
    uint64_t               ref_count;
    tardy_timestamp_t      last_accessed;

    /* Constitution — invariants checked every operation */
    tardy_constitution_t   constitution;

    /* Per-agent semantics override (NULL = use VM global) */
    tardy_semantics_t     *custom_semantics;

    /* Temp state */
    uint64_t               temp_ttl_ms;

    /* Conversation history — agent body is a conversation */
    tardy_conversation_turn_t conversation[TARDY_MAX_CONVERSATION];
    int                       conversation_count;
} tardy_agent_t;

/* ============================================
 * Context Pointer Operations
 * ============================================ */

/* Create a context pointer to an agent */
tardy_ctx_ptr_t tardy_ctx_create(tardy_agent_t *agent, tardy_uuid_t owner);

/* Dereference — read value with trust-level verification */
tardy_read_status_t tardy_ctx_deref(const tardy_ctx_ptr_t *ptr,
                                     void *out, size_t len);

/* Check type compatibility (compile-time in real language, runtime in VM) */
bool tardy_ctx_type_check(const tardy_ctx_ptr_t *ptr, tardy_type_t expected);

/* Check ontology compatibility (context matching) */
bool tardy_ctx_ontology_check(const tardy_ctx_ptr_t *ptr,
                               tardy_uuid_t accessor_ontology);

#endif /* TARDY_CONTEXT_H */
