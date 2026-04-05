//! Raw FFI bindings to the Tardygrada C VM.
//!
//! These are hand-written from the C headers under `tardygrada/vm/`.
//! Every struct layout must match the C side exactly.

#![allow(non_camel_case_types, non_upper_case_globals, dead_code)]

use std::os::raw::{c_char, c_int, c_void};

// ── tardy_uuid_t ──────────────────────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct tardy_uuid_t {
    pub hi: u64,
    pub lo: u64,
}

impl tardy_uuid_t {
    pub fn is_zero(&self) -> bool {
        self.hi == 0 && self.lo == 0
    }
}

// ── Enums ─────────────────────────────────────────────────────────

pub type tardy_timestamp_t = u64;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum tardy_type_t {
    TARDY_TYPE_INT   = 0x01,
    TARDY_TYPE_FLOAT = 0x02,
    TARDY_TYPE_BOOL  = 0x03,
    TARDY_TYPE_STR   = 0x04,
    TARDY_TYPE_UNIT  = 0x05,
    TARDY_TYPE_FACT  = 0x06,
    TARDY_TYPE_AGENT = 0x07,
    TARDY_TYPE_ERROR = 0x08,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum tardy_trust_t {
    TARDY_TRUST_MUTABLE   = 0x00,
    TARDY_TRUST_DEFAULT   = 0x01,
    TARDY_TRUST_VERIFIED  = 0x02,
    TARDY_TRUST_HARDENED  = 0x03,
    TARDY_TRUST_SOVEREIGN = 0x04,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum tardy_state_t {
    TARDY_STATE_LIVE   = 0x01,
    TARDY_STATE_STATIC = 0x02,
    TARDY_STATE_TEMP   = 0x03,
    TARDY_STATE_DEAD   = 0x04,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum tardy_truth_strength_t {
    TARDY_TRUTH_REFUTED      = 0x00,
    TARDY_TRUTH_CONTESTED    = 0x01,
    TARDY_TRUTH_HYPOTHETICAL = 0x02,
    TARDY_TRUTH_ATTESTED     = 0x03,
    TARDY_TRUTH_EVIDENCED    = 0x04,
    TARDY_TRUTH_PROVEN       = 0x05,
    TARDY_TRUTH_AXIOMATIC    = 0x06,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum tardy_read_status_t {
    TARDY_READ_OK            = 0,
    TARDY_READ_HASH_MISMATCH = 1,
    TARDY_READ_NO_CONSENSUS  = 2,
    TARDY_READ_SIG_INVALID   = 3,
}

// ── Crypto structs ────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_hash_t {
    pub bytes: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_keypair_t {
    pub secret: [u8; 64],
    pub public_key: [u8; 32], // "public" is a Rust keyword
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_signature_t {
    pub bytes: [u8; 64],
}

// ── Memory structs ────────────────────────────────────────────────

#[repr(C)]
pub struct tardy_page_t {
    pub ptr: *mut c_void,
    pub size: usize,
    pub protection: tardy_trust_t,
    pub locked: bool,
}

#[repr(C)]
pub struct tardy_agent_memory_t {
    pub trust: tardy_trust_t,
    pub primary: tardy_page_t,

    // @verified
    pub birth_hash: tardy_hash_t,
    pub has_hash: bool,

    // @hardened
    pub replicas: *mut tardy_page_t,
    pub replica_count: c_int,

    // @sovereign
    pub signature: tardy_signature_t,
    pub has_signature: bool,
    pub signer_pub: [u8; 32],
    pub hash_replicas: *mut tardy_hash_t,
    pub hash_replica_count: c_int,
}

// ── Constitution ──────────────────────────────────────────────────

pub const TARDY_MAX_INVARIANTS: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum tardy_invariant_type_t {
    TARDY_INVARIANT_TYPE_CHECK = 0,
    TARDY_INVARIANT_RANGE      = 1,
    TARDY_INVARIANT_NON_EMPTY  = 2,
    TARDY_INVARIANT_TRUST_MIN  = 3,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_invariant_t {
    pub type_: tardy_invariant_type_t,
    pub min_val: i64,
    pub max_val: i64,
    pub type_arg: tardy_type_t,
    pub trust_arg: tardy_trust_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_constitution_t {
    pub invariants: [tardy_invariant_t; TARDY_MAX_INVARIANTS],
    pub count: c_int,
    pub constitutional_hash: tardy_hash_t,
}

// ── Message ───────────────────────────────────────────────────────

pub const TARDY_MAX_PAYLOAD: usize = 512;
pub const TARDY_MSG_QUEUE_SIZE: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_message_t {
    pub from: tardy_uuid_t,
    pub to: tardy_uuid_t,
    pub payload_type: tardy_type_t,
    pub payload: [c_char; TARDY_MAX_PAYLOAD],
    pub payload_len: usize,
    pub hash: tardy_hash_t,
    pub sent_at: tardy_timestamp_t,
}

#[repr(C)]
pub struct tardy_message_queue_t {
    pub messages: [tardy_message_t; TARDY_MSG_QUEUE_SIZE],
    pub head: c_int,
    pub tail: c_int,
    pub count: c_int,
}

// ── Provenance ────────────────────────────────────────────────────

#[repr(C)]
pub struct tardy_provenance_t {
    pub created_by: tardy_uuid_t,
    pub created_at: tardy_timestamp_t,
    pub reason: *const c_char,
    pub causality: *mut tardy_uuid_t,
    pub causality_count: c_int,
    pub birth_hash: tardy_hash_t,
}

// ── Mutation ──────────────────────────────────────────────────────

#[repr(C)]
pub struct tardy_mutation_t {
    pub from_hash: tardy_hash_t,
    pub to_hash: tardy_hash_t,
    pub at: tardy_timestamp_t,
    pub by: tardy_uuid_t,
    pub reason: *const c_char,
}

// ── Context ───────────────────────────────────────────────────────

pub const TARDY_CTX_MAX_CHILDREN: usize = 256;
pub const TARDY_CTX_MAX_NAME: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_named_child_t {
    pub name: [c_char; TARDY_CTX_MAX_NAME],
    pub agent_id: tardy_uuid_t,
}

#[repr(C)]
pub struct tardy_agent_context_t {
    pub children: [tardy_named_child_t; TARDY_CTX_MAX_CHILDREN],
    pub child_count: c_int,
    pub inbox: tardy_message_queue_t,
}

// ── Snapshot / Tombstone ──────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_snapshot_t {
    pub original_id: tardy_uuid_t,
    pub value_hash: tardy_hash_t,
    pub created_by: tardy_uuid_t,
    pub created_at: tardy_timestamp_t,
    pub trust: tardy_trust_t,
    pub type_tag: tardy_type_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_tombstone_t {
    pub id: tardy_uuid_t,
    pub was_type: tardy_type_t,
    pub birth_hash: tardy_hash_t,
    pub final_hash: tardy_hash_t,
    pub died_at: tardy_timestamp_t,
}

// ── Conversation ──────────────────────────────────────────────────

pub const TARDY_MAX_CONVERSATION: usize = 32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_conversation_turn_t {
    pub role: [c_char; 16],
    pub content: [c_char; 512],
    pub at: tardy_timestamp_t,
}

// ── The Agent ─────────────────────────────────────────────────────

#[repr(C)]
pub struct tardy_agent_t {
    pub id: tardy_uuid_t,
    pub state: tardy_state_t,
    pub type_tag: tardy_type_t,
    pub trust: tardy_trust_t,

    // Live state
    pub memory: tardy_agent_memory_t,
    pub provenance: tardy_provenance_t,
    pub context: tardy_agent_context_t,

    // Mutable: mutation log
    pub mutations: *mut tardy_mutation_t,
    pub mutation_count: c_int,
    pub mutation_cap: c_int,

    // Data size tracking
    pub data_size: usize,

    // Static state
    pub static_value: i64,
    pub static_str: [c_char; 256],
    pub snapshot: tardy_snapshot_t,

    // GC tracking
    pub ref_count: u64,
    pub last_accessed: tardy_timestamp_t,

    // Constitution
    pub constitution: tardy_constitution_t,

    // Per-agent semantics override
    pub custom_semantics: *mut tardy_semantics_t,

    // Temp state
    pub temp_ttl_ms: u64,

    // Conversation history
    pub conversation: [tardy_conversation_turn_t; TARDY_MAX_CONVERSATION],
    pub conversation_count: c_int,
}

// ── Semantics ─────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_truth_semantics_t {
    pub min_evidence_triples: c_int,
    pub max_contradictions: c_int,
    pub min_confidence: f32,
    pub min_consensus_agents: c_int,
    pub min_agreement_ratio: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_hallucination_semantics_t {
    pub grounding_threshold: f32,
    pub min_decomposers: c_int,
    pub min_decomposition_agreement: f32,
    pub require_dual_ontology: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_laziness_semantics_t {
    pub min_observed_operations: c_int,
    pub min_work_authenticity: f32,
    pub max_idle_ms: c_int,
    pub min_impossibility_verifiers: c_int,
    pub max_work_similarity: f32,
    pub max_verification_chain: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_immutability_semantics_t {
    pub hardened_replica_count: c_int,
    pub sovereign_replica_count: c_int,
    pub sovereign_quorum_ratio: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_lifecycle_semantics_t {
    pub demotion_idle_ms: c_int,
    pub temp_ttl_ms: c_int,
    pub sovereign_dump_idle_ms: c_int,
    pub gc_interval_ms: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_pipeline_semantics_t {
    pub layer_ontology_grounding: bool,
    pub layer_consistency_check: bool,
    pub layer_probabilistic_scoring: bool,
    pub layer_protocol_check: bool,
    pub layer_formal_certification: bool,
    pub layer_cross_representation: bool,
    pub layer_work_verification: bool,
    pub min_passing_layers: c_int,
    pub skip_for_literals: bool,
    pub skip_for_arithmetic: bool,
    pub skip_for_internal_routing: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tardy_semantics_t {
    pub truth: tardy_truth_semantics_t,
    pub hallucination: tardy_hallucination_semantics_t,
    pub laziness: tardy_laziness_semantics_t,
    pub immutability: tardy_immutability_semantics_t,
    pub lifecycle: tardy_lifecycle_semantics_t,
    pub pipeline: tardy_pipeline_semantics_t,
}

// ── Read Result ───────────────────────────────────────────────────

#[repr(C)]
pub struct tardy_read_result_t {
    pub status: tardy_read_status_t,
    pub provenance: tardy_provenance_t,
    pub trust: tardy_trust_t,
    pub strength: tardy_truth_strength_t,
    pub state: tardy_state_t,
    pub type_tag: tardy_type_t,
    pub data_size: usize,
}

// ── VM Constants ──────────────────────────────────────────────────

pub const TARDY_MAX_AGENTS: usize = 65536;
pub const TARDY_MAX_TOMBSTONES: usize = 16384;

// ── The VM — fully opaque, allocated/freed on C side ─────────────
// tardy_vm_t is enormous (65536 agents * ~80KB each). We never allocate
// it from Rust. Instead, bench_wrapper.c provides create/destroy functions
// that use calloc/free on the C side. Rust only holds `*mut tardy_vm_t`.

/// Opaque VM type — never instantiated on the Rust side.
/// Use `tardy_bench_vm_create` / `tardy_bench_vm_destroy` from C.
#[repr(C)]
pub struct tardy_vm_t {
    _opaque: [u8; 0],
}

// ── Default semantics (mirrors TARDY_DEFAULT_SEMANTICS in C) ─────

impl tardy_semantics_t {
    pub fn default_semantics() -> Self {
        tardy_semantics_t {
            truth: tardy_truth_semantics_t {
                min_evidence_triples: 1,
                max_contradictions: 0,
                min_confidence: 0.85,
                min_consensus_agents: 3,
                min_agreement_ratio: 0.67,
            },
            hallucination: tardy_hallucination_semantics_t {
                grounding_threshold: 0.0,
                min_decomposers: 3,
                min_decomposition_agreement: 0.5,
                require_dual_ontology: true,
            },
            laziness: tardy_laziness_semantics_t {
                min_observed_operations: 1,
                min_work_authenticity: 0.9,
                max_idle_ms: 5000,
                min_impossibility_verifiers: 2,
                max_work_similarity: 0.95,
                max_verification_chain: 3,
            },
            immutability: tardy_immutability_semantics_t {
                hardened_replica_count: 3,
                sovereign_replica_count: 5,
                sovereign_quorum_ratio: 0.67,
            },
            lifecycle: tardy_lifecycle_semantics_t {
                demotion_idle_ms: 30000,
                temp_ttl_ms: 60000,
                sovereign_dump_idle_ms: 300000,
                gc_interval_ms: 1000,
            },
            pipeline: tardy_pipeline_semantics_t {
                layer_ontology_grounding: true,
                layer_consistency_check: true,
                layer_probabilistic_scoring: true,
                layer_protocol_check: true,
                layer_formal_certification: false,
                layer_cross_representation: false,
                layer_work_verification: true,
                min_passing_layers: 5,
                skip_for_literals: true,
                skip_for_arithmetic: true,
                skip_for_internal_routing: true,
            },
        }
    }
}

// ── Extern C functions ────────────────────────────────────────────

unsafe extern "C" {
    // ── Bench wrapper (C-side allocation) ────────────────────────
    /// Allocate + calloc + tardy_vm_init with default semantics.
    /// Returns NULL on failure.
    pub fn tardy_bench_vm_create() -> *mut tardy_vm_t;
    /// tardy_vm_shutdown + free. Accepts NULL safely.
    pub fn tardy_bench_vm_destroy(vm: *mut tardy_vm_t);
    /// Read the root_id from the VM struct (avoids offset guessing).
    pub fn tardy_bench_vm_root_id(vm: *const tardy_vm_t) -> tardy_uuid_t;

    // VM lifecycle (kept for advanced use but prefer bench wrappers)
    pub fn tardy_vm_init(vm: *mut tardy_vm_t, semantics: *const tardy_semantics_t) -> c_int;
    pub fn tardy_vm_shutdown(vm: *mut tardy_vm_t);

    // Agent management
    pub fn tardy_vm_spawn(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        type_: tardy_type_t,
        trust: tardy_trust_t,
        data: *const c_void,
        len: usize,
    ) -> tardy_uuid_t;

    pub fn tardy_vm_spawn_error(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        message: *const c_char,
    ) -> tardy_uuid_t;

    pub fn tardy_vm_kill(vm: *mut tardy_vm_t, agent_id: tardy_uuid_t) -> c_int;

    pub fn tardy_vm_read(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        out: *mut c_void,
        len: usize,
    ) -> tardy_read_status_t;

    pub fn tardy_vm_mutate(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        data: *const c_void,
        len: usize,
    ) -> c_int;

    pub fn tardy_vm_freeze(
        vm: *mut tardy_vm_t,
        agent_id: tardy_uuid_t,
        new_trust: tardy_trust_t,
    ) -> tardy_uuid_t;

    // Messaging
    pub fn tardy_vm_send(
        vm: *mut tardy_vm_t,
        from: tardy_uuid_t,
        to: tardy_uuid_t,
        payload: *const c_void,
        len: usize,
        type_: tardy_type_t,
    ) -> c_int;

    pub fn tardy_vm_recv(
        vm: *mut tardy_vm_t,
        agent_id: tardy_uuid_t,
        out: *mut tardy_message_t,
    ) -> c_int;

    // Lookup
    pub fn tardy_vm_find(vm: *mut tardy_vm_t, id: tardy_uuid_t) -> *mut tardy_agent_t;
    pub fn tardy_vm_find_by_name(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
    ) -> *mut tardy_agent_t;

    // GC
    pub fn tardy_vm_gc(vm: *mut tardy_vm_t) -> c_int;
    pub fn tardy_vm_demote(vm: *mut tardy_vm_t, agent_id: tardy_uuid_t) -> c_int;
    pub fn tardy_vm_promote(vm: *mut tardy_vm_t, agent_id: tardy_uuid_t) -> c_int;

    // Full read
    pub fn tardy_vm_read_full(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        out: *mut c_void,
        len: usize,
    ) -> tardy_read_result_t;
}
