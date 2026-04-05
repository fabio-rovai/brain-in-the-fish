//! Orchestrator — runs the full evaluation pipeline end-to-end.
//!
//! Two modes:
//! - `run_full_pipeline` — mock-based, deterministic, for Criterion coordination benchmarks
//! - `run_full_pipeline_llm` — real LLM subagents via Claude API, parallel tokio tasks

use uuid::Uuid;

use crate::agent;
use crate::alignment;
use crate::argument_graph;
use crate::debate;
use crate::gate;
use crate::llm::ClaudeClient;
use crate::moderation::{self, OverallResult};
use crate::rules;
use crate::scoring;
use crate::types::{
    Challenge, Document, Framework, GateWeights, Score, Session, Verdict,
};

use open_ontologies::graph::GraphStore;

/// Run the full naive evaluation pipeline with MOCK scores.
///
/// Kept for Criterion coordination benchmarks — no API calls, deterministic.
pub fn run_full_pipeline(
    doc: Document,
    framework: Framework,
    mock_scores: &[Score],
    max_rounds: u32,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> (Session, OverallResult, Verdict) {
    // 1. Spawn agents
    let mut agents = agent::spawn_panel(&doc.title, &framework);
    agent::wire_trust_weights(&mut agents);

    // 2. Align sections to criteria
    let (alignments, gaps) = alignment::align_sections_to_criteria(&doc, &framework);

    // 3. Build argument graph
    let arg_graph = argument_graph::build_from_document(&doc);

    // 4. Debate loop
    let mut all_scores: Vec<Score> = Vec::new();
    let mut rounds = Vec::new();

    for round in 1..=max_rounds {
        // Collect scores for this round from mock data
        let round_scores: Vec<Score> = mock_scores
            .iter()
            .filter(|s| s.round == round)
            .cloned()
            .collect();

        for s in &round_scores {
            scoring::record_score(&mut all_scores, s.clone());
        }

        // Find disagreements
        let _disagreements =
            debate::find_disagreements(&round_scores, disagreement_threshold);

        // Build challenges (in real pipeline, LLM would generate these)
        let challenges = Vec::new(); // No actual LLM calls in benchmark mode

        // Calculate drift
        let drift_velocity = if round > 1 {
            let prev = scoring::get_scores_for_round(&all_scores, round - 1);
            let curr = scoring::get_scores_for_round(&all_scores, round);
            let prev_owned: Vec<Score> = prev.into_iter().cloned().collect();
            let curr_owned: Vec<Score> = curr.into_iter().cloned().collect();
            Some(debate::calculate_drift_velocity(&prev_owned, &curr_owned))
        } else {
            None
        };

        let converged = drift_velocity
            .map(|dv| debate::check_convergence(dv, convergence_threshold))
            .unwrap_or(false);

        // Update trust weights based on challenges
        debate::update_trust_weights(&mut agents, &challenges);

        rounds.push(debate::build_debate_round(
            round,
            round_scores,
            challenges,
            drift_velocity,
            converged,
        ));

        if converged {
            break;
        }
    }

    // 5. Moderation
    let moderated = moderation::calculate_moderated_scores(&all_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    // 6. Rules — mine facts via GraphStore
    let graph_store = GraphStore::new();
    let facts = rules::mine_facts(&graph_store, &doc);
    let total_claims: usize = doc.sections.iter().map(|s| s.claims.len()).sum();

    // 7. Gate check
    let verdict = gate::check(
        overall.weighted_score,
        overall.max_possible,
        &arg_graph,
        &facts,
        total_claims,
        &GateWeights::default(),
    );

    // Build session
    let session = Session {
        id: Uuid::new_v4().to_string(),
        document: doc,
        framework,
        agents,
        alignments,
        gaps,
        rounds,
        final_scores: moderated,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    (session, overall, verdict)
}

/// Run the full evaluation pipeline with REAL LLM subagents.
///
/// Each agent is a tokio task that independently scores via Claude API.
/// Scoring tasks for each round run in parallel (all agents score simultaneously).
/// Challenge/response pairs also run in parallel.
pub async fn run_full_pipeline_llm(
    _client: &ClaudeClient,
    doc: Document,
    framework: Framework,
    max_rounds: u32,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> (Session, OverallResult, Verdict) {
    // 1. Spawn agent panel
    let mut agents = agent::spawn_panel(&doc.title, &framework);
    agent::wire_trust_weights(&mut agents);

    // 2. Align sections to criteria
    let (alignments, gaps) = alignment::align_sections_to_criteria(&doc, &framework);

    // 3. Build argument graph
    let arg_graph = argument_graph::build_from_document(&doc);

    // 4. Debate loop with real LLM scoring
    let mut all_scores: Vec<Score> = Vec::new();
    let mut rounds = Vec::new();

    // Only evaluators score (not the moderator)
    let evaluator_indices: Vec<usize> = agents
        .iter()
        .enumerate()
        .filter(|(_, a)| a.role != "moderator")
        .map(|(i, _)| i)
        .collect();

    for round in 1..=max_rounds {
        // 4a. Parallel LLM scoring: spawn a task for each (agent, criterion) pair
        let mut scoring_handles = Vec::new();

        for &agent_idx in &evaluator_indices {
            let agent = agents[agent_idx].clone();
            for criterion in &framework.criteria {
                let criterion_clone = criterion.clone();
                let sections = alignment::sections_for_criterion(
                    &alignments,
                    &criterion.id,
                    &doc,
                );
                let sections_owned: Vec<(crate::types::Section, f64)> = sections
                    .into_iter()
                    .map(|(s, c)| (s.clone(), c))
                    .collect();
                let agent_clone = agent.clone();
                let round_copy = round;

                // Generate the scoring prompt
                let section_refs: Vec<(&crate::types::Section, f64)> = sections_owned
                    .iter()
                    .map(|(s, c)| (s, *c))
                    .collect();
                let prompt = scoring::generate_scoring_prompt(
                    &agent_clone,
                    &criterion_clone,
                    &section_refs,
                    round_copy,
                );

                // Add JSON format instruction to the prompt
                let full_prompt = format!(
                    "{prompt}\n\nRespond ONLY with this JSON format:\n\
                     {{\n\
                       \"score\": <number 0-{max}>,\n\
                       \"justification\": \"<your justification>\",\n\
                       \"evidence_used\": [\"<evidence1>\", ...],\n\
                       \"gaps_identified\": [\"<gap1>\", ...]\n\
                     }}",
                    max = criterion_clone.max_score,
                );

                let agent_id = agent_clone.id.clone();
                let criterion_id = criterion_clone.id.clone();
                let max_score = criterion_clone.max_score;

                scoring_handles.push(tokio::spawn({
                    // We need to use a reference to client, but tokio::spawn requires 'static.
                    // So we build the future inline and use the prompt we already have.
                    let prompt = full_prompt;
                    // We cannot move client into spawn since it's borrowed. Build the HTTP
                    // request inline instead (duplicating the API call logic for spawn compatibility).
                    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
                    let model = std::env::var("BRAIN_MODEL")
                        .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
                    let base_url = std::env::var("ANTHROPIC_BASE_URL")
                        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
                    let http_client = reqwest::Client::new();

                    async move {
                        let result = call_claude_scoring(
                            &http_client,
                            &api_key,
                            &model,
                            &base_url,
                            &prompt,
                        )
                        .await;

                        match result {
                            Ok(sr) => Score {
                                agent_id,
                                criterion_id,
                                score: sr.score.clamp(0.0, max_score),
                                max_score,
                                round: round_copy,
                                justification: sr.justification,
                                evidence_used: sr.evidence_used,
                                gaps_identified: sr.gaps_identified,
                            },
                            Err(e) => {
                                eprintln!("  [WARN] LLM scoring failed, using fallback: {e}");
                                Score {
                                    agent_id,
                                    criterion_id,
                                    score: max_score * 0.5, // fallback: midpoint
                                    max_score,
                                    round: round_copy,
                                    justification: format!("LLM call failed: {e}"),
                                    evidence_used: Vec::new(),
                                    gaps_identified: Vec::new(),
                                }
                            }
                        }
                    }
                }));
            }
        }

        // 4b. Await all scoring tasks (parallel LLM calls!)
        let mut round_scores: Vec<Score> = Vec::new();
        for handle in scoring_handles {
            match handle.await {
                Ok(score) => round_scores.push(score),
                Err(e) => eprintln!("  [WARN] Scoring task panicked: {e}"),
            }
        }

        for s in &round_scores {
            scoring::record_score(&mut all_scores, s.clone());
        }

        // 4c. Find disagreements
        let disagreements =
            debate::find_disagreements(&round_scores, disagreement_threshold);

        // 4d. Parallel challenge/response for each disagreement
        let mut challenges: Vec<Challenge> = Vec::new();

        if !disagreements.is_empty() {
            let mut challenge_handles = Vec::new();

            for disagreement in &disagreements {
                let challenger = agents
                    .iter()
                    .find(|a| a.id == disagreement.agent_a_id)
                    .cloned();
                let target = agents
                    .iter()
                    .find(|a| a.id == disagreement.agent_b_id)
                    .cloned();
                let criterion = framework
                    .criteria
                    .iter()
                    .find(|c| c.id == disagreement.criterion_id)
                    .cloned();
                let target_score = round_scores
                    .iter()
                    .find(|s| {
                        s.agent_id == disagreement.agent_b_id
                            && s.criterion_id == disagreement.criterion_id
                    })
                    .cloned();

                if let (Some(challenger), Some(target), Some(criterion), Some(target_score)) =
                    (challenger, target, criterion, target_score)
                {
                    let disagreement = disagreement.clone();
                    let round_copy = round;

                    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
                    let model = std::env::var("BRAIN_MODEL")
                        .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
                    let base_url = std::env::var("ANTHROPIC_BASE_URL")
                        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
                    let http_client = reqwest::Client::new();

                    challenge_handles.push(tokio::spawn(async move {
                        // Generate challenge
                        let challenge_prompt = debate::generate_challenge_prompt(
                            &challenger,
                            &target,
                            &disagreement,
                            &criterion,
                        );
                        let challenge_prompt_json = format!(
                            "{challenge_prompt}\n\nRespond ONLY with this JSON format:\n\
                             {{\n\
                               \"argument\": \"<your challenge argument>\",\n\
                               \"evidence_cited\": [\"<evidence1>\", ...]\n\
                             }}"
                        );

                        let challenge_result = call_claude_challenge(
                            &http_client,
                            &api_key,
                            &model,
                            &base_url,
                            &challenge_prompt_json,
                        )
                        .await;

                        let challenge_arg = match &challenge_result {
                            Ok(cr) => cr.argument.clone(),
                            Err(_) => "Challenge generation failed".to_string(),
                        };

                        // Generate response
                        let response_prompt = debate::generate_response_prompt(
                            &target,
                            &challenger,
                            &challenge_arg,
                            &target_score,
                            &criterion,
                        );
                        let response_prompt_json = format!(
                            "{response_prompt}\n\nRespond ONLY with this JSON format:\n\
                             {{\n\
                               \"maintain_score\": true/false,\n\
                               \"new_score\": null or <number>,\n\
                               \"response\": \"<your response>\",\n\
                               \"justification\": \"<justification>\"\n\
                             }}"
                        );

                        let response_result = call_claude_response(
                            &http_client,
                            &api_key,
                            &model,
                            &base_url,
                            &response_prompt_json,
                        )
                        .await;

                        let score_change = match &response_result {
                            Ok(rr) if !rr.maintain_score => {
                                rr.new_score.map(|ns| (target_score.score, ns))
                            }
                            _ => None,
                        };

                        Challenge {
                            challenger_id: disagreement.agent_a_id.clone(),
                            target_agent_id: disagreement.agent_b_id.clone(),
                            criterion_id: disagreement.criterion_id.clone(),
                            round: round_copy,
                            argument: challenge_arg,
                            response: response_result.ok().map(|rr| rr.response),
                            score_change,
                        }
                    }));
                }
            }

            for handle in challenge_handles {
                match handle.await {
                    Ok(challenge) => challenges.push(challenge),
                    Err(e) => eprintln!("  [WARN] Challenge task panicked: {e}"),
                }
            }
        }

        // 4e. Calculate drift
        let drift_velocity = if round > 1 {
            let prev = scoring::get_scores_for_round(&all_scores, round - 1);
            let curr = scoring::get_scores_for_round(&all_scores, round);
            let prev_owned: Vec<Score> = prev.into_iter().cloned().collect();
            let curr_owned: Vec<Score> = curr.into_iter().cloned().collect();
            Some(debate::calculate_drift_velocity(&prev_owned, &curr_owned))
        } else {
            None
        };

        let converged = drift_velocity
            .map(|dv| debate::check_convergence(dv, convergence_threshold))
            .unwrap_or(false);

        // Update trust weights based on challenges
        debate::update_trust_weights(&mut agents, &challenges);

        rounds.push(debate::build_debate_round(
            round,
            round_scores,
            challenges,
            drift_velocity,
            converged,
        ));

        if converged {
            break;
        }
    }

    // 5. Moderation
    let moderated = moderation::calculate_moderated_scores(&all_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    // 6. Rules — mine facts via GraphStore
    let graph_store = GraphStore::new();
    let facts = rules::mine_facts(&graph_store, &doc);
    let total_claims: usize = doc.sections.iter().map(|s| s.claims.len()).sum();

    // 7. Gate check
    let verdict = gate::check(
        overall.weighted_score,
        overall.max_possible,
        &arg_graph,
        &facts,
        total_claims,
        &GateWeights::default(),
    );

    // Build session
    let session = Session {
        id: Uuid::new_v4().to_string(),
        document: doc,
        framework,
        agents,
        alignments,
        gaps,
        rounds,
        final_scores: moderated,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    (session, overall, verdict)
}

// ── Internal API call helpers (self-contained for tokio::spawn 'static) ──

use crate::llm::{extract_json, ChallengeResult, ResponseResult, ScoringResult};

async fn call_claude_scoring(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    prompt: &str,
) -> anyhow::Result<ScoringResult> {
    let response = call_claude_raw(
        client,
        api_key,
        model,
        base_url,
        "You are an expert evaluator. Respond ONLY with valid JSON matching the requested format. No markdown, no explanation outside the JSON.",
        prompt,
        2000,
    )
    .await?;
    let json_str = extract_json(&response);
    let result: ScoringResult = serde_json::from_str(&json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse scoring response: {}. Raw: {}", e, response)
    })?;
    Ok(result)
}

async fn call_claude_challenge(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    prompt: &str,
) -> anyhow::Result<ChallengeResult> {
    let response = call_claude_raw(
        client,
        api_key,
        model,
        base_url,
        "You are an expert evaluator in a structured debate. Respond ONLY with valid JSON.",
        prompt,
        1500,
    )
    .await?;
    let json_str = extract_json(&response);
    let result: ChallengeResult = serde_json::from_str(&json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse challenge: {}. Raw: {}", e, response)
    })?;
    Ok(result)
}

async fn call_claude_response(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    prompt: &str,
) -> anyhow::Result<ResponseResult> {
    let response = call_claude_raw(
        client,
        api_key,
        model,
        base_url,
        "You are an expert evaluator responding to a challenge. Respond ONLY with valid JSON.",
        prompt,
        1500,
    )
    .await?;
    let json_str = extract_json(&response);
    let result: ResponseResult = serde_json::from_str(&json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse response: {}. Raw: {}", e, response)
    })?;
    Ok(result)
}

/// Low-level Claude API call — self-contained for use in tokio::spawn tasks.
///
/// Retries once on transient failure.
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
        "messages": [{
            "role": "user",
            "content": user_prompt,
        }],
    });

    // First attempt
    let result = do_claude_request(client, api_key, base_url, &body).await;
    match result {
        Ok(text) => Ok(text),
        Err(first_err) => {
            // Retry once
            eprintln!("  [RETRY] First attempt failed: {first_err}, retrying...");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            do_claude_request(client, api_key, base_url, &body).await
        }
    }
}

async fn do_claude_request(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    body: &serde_json::Value,
) -> anyhow::Result<String> {
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
        anyhow::bail!("Claude API error {}: {}", status, text);
    }

    let response: crate::llm::ClaudeResponse = resp.json().await?;
    let text = response
        .content
        .iter()
        .filter_map(|b| b.text.as_ref())
        .cloned()
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}
