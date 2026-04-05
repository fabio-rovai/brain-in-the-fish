//! Orchestrator: full brain-in-the-fish evaluation pipeline on the Tardygrada VM.
//!
//! Two modes:
//! - `run_full_pipeline` — mock-based, deterministic, for Criterion coordination benchmarks
//! - `run_full_pipeline_llm` — real LLM subagents via Claude API through Tardygrada VM
//!
//! In LLM mode, each agent is a tardy_vm_spawn agent. Scores are stored as
//! verified Fact agents via record_score. Score reads go through hash-verified
//! tardy_vm_read. Challenges sent via tardy_vm_send. Responses received via
//! tardy_vm_recv. Trust updates via tardy_vm_mutate. Verdict stored as
//! sovereign Fact. GC at the end.

use crate::ffi::*;
use crate::pipeline;
use crate::vm_agents::TardyVm;

use bench_naive::llm::{extract_json, ClaudeClient, ScoringResult, ChallengeResult, ResponseResult};

/// A benchmark score record.
#[derive(Debug, Clone)]
pub struct BenchScore {
    pub agent_name: String,
    pub criterion_id: String,
    pub score: f64,
    pub round: usize,
}

/// Run the full brain-in-the-fish evaluation pipeline using the Tardygrada VM
/// with MOCK scores.
///
/// Kept for Criterion coordination benchmarks — no API calls, deterministic.
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

