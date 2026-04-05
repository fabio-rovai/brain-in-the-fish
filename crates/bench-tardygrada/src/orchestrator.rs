//! Orchestrator: full brain-in-the-fish evaluation pipeline on the Tardygrada VM.
//!
//! Runs: spawn agents -> wire trust -> store alignments -> debate loop
//! (record scores, read verified, find disagreements, send challenges,
//! calc drift, update trust) -> moderation consensus -> build arg graph
//! -> gate verdict -> GC.

use crate::ffi::*;
use crate::pipeline;
use crate::vm_agents::TardyVm;

/// A benchmark score record.
#[derive(Debug, Clone)]
pub struct BenchScore {
    pub agent_name: String,
    pub criterion_id: String,
    pub score: f64,
    pub round: usize,
}

/// Run the full brain-in-the-fish evaluation pipeline using the Tardygrada VM.
///
/// Returns a JSON-formatted summary string with scores, verdicts, and GC stats.
#[allow(clippy::too_many_arguments)]
pub fn run_full_pipeline(
    vm: &TardyVm,
    agent_names: &[&str],
    criterion_ids: &[&str],
    alignments: &[(&str, &str, f64)],   // (section_id, criterion_id, confidence)
    mock_scores: &[(&str, &str, f64)],   // (agent_name, criterion_id, score)
    max_rounds: usize,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> String {
    let root = vm.root_id();
    let mut all_scores: Vec<BenchScore> = Vec::new();

    // ── Phase 1: Spawn evaluator agents ───────────────────────────
    let mut agent_ids: Vec<(String, tardy_uuid_t)> = Vec::new();
    for &name in agent_names {
        let id = pipeline::spawn_evaluator(vm, name, "evaluator", "benchmark", tardy_trust_t::TARDY_TRUST_DEFAULT)
            .unwrap_or_else(|e| panic!("spawn evaluator '{name}': {e}"));
        agent_ids.push((name.to_string(), id));
    }

    // ── Phase 2: Wire trust between agents ────────────────────────
    for i in 0..agent_ids.len() {
        for j in 0..agent_ids.len() {
            if i != j {
                let (ref name_j, _) = agent_ids[j];
                pipeline::store_trust(vm, agent_ids[i].1, name_j, "peer")
                    .unwrap_or_else(|e| panic!("store_trust: {e}"));
            }
        }
    }

    // ── Phase 3: Store alignments ─────────────────────────────────
    for &(section, criterion, confidence) in alignments {
        pipeline::store_alignment(vm, root, section, criterion, confidence)
            .unwrap_or_else(|e| panic!("store_alignment: {e}"));
    }

    // ── Phase 4: Debate loop ──────────────────────────────────────
    let mut converged = false;
    let mut final_round = 0;

    for round in 0..max_rounds {
        final_round = round;

        // 4a. Record scores (use mock_scores for initial, then drift on subsequent rounds)
        for &(agent_name, criterion, score) in mock_scores {
            let agent_entry = agent_ids.iter().find(|(n, _)| n == agent_name);
            if let Some((_, agent_id)) = agent_entry {
                // Add a small drift per round to simulate debate convergence
                let drifted_score = if round == 0 {
                    score
                } else {
                    // Read current and drift toward mean
                    let current = pipeline::read_score(vm, *agent_id, criterion).unwrap_or(score);
                    let mean = compute_mean_score(vm, &agent_ids, criterion);
                    current + (mean - current) * 0.3
                };

                let justification = format!("round_{round}_score_{drifted_score:.2}");

                if round == 0 {
                    pipeline::record_score(vm, *agent_id, criterion, drifted_score, &justification)
                        .unwrap_or_else(|e| panic!("record_score: {e}"));
                } else {
                    // Mutate existing score
                    let score_name = format!("score_{criterion}");
                    let _ = vm.mutate_float(*agent_id, &score_name, drifted_score);
                }

                all_scores.push(BenchScore {
                    agent_name: agent_name.to_string(),
                    criterion_id: criterion.to_string(),
                    score: drifted_score,
                    round,
                });
            }
        }

        // 4b. Read verified scores and find disagreements
        let mut max_disagreement: f64 = 0.0;
        for &criterion in criterion_ids {
            let scores: Vec<f64> = agent_ids
                .iter()
                .filter_map(|(_, id)| pipeline::read_score(vm, *id, criterion))
                .collect();

            if scores.len() >= 2 {
                let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let spread = max - min;
                if spread > max_disagreement {
                    max_disagreement = spread;
                }
            }
        }

        // 4c. Send challenges for disagreements
        if max_disagreement > disagreement_threshold {
            for i in 0..agent_ids.len() {
                for j in (i + 1)..agent_ids.len() {
                    let challenge = format!(
                        "round_{round}_disagreement_{max_disagreement:.3}_challenge"
                    );
                    let _ = pipeline::send_challenge(
                        vm,
                        agent_ids[i].1,
                        agent_ids[j].1,
                        &challenge,
                    );
                }
            }
        }

        // 4d. Drain responses (challenges received become responses)
        for (_, agent_id) in &agent_ids {
            while pipeline::recv_response(vm, *agent_id).is_some() {
                // Process responses — in benchmark we just drain them
            }
        }

        // 4e. Update trust based on agreement
        for i in 0..agent_ids.len() {
            for j in 0..agent_ids.len() {
                if i != j {
                    let trust_label = if max_disagreement < convergence_threshold {
                        "high"
                    } else if max_disagreement < disagreement_threshold {
                        "medium"
                    } else {
                        "low"
                    };
                    let _ = pipeline::update_trust(
                        vm,
                        agent_ids[i].1,
                        &agent_ids[j].0,
                        trust_label,
                    );
                }
            }
        }

        // 4f. Check convergence
        if max_disagreement <= convergence_threshold {
            converged = true;
            break;
        }
    }

    // ── Phase 5: Moderation consensus ─────────────────────────────
    let mut consensus_scores: Vec<(String, f64)> = Vec::new();
    for &criterion in criterion_ids {
        let scores: Vec<f64> = agent_ids
            .iter()
            .filter_map(|(_, id)| pipeline::read_score(vm, *id, criterion))
            .collect();
        let mean = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };
        consensus_scores.push((criterion.to_string(), mean));
    }

    // ── Phase 6: Build argument graph ─────────────────────────────
    let arg_parent = vm
        .spawn_str(root, "argument_graph", "bitf_eval", tardy_trust_t::TARDY_TRUST_VERIFIED)
        .unwrap_or_else(|e| panic!("spawn arg graph: {e}"));

    for (criterion, score) in &consensus_scores {
        let node_type = if *score >= 0.7 { "StrongClaim" } else { "WeakClaim" };
        let text = format!("{criterion}: {score:.3}");
        pipeline::store_arg_node(vm, arg_parent, criterion, node_type, &text)
            .unwrap_or_else(|e| panic!("store_arg_node: {e}"));
    }

    // ── Phase 7: Gate verdict ─────────────────────────────────────
    let overall = if consensus_scores.is_empty() {
        0.0
    } else {
        consensus_scores.iter().map(|(_, s)| s).sum::<f64>() / consensus_scores.len() as f64
    };

    let verdict = if overall >= 0.7 && converged {
        "CONFIRMED"
    } else if overall >= 0.4 {
        "FLAGGED"
    } else {
        "REJECTED"
    };

    let reason = format!(
        "overall={overall:.3}, converged={converged}, rounds={final_round}, agents={}",
        agent_names.len()
    );
    pipeline::store_verdict(vm, root, verdict, &reason)
        .unwrap_or_else(|e| panic!("store_verdict: {e}"));

    // ── Phase 8: GC ──────────────────────────────────────────────
    let gc_collected = vm.gc();

    // ── Build result JSON ─────────────────────────────────────────
    let scores_json: Vec<String> = consensus_scores
        .iter()
        .map(|(c, s)| format!("    \"{c}\": {s:.4}"))
        .collect();

    let history_json: Vec<String> = all_scores
        .iter()
        .map(|s| {
            format!(
                "    {{\"agent\": \"{}\", \"criterion\": \"{}\", \"score\": {:.4}, \"round\": {}}}",
                s.agent_name, s.criterion_id, s.score, s.round
            )
        })
        .collect();

    format!(
        r#"{{
  "verdict": "{verdict}",
  "overall_score": {overall:.4},
  "converged": {converged},
  "rounds": {final_round},
  "gc_collected": {gc_collected},
  "consensus_scores": {{
{scores}
  }},
  "score_history": [
{history}
  ]
}}"#,
        scores = scores_json.join(",\n"),
        history = history_json.join(",\n"),
    )
}

/// Compute mean score across all agents for a given criterion.
fn compute_mean_score(
    vm: &TardyVm,
    agent_ids: &[(String, tardy_uuid_t)],
    criterion: &str,
) -> f64 {
    let scores: Vec<f64> = agent_ids
        .iter()
        .filter_map(|(_, id)| pipeline::read_score(vm, *id, criterion))
        .collect();
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}
