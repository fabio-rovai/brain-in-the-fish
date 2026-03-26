//! Benchmark: LLM-extracted evidence fed through EDS vs regex-extracted evidence.

use brain_in_the_fish_core::agent;
use brain_in_the_fish_core::alignment;
use brain_in_the_fish_core::benchmark;
use brain_in_the_fish_core::criteria;
use brain_in_the_fish_core::extract;
use brain_in_the_fish_core::moderation;
use brain_in_the_fish_core::snn;
use brain_in_the_fish_core::types::*;
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize)]
struct LlmEvidence {
    id: String,
    evidence: Vec<LlmEvidenceItem>,
}

#[derive(serde::Deserialize)]
struct LlmEvidenceItem {
    source_id: String,
    evidence_type: String,
    strength: f64,
    #[allow(dead_code)]
    text: String,
}

fn spike_type_from_str(s: &str) -> snn::SpikeType {
    match s {
        "quantified_data" => snn::SpikeType::QuantifiedData,
        "evidence" => snn::SpikeType::Evidence,
        "citation" => snn::SpikeType::Citation,
        "alignment" => snn::SpikeType::Alignment,
        _ => snn::SpikeType::Claim,
    }
}

/// Run EDS scoring with LLM-extracted evidence for one essay.
fn score_with_llm_evidence(
    evidence_items: &[LlmEvidenceItem],
    agents: &[EvaluatorAgent],
    framework: &EvaluationFramework,
    config: &snn::SNNConfig,
) -> f64 {
    let mut networks: Vec<snn::AgentNetwork> = agents
        .iter()
        .map(|a| snn::AgentNetwork::new(a, &framework.criteria))
        .collect();

    // Feed evidence into each agent's network, for each criterion
    for network in &mut networks {
        for neuron in &mut network.neurons {
            for (i, ev) in evidence_items.iter().enumerate() {
                if i > 0 && i as u32 % config.refractory_period == 0 {
                    neuron.clear_refractory();
                }
                neuron.receive_spike(
                    snn::Spike {
                        source_id: ev.source_id.clone(),
                        strength: ev.strength.clamp(0.0, 1.0),
                        spike_type: spike_type_from_str(&ev.evidence_type),
                        timestep: i as u32 % config.timesteps,
                    },
                    config,
                );
            }
        }
    }

    // Compute scores and moderate
    let mut all_scores = Vec::new();
    for network in &networks {
        let snn_scores = network.compute_scores(&framework.criteria, config);
        for (criterion_id, snn_score) in &snn_scores {
            let max_score = framework
                .criteria
                .iter()
                .find(|c| c.id == *criterion_id)
                .map(|c| c.max_score)
                .unwrap_or(10.0);
            all_scores.push(Score {
                agent_id: network.agent_id.clone(),
                criterion_id: criterion_id.clone(),
                score: snn_score.snn_score,
                max_score,
                round: 1,
                justification: String::new(),
                evidence_used: vec![],
                gaps_identified: vec![],
            });
        }
    }

    let moderated = moderation::calculate_moderated_scores(&all_scores, agents);
    let overall = moderation::calculate_overall_score(&moderated, framework);
    overall.percentage / 100.0 * 5.0 // Scale to 1.0-5.0 range (ELLIPSE max = 5.0)
}

/// Run EDS scoring with regex-extracted evidence for one essay (existing pipeline).
fn score_with_regex(
    sample: &benchmark::LabeledSample,
    agents: &[EvaluatorAgent],
    framework: &EvaluationFramework,
    config: &snn::SNNConfig,
) -> f64 {
    let mut doc = EvalDocument::new(format!("Benchmark: {}", sample.id), "essay".into());
    let word_count = sample.text.split_whitespace().count() as u32;
    let mut section = Section {
        id: uuid::Uuid::new_v4().to_string(),
        title: sample.id.clone(),
        text: sample.text.clone(),
        word_count,
        page_range: None,
        claims: vec![],
        evidence: vec![],
        subsections: vec![],
    };
    let extracted = extract::extract_all(&section.text);
    let (claims, evidence) = extract::to_claims_and_evidence(&extracted);
    section.claims = claims;
    section.evidence = evidence;
    doc.sections.push(section);
    doc.total_word_count = Some(word_count);

    let (alignments, _gaps) = alignment::align_sections_to_criteria(&doc, framework);

    let mut networks: Vec<snn::AgentNetwork> = agents
        .iter()
        .map(|a| snn::AgentNetwork::new(a, &framework.criteria))
        .collect();

    for network in &mut networks {
        network.feed_evidence(&doc, &alignments, config);
    }

    let mut all_scores = Vec::new();
    for network in &networks {
        let snn_scores = network.compute_scores(&framework.criteria, config);
        for (criterion_id, snn_score) in &snn_scores {
            let max_score = framework
                .criteria
                .iter()
                .find(|c| c.id == *criterion_id)
                .map(|c| c.max_score)
                .unwrap_or(10.0);
            all_scores.push(Score {
                agent_id: network.agent_id.clone(),
                criterion_id: criterion_id.clone(),
                score: snn_score.snn_score,
                max_score,
                round: 1,
                justification: String::new(),
                evidence_used: vec![],
                gaps_identified: vec![],
            });
        }
    }

    let moderated = moderation::calculate_moderated_scores(&all_scores, agents);
    let overall = moderation::calculate_overall_score(&moderated, framework);
    overall.percentage / 100.0 * sample.max_score
}