/// Run the full evaluation pipeline with REAL LLM subagents on the Tardygrada VM.
///
/// Each agent is a tardy_vm_spawn agent. LLM calls happen in parallel tokio tasks.
/// Scores stored as verified Fact agents. Reads go through hash-verified tardy_vm_read.
/// Challenges sent via tardy_vm_send. Verdict stored as sovereign Fact. GC at end.
#[allow(clippy::too_many_arguments)]
pub async fn run_full_pipeline_llm(
    _client: &ClaudeClient,
    vm: &TardyVm,
    agent_names: &[&str],
    criterion_ids: &[&str],
    alignments: &[(&str, &str, f64)],
    section_texts: &[(&str, &str)],  // (section_id, section_text) for prompt generation
    max_rounds: usize,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> String {
    let root = vm.root_id();
    let mut all_scores: Vec<BenchScore> = Vec::new();

    // ── Phase 1: Spawn evaluator agents ───────────────────────────
    let mut agent_ids: Vec<(String, tardy_uuid_t)> = Vec::new();
    for &name in agent_names {
        let id = pipeline::spawn_evaluator(
            vm, name, "evaluator", "benchmark",
            tardy_trust_t::TARDY_TRUST_DEFAULT,
        )
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

    // Build section text lookup
    let section_map: std::collections::HashMap<&str, &str> = section_texts.iter()
        .map(|&(id, text)| (id, text))
        .collect();

    // Build alignment lookup: criterion_id -> [(section_id, confidence)]
    let mut alignment_map: std::collections::HashMap<&str, Vec<(&str, f64)>> =
        std::collections::HashMap::new();
    for &(section, criterion, confidence) in alignments {
        alignment_map.entry(criterion).or_default().push((section, confidence));
    }

    // ── Phase 4: Debate loop with real LLM scoring ────────────────
    let mut converged = false;
    let mut final_round = 0;

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let model = std::env::var("BRAIN_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

    for round in 0..max_rounds {
        final_round = round;

        // 4a. Parallel LLM scoring for all (agent, criterion) pairs
        let mut scoring_handles = Vec::new();

        for (agent_name, _agent_id) in &agent_ids {
            for &criterion in criterion_ids {
                // Build section context for this criterion
                let relevant_sections: Vec<String> = alignment_map
                    .get(criterion)
                    .map(|secs| {
                        secs.iter()
                            .filter_map(|&(sec_id, conf)| {
                                section_map.get(sec_id).map(|text| {
                                    let preview: String = text.chars().take(200).collect();
                                    format!("- [{:.0}% match] {sec_id}: {preview}...", conf * 100.0)
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let prompt = format!(
                    "You are {agent_name} (evaluator, benchmark domain).\n\n\
                     ## Round {round} Evaluation\n\n\
                     **Criterion:** {criterion}\n\
                     **Max score:** 10.0\n\n\
                     ### Relevant document sections\n{sections}\n\n\
                     Score this criterion on a scale of 0-10.\n\n\
                     Respond ONLY with this JSON format:\n\
                     {{\n\
                       \"score\": <number 0-10>,\n\
                       \"justification\": \"<your justification>\",\n\
                       \"evidence_used\": [\"<evidence1>\", ...],\n\
                       \"gaps_identified\": [\"<gap1>\", ...]\n\
                     }}",
                    sections = if relevant_sections.is_empty() {
                        "  (no matching sections found)".to_string()
                    } else {
                        relevant_sections.join("\n")
                    },
                );

                let agent_name_owned = agent_name.clone();
                let criterion_owned = criterion.to_string();
                let api_key = api_key.clone();
                let model = model.clone();
                let base_url = base_url.clone();

                scoring_handles.push(tokio::spawn(async move {
                    let http_client = reqwest::Client::new();
                    let result = call_claude_scoring_raw(
                        &http_client, &api_key, &model, &base_url, &prompt,
                    ).await;

                    match result {
                        Ok(sr) => (agent_name_owned, criterion_owned, sr.score.clamp(0.0, 10.0), round),
                        Err(e) => {
                            eprintln!("  [WARN] LLM scoring failed for {agent_name_owned}/{criterion_owned}: {e}");
                            (agent_name_owned, criterion_owned, 5.0, round) // fallback midpoint
                        }
                    }
                }));
            }
        }

        // Await all scoring tasks and record in VM
        for handle in scoring_handles {
            if let Ok((agent_name, criterion_id, score, rnd)) = handle.await {
                let agent_entry = agent_ids.iter().find(|(n, _)| *n == agent_name);
                if let Some((_, agent_id)) = agent_entry {
                    let justification = format!("llm_round_{rnd}_score_{score:.2}");

                    if rnd == 0 {
                        if let Err(e) = pipeline::record_score(vm, *agent_id, &criterion_id, score, &justification) {
                            eprintln!("record_score: {e}");
                        }
                    } else {
                        let score_name = format!("score_{criterion_id}");
                        let _ = vm.mutate_float(*agent_id, &score_name, score);
                    }

                    all_scores.push(BenchScore {
                        agent_name,
                        criterion_id,
                        score,
                        round: rnd,
                    });
                }
            }
        }

        // 4b. Read verified scores and find disagreements
        let mut max_disagreement: f64 = 0.0;
        let mut disagreement_pairs: Vec<(usize, usize, String, f64)> = Vec::new();

        for &criterion in criterion_ids {
            let scores: Vec<(usize, f64)> = agent_ids
                .iter()
                .enumerate()
                .filter_map(|(i, (_, id))| {
                    pipeline::read_score(vm, *id, criterion).map(|s| (i, s))
                })
                .collect();

            for i in 0..scores.len() {
                for j in (i + 1)..scores.len() {
                    let spread = (scores[i].1 - scores[j].1).abs();
                    if spread > max_disagreement {
                        max_disagreement = spread;
                    }
                    if spread > disagreement_threshold {
                        disagreement_pairs.push((
                            scores[i].0,
                            scores[j].0,
                            criterion.to_string(),
                            spread,
                        ));
                    }
                }
            }
        }

        // 4c. Parallel LLM challenges for disagreements
        if !disagreement_pairs.is_empty() {
            let mut challenge_handles = Vec::new();

            for (idx_a, idx_b, criterion, delta) in &disagreement_pairs {
                let name_a = agent_ids[*idx_a].0.clone();
                let name_b = agent_ids[*idx_b].0.clone();
                let score_a = pipeline::read_score(vm, agent_ids[*idx_a].1, criterion).unwrap_or(5.0);
                let score_b = pipeline::read_score(vm, agent_ids[*idx_b].1, criterion).unwrap_or(5.0);
                let criterion = criterion.clone();
                let delta = *delta;
                let api_key = api_key.clone();
                let model = model.clone();
                let base_url = base_url.clone();

                challenge_handles.push(tokio::spawn(async move {
                    let http_client = reqwest::Client::new();

                    // Generate challenge
                    let challenge_prompt = format!(
                        "You are {name_a} challenging {name_b}'s score on \"{criterion}\".\n\
                         Your score: {score_a:.1}/10\n\
                         Their score: {score_b:.1}/10\n\
                         Difference: {delta:.1}\n\n\
                         Explain why your assessment differs.\n\n\
                         Respond ONLY with JSON:\n\
                         {{\"argument\": \"<your argument>\", \"evidence_cited\": [\"<ev1>\", ...]}}"
                    );

                    let challenge_result = call_claude_challenge_raw(
                        &http_client, &api_key, &model, &base_url, &challenge_prompt,
                    ).await;

                    let challenge_arg = match &challenge_result {
                        Ok(cr) => cr.argument.clone(),
                        Err(_) => "Challenge generation failed".to_string(),
                    };

                    // Generate response
                    let response_prompt = format!(
                        "You are {name_b} responding to {name_a}'s challenge on \"{criterion}\".\n\
                         Your original score: {score_b:.1}/10\n\n\
                         Their challenge: {challenge_arg}\n\n\
                         Respond ONLY with JSON:\n\
                         {{\"maintain_score\": true/false, \"new_score\": null or <number>, \
                         \"response\": \"<response>\", \"justification\": \"<justification>\"}}"
                    );

                    let response_result = call_claude_response_raw(
                        &http_client, &api_key, &model, &base_url, &response_prompt,
                    ).await;

                    let new_score = match &response_result {
                        Ok(rr) if !rr.maintain_score => rr.new_score,
                        _ => None,
                    };

                    (name_a, name_b, criterion, challenge_arg, new_score)
                }));
            }

            // Process challenge results — send via VM messaging, apply score changes
            for handle in challenge_handles {
                if let Ok((name_a, name_b, criterion, challenge_arg, new_score)) = handle.await {
                    // Send challenge via VM
                    let id_a = agent_ids.iter().find(|(n, _)| *n == name_a).map(|(_, id)| *id);
                    let id_b = agent_ids.iter().find(|(n, _)| *n == name_b).map(|(_, id)| *id);
                    if let (Some(id_a), Some(id_b)) = (id_a, id_b) {
                        let _ = pipeline::send_challenge(vm, id_a, id_b, &challenge_arg);

                        // Apply score change if agent changed their mind
                        if let Some(ns) = new_score {
                            let score_name = format!("score_{criterion}");
                            let _ = vm.mutate_float(id_b, &score_name, ns.clamp(0.0, 10.0));
                        }
                    }
                }
            }

            // Drain responses
            for (_, agent_id) in &agent_ids {
                while pipeline::recv_response(vm, *agent_id).is_some() {}
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
        .spawn_str(root, "argument_graph_llm", "bitf_eval", tardy_trust_t::TARDY_TRUST_VERIFIED)
        .unwrap_or_else(|e| panic!("spawn arg graph: {e}"));

    for (criterion, score) in &consensus_scores {
        let node_type = if *score >= 7.0 { "StrongClaim" } else { "WeakClaim" };
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

    let verdict = if overall >= 7.0 && converged {
        "CONFIRMED"
    } else if overall >= 4.0 {
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

// ── Internal API call helpers (self-contained for tokio::spawn 'static) ──

async fn call_claude_scoring_raw(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    prompt: &str,
) -> anyhow::Result<ScoringResult> {
    let response = call_claude_raw(
        client, api_key, model, base_url,
        "You are an expert evaluator. Respond ONLY with valid JSON.",
        prompt, 2000,
    ).await?;
    let json_str = extract_json(&response);
    serde_json::from_str(&json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse scoring response: {}. Raw: {}", e, response)
    })
}

async fn call_claude_challenge_raw(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    prompt: &str,
) -> anyhow::Result<ChallengeResult> {
    let response = call_claude_raw(
        client, api_key, model, base_url,
        "You are an expert evaluator in a structured debate. Respond ONLY with valid JSON.",
        prompt, 1500,
    ).await?;
    let json_str = extract_json(&response);
    serde_json::from_str(&json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse challenge: {}. Raw: {}", e, response)
    })
}

async fn call_claude_response_raw(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    prompt: &str,
) -> anyhow::Result<ResponseResult> {
    let response = call_claude_raw(
        client, api_key, model, base_url,
        "You are an expert evaluator responding to a challenge. Respond ONLY with valid JSON.",
        prompt, 1500,
    ).await?;
    let json_str = extract_json(&response);
    serde_json::from_str(&json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse response: {}. Raw: {}", e, response)
    })
}

/// Low-level Claude API call with one retry on transient failure.
async fn call_claude_raw(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    system: &str,
    user_prompt: &str,
    max_tokens: u32,
) -> anyhow::Result<String> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        "messages": [{"role": "user", "content": user_prompt}],
    });

    let result = do_request(client, api_key, base_url, &body).await;
    match result {
        Ok(text) => Ok(text),
        Err(first_err) => {
            eprintln!("  [RETRY] First attempt failed: {first_err}, retrying...");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            do_request(client, api_key, base_url, &body).await
        }
    }
}

async fn do_request(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    body: &serde_json::Value,
) -> anyhow::Result<String> {
    #[derive(serde::Deserialize)]
    struct Resp { content: Vec<Block> }
    #[derive(serde::Deserialize)]
    struct Block { text: Option<String> }

    let resp = client
        .post(format!("{base_url}/v1/messages"))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Claude API error {status}: {text}");
    }

    let response: Resp = resp.json().await?;
    Ok(response.content.iter().filter_map(|b| b.text.as_ref()).cloned().collect::<Vec<_>>().join(""))
}
