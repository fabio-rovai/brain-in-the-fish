//! Pipeline: maps every brain-in-the-fish operation to Tardygrada VM operations.
//!
//! Each function spawns, reads, mutates, or messages agents in the VM,
//! so every operation has provenance, trust, and verification built in.

use crate::ffi::*;
use crate::vm_agents::TardyVm;

/// Spawn an evaluator agent with role, domain, and trust metadata.
/// Returns the evaluator agent's UUID.
pub fn spawn_evaluator(
    vm: &TardyVm,
    name: &str,
    role: &str,
    domain: &str,
    trust: tardy_trust_t,
) -> Result<tardy_uuid_t, String> {
    // Spawn a parent agent for this evaluator
    let agent_id = vm.spawn_str(vm.root_id(), name, role, trust)?;
    // Attach domain as a child agent
    let domain_name = format!("{name}_domain");
    vm.spawn_str(agent_id, &domain_name, domain, trust)?;
    Ok(agent_id)
}

/// Record a score: spawns a mutable float agent under the evaluator for a criterion.
pub fn record_score(
    vm: &TardyVm,
    agent_id: tardy_uuid_t,
    criterion_id: &str,
    score: f64,
    justification: &str,
) -> Result<tardy_uuid_t, String> {
    // Spawn the score as a mutable float
    let score_name = format!("score_{criterion_id}");
    let score_id = vm.spawn_float(agent_id, &score_name, score, tardy_trust_t::TARDY_TRUST_MUTABLE)?;
    // Spawn justification as a verified string child of the score agent
    let just_name = format!("just_{criterion_id}");
    vm.spawn_str(agent_id, &just_name, justification, tardy_trust_t::TARDY_TRUST_VERIFIED)?;
    Ok(score_id)
}

/// Read a verified score for a criterion from an evaluator.
pub fn read_score(
    vm: &TardyVm,
    agent_id: tardy_uuid_t,
    criterion_id: &str,
) -> Option<f64> {
    let score_name = format!("score_{criterion_id}");
    vm.read_float(agent_id, &score_name).ok()
}

/// Store a trust relationship: evaluator trusts another evaluator at a given level.
pub fn store_trust(
    vm: &TardyVm,
    agent_id: tardy_uuid_t,
    target_name: &str,
    trust_level: &str,
) -> Result<tardy_uuid_t, String> {
    let trust_name = format!("trust_{target_name}");
    vm.spawn_str(
        agent_id,
        &trust_name,
        trust_level,
        tardy_trust_t::TARDY_TRUST_MUTABLE,
    )
}

/// Update a trust relationship (mutate the existing trust agent).
pub fn update_trust(
    vm: &TardyVm,
    agent_id: tardy_uuid_t,
    target_name: &str,
    new_trust: &str,
) -> Result<(), String> {
    let trust_name = format!("trust_{target_name}");
    // We need to use the raw mutate for strings
    let c_name = std::ffi::CString::new(trust_name.as_str()).map_err(|e| e.to_string())?;
    let c_val = std::ffi::CString::new(new_trust).map_err(|e| e.to_string())?;
    let rc = unsafe {
        tardy_vm_mutate(
            vm.as_ptr(),
            agent_id,
            c_name.as_ptr(),
            c_val.as_ptr() as *const std::os::raw::c_void,
            new_trust.len() + 1,
        )
    };
    if rc == 0 { Ok(()) } else { Err(format!("update_trust failed: {rc}")) }
}

/// Send a challenge message from one evaluator to another.
pub fn send_challenge(
    vm: &TardyVm,
    challenger_id: tardy_uuid_t,
    target_id: tardy_uuid_t,
    argument: &str,
) -> Result<(), String> {
    vm.send(challenger_id, target_id, argument)
}

/// Receive a response message from an evaluator's inbox.
pub fn recv_response(vm: &TardyVm, agent_id: tardy_uuid_t) -> Option<String> {
    vm.recv(agent_id)
}

/// Store an alignment mapping: parent -> section -> criterion with confidence.
pub fn store_alignment(
    vm: &TardyVm,
    parent: tardy_uuid_t,
    section_id: &str,
    criterion_id: &str,
    confidence: f64,
) -> Result<tardy_uuid_t, String> {
    let align_name = format!("align_{section_id}_{criterion_id}");
    vm.spawn_float(
        parent,
        &align_name,
        confidence,
        tardy_trust_t::TARDY_TRUST_VERIFIED,
    )
}

/// Store an argument graph node.
pub fn store_arg_node(
    vm: &TardyVm,
    parent: tardy_uuid_t,
    iri: &str,
    node_type: &str,
    text: &str,
) -> Result<tardy_uuid_t, String> {
    let node_name = format!("arg_{iri}");
    let content = format!("{node_type}:{text}");
    vm.spawn_fact(
        parent,
        &node_name,
        &content,
        tardy_trust_t::TARDY_TRUST_VERIFIED,
    )
}

/// Store a final verdict.
pub fn store_verdict(
    vm: &TardyVm,
    parent: tardy_uuid_t,
    verdict: &str,
    reason: &str,
) -> Result<tardy_uuid_t, String> {
    let content = format!("{verdict}|{reason}");
    vm.spawn_fact(
        parent,
        "verdict",
        &content,
        tardy_trust_t::TARDY_TRUST_VERIFIED,
    )
}