#[test]
fn test_llm_eds_vs_regex_eds() {
    // Load datasets — resolve relative to workspace root (two levels up from crate)
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let samples =
        benchmark::load_dataset(&workspace_root.join("data/ellipse-sample.json")).unwrap();

    let evidence_path = Path::new("/tmp/bitf-eds-test/llm_evidence.json");
    if !evidence_path.exists() {
        eprintln!(
            "Skipping LLM+EDS benchmark: no evidence file at {:?}",
            evidence_path
        );
        return;
    }

    let evidence_json = std::fs::read_to_string(evidence_path).unwrap();
    let llm_evidence: Vec<LlmEvidence> = serde_json::from_str(&evidence_json).unwrap();
    let evidence_map: HashMap<&str, &Vec<LlmEvidenceItem>> = llm_evidence
        .iter()
        .map(|e| (e.id.as_str(), &e.evidence))
        .collect();

    let intent = "academic essay evaluation";
    let framework = criteria::framework_for_intent(intent);
    let agents = agent::spawn_panel(intent, &framework);
    let config = snn::SNNConfig::default();

    let mut llm_predicted = Vec::new();
    let mut regex_predicted = Vec::new();
    let mut actual = Vec::new();

    println!(
        "\n{:<14} | Expert | LLM+EDS | Regex+EDS | LLM delta | Regex delta",
        "ID"
    );
    println!("{}", "-".repeat(80));

    for sample in &samples {
        let expert = sample.expert_score;
        actual.push(expert);

        // LLM+EDS
        let llm_score = if let Some(evidence) = evidence_map.get(sample.id.as_str()) {
            score_with_llm_evidence(evidence, &agents, &framework, &config)
        } else {
            0.0
        };
        llm_predicted.push(llm_score);

        // Regex+EDS
        let regex_score = score_with_regex(sample, &agents, &framework, &config);
        regex_predicted.push(regex_score);

        let id_display = if sample.id.len() > 12 {
            &sample.id[..12]
        } else {
            &sample.id
        };
        println!(
            "{:<14} | {:.1}    | {:.1}     | {:.1}       | {:.1}       | {:.1}",
            id_display,
            expert,
            llm_score,
            regex_score,
            (llm_score - expert).abs(),
            (regex_score - expert).abs(),
        );
    }

    let llm_pearson = benchmark::pearson_correlation(&llm_predicted, &actual);
    let llm_qwk = benchmark::quadratic_weighted_kappa(&llm_predicted, &actual, 0.0, 5.0);
    let llm_mae = benchmark::mean_absolute_error(&llm_predicted, &actual);

    let regex_pearson = benchmark::pearson_correlation(&regex_predicted, &actual);
    let regex_qwk = benchmark::quadratic_weighted_kappa(&regex_predicted, &actual, 0.0, 5.0);
    let regex_mae = benchmark::mean_absolute_error(&regex_predicted, &actual);

    println!("\n| Method    | Pearson r | QWK   | MAE  |");
    println!("|-----------|-----------|-------|------|");
    println!(
        "| LLM+EDS   | {:.3}     | {:.3} | {:.2} |",
        llm_pearson, llm_qwk, llm_mae
    );
    println!(
        "| Regex+EDS | {:.3}     | {:.3} | {:.2} |",
        regex_pearson, regex_qwk, regex_mae
    );

    // LLM+EDS should beat regex+EDS
    assert!(
        llm_pearson > regex_pearson || (llm_pearson - regex_pearson).abs() < 0.05,
        "LLM+EDS Pearson ({:.3}) should be >= Regex+EDS ({:.3})",
        llm_pearson, regex_pearson
    );
}
