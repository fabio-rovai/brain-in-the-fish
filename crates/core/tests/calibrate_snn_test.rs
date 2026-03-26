//! SNN self-calibration benchmark — optimize weights against expert scores.
//!
//! Loads ELLIPSE 45 essays, runs the deterministic EDS pipeline with
//! configurable SNN weights, and uses Nelder-Mead to find the weights
//! that maximize Pearson correlation with expert scores.

use brain_in_the_fish_core::agent;
use brain_in_the_fish_core::benchmark::{
    load_dataset, mean_absolute_error, pearson_correlation, rmse,
};
use brain_in_the_fish_core::criteria;
use brain_in_the_fish_core::moderation;
use brain_in_the_fish_core::optimize::nelder_mead;
use brain_in_the_fish_core::shoal::eds_score_essay_with_config;
use brain_in_the_fish_core::snn::{self, SNNConfig, ScoreWeights};
use brain_in_the_fish_core::types::*;
use std::collections::HashMap;
use std::path::Path;

// --- LLM evidence types ---

#[derive(serde::Deserialize)]
struct LlmEvidence {
    id: String,
    evidence: Vec<LlmEvidenceItem>,
}

#[derive(serde::Deserialize, Clone)]
struct LlmEvidenceItem {
    source_id: String,
    evidence_type: String,
    strength: f64,
    #[allow(dead_code)]
    text: String,
}

// --- LLM evidence scoring helpers ---

fn score_essay_llm(
    evidence_items: &[LlmEvidenceItem],
    agents: &[EvaluatorAgent],
    framework: &EvaluationFramework,
    config: &snn::SNNConfig,
) -> f64 {
    let mut networks: Vec<snn::AgentNetwork> = agents
        .iter()
        .map(|a| snn::AgentNetwork::new(a, &framework.criteria))
        .collect();

    for network in &mut networks {
        for neuron in &mut network.neurons {
            for (i, ev) in evidence_items.iter().enumerate() {
                if i > 0 && i as u32 % config.refractory_period == 0 {
                    neuron.clear_refractory();
                }
                let spike_type = match ev.evidence_type.as_str() {
                    "quantified_data" => snn::SpikeType::QuantifiedData,
                    "evidence" => snn::SpikeType::Evidence,
                    "citation" => snn::SpikeType::Citation,
                    "alignment" => snn::SpikeType::Alignment,
                    _ => snn::SpikeType::Claim,
                };
                neuron.receive_spike(
                    snn::Spike {
                        source_id: ev.source_id.clone(),
                        strength: ev.strength.clamp(0.0, 1.0),
                        spike_type,
                        timestep: i as u32 % config.timesteps,
                    },
                    config,
                );
            }
        }
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
    overall.percentage / 100.0 * 5.0 // ELLIPSE scale
}

fn objective_llm(
    params: &[f64],
    essay_data: &[(Vec<LlmEvidenceItem>, f64)],
    agents: &[EvaluatorAgent],
    framework: &EvaluationFramework,
) -> f64 {
    let weights = snn::ScoreWeights::from_params(&params[..9]);
    let mut config = snn::SNNConfig::default();
    config.weights = weights;
    config.decay_rate = params[9].clamp(0.01, 0.5);

    let mut predicted = Vec::new();
    let mut actual = Vec::new();
    for (evidence, expert_score) in essay_data {
        let score = score_essay_llm(evidence, agents, framework, &config);
        predicted.push(score);
        actual.push(*expert_score);
    }

    let range = predicted.iter().cloned().fold(f64::INFINITY, f64::min)
        ..=predicted.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if (range.end() - range.start()).abs() < 1e-10 {
        return 1.0;
    }

    1.0 - pearson_correlation(&predicted, &actual)
}

/// Score all essays with given SNN config, return (predicted, actual) vectors.
fn score_all(
    samples: &[brain_in_the_fish_core::benchmark::LabeledSample],
    config: &SNNConfig,
    intent: &str,
) -> (Vec<f64>, Vec<f64>) {
    let mut predicted = Vec::new();
    let mut actual = Vec::new();

    for sample in samples {
        let eds_pct = eds_score_essay_with_config(sample, intent, config);
        // Convert from 0.0-1.0 percentage to the essay's scale
        let score = eds_pct * sample.max_score;
        predicted.push(score);
        actual.push(sample.expert_score);
    }

    (predicted, actual)
}

/// Objective function: 1.0 - Pearson correlation (we minimize this).
fn objective(params: &[f64], samples: &[brain_in_the_fish_core::benchmark::LabeledSample]) -> f64 {
    let weights = ScoreWeights::from_params(&params[..9]);
    let mut config = SNNConfig::default();
    config.weights = weights;
    config.decay_rate = params[9].clamp(0.01, 0.5);

    let (predicted, actual) = score_all(samples, &config, "grade this essay");

    // If all predictions are the same, Pearson is undefined — return worst case
    let pred_range = predicted.iter().cloned().fold(f64::INFINITY, f64::min)
        ..=predicted.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if (pred_range.end() - pred_range.start()).abs() < 1e-10 {
        return 1.0;
    }

    let pearson = pearson_correlation(&predicted, &actual);
    1.0 - pearson // minimize
}

#[test]
fn calibrate_snn_weights() {
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/ellipse-sample.json");

    if !data_path.exists() {
        eprintln!("ELLIPSE sample data not found at {:?}, skipping", data_path);
        return;
    }

    let samples = load_dataset(&data_path).expect("Failed to load ELLIPSE sample data");
    assert_eq!(samples.len(), 45, "Expected 45 ELLIPSE essays");

    // --- Baseline: default weights ---
    let default_config = SNNConfig::default();
    let (pred_default, actual) = score_all(&samples, &default_config, "grade this essay");
    let baseline_pearson = pearson_correlation(&pred_default, &actual);
    let baseline_mae = mean_absolute_error(&pred_default, &actual);
    let baseline_rmse = rmse(&pred_default, &actual);

    println!("\n=== SNN Self-Calibration Benchmark ===\n");
    println!("Dataset: ELLIPSE 45 essays");
    println!("Score range: 1.0 - 5.0\n");
    println!("--- Baseline (default weights) ---");
    println!("  Pearson r:  {:.4}", baseline_pearson);
    println!("  MAE:        {:.4}", baseline_mae);
    println!("  RMSE:       {:.4}", baseline_rmse);
    println!(
        "  Pred range: {:.2} - {:.2}",
        pred_default.iter().cloned().fold(f64::INFINITY, f64::min),
        pred_default.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    println!(
        "  Mean pred:  {:.4}  Mean actual: {:.4}",
        pred_default.iter().sum::<f64>() / pred_default.len() as f64,
        actual.iter().sum::<f64>() / actual.len() as f64,
    );

    // --- Optimize: Nelder-Mead ---
    let default_weights = ScoreWeights::default();
    let mut initial_params = default_weights.to_params();
    initial_params.push(default_config.decay_rate); // param[9] = decay_rate

    let (best_params, best_loss) = nelder_mead(
        &|p: &[f64]| objective(p, &samples),
        &initial_params,
        2000,
        1e-8,
    );

    let optimized_weights = ScoreWeights::from_params(&best_params[..9]);
    let mut optimized_config = SNNConfig::default();
    optimized_config.weights = optimized_weights;
    optimized_config.decay_rate = best_params[9].clamp(0.01, 0.5);

    let (pred_optimized, _) = score_all(&samples, &optimized_config, "grade this essay");
    let opt_pearson = pearson_correlation(&pred_optimized, &actual);
    let opt_mae = mean_absolute_error(&pred_optimized, &actual);
    let opt_rmse = rmse(&pred_optimized, &actual);

    println!("\n--- Optimized weights (Nelder-Mead, 2000 iters) ---");
    println!("  Pearson r:  {:.4}", opt_pearson);
    println!("  MAE:        {:.4}", opt_mae);
    println!("  RMSE:       {:.4}", opt_rmse);
    println!(
        "  Pred range: {:.2} - {:.2}",
        pred_optimized.iter().cloned().fold(f64::INFINITY, f64::min),
        pred_optimized.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    println!("  Loss (1-r): {:.6}", best_loss);

    println!("\n--- Optimized ScoreWeights ---");
    println!("  w_saturation:  {:.4}", best_params[0].clamp(0.01, 1.0));
    println!("  w_quality:     {:.4}", best_params[1].clamp(0.01, 1.0));
    println!("  w_firing:      {:.4}", best_params[2].clamp(0.0, 1.0));
    println!("  saturation_base: {:.4}", best_params[3].clamp(2.0, 100.0));
    println!("  lr_quantified: {:.4}", best_params[4].clamp(1.0, 10.0));
    println!("  lr_evidence:   {:.4}", best_params[5].clamp(1.0, 10.0));
    println!("  lr_citation:   {:.4}", best_params[6].clamp(1.0, 10.0));
    println!("  lr_alignment:  {:.4}", best_params[7].clamp(1.0, 10.0));
    println!("  lr_claim:      {:.4}", best_params[8].clamp(1.0, 5.0));
    println!("  decay_rate:    {:.4}", best_params[9].clamp(0.01, 0.5));

    println!("\n--- Improvement ---");
    let delta_r = opt_pearson - baseline_pearson;
    let delta_mae = baseline_mae - opt_mae;
    println!(
        "  Pearson: {:.4} -> {:.4} ({:+.4})",
        baseline_pearson, opt_pearson, delta_r
    );
    println!(
        "  MAE:     {:.4} -> {:.4} ({:+.4})",
        baseline_mae, opt_mae, delta_mae
    );
    println!(
        "  RMSE:    {:.4} -> {:.4} ({:+.4})",
        baseline_rmse, opt_rmse, baseline_rmse - opt_rmse
    );

    // The optimized Pearson should be at least as good as baseline
    assert!(
        opt_pearson >= baseline_pearson - 0.01,
        "Optimized Pearson ({:.4}) should not be worse than baseline ({:.4})",
        opt_pearson,
        baseline_pearson,
    );

    println!("\n=== Calibration complete ===");
}

#[test]
fn test_default_config_produces_scores() {
    // Sanity check: default config should produce non-zero scores
    // for at least some essays.
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/ellipse-sample.json");

    if !data_path.exists() {
        return;
    }

    let samples = load_dataset(&data_path).expect("Failed to load data");
    let config = SNNConfig::default();
    let (predicted, _) = score_all(&samples.iter().take(5).cloned().collect::<Vec<_>>(), &config, "grade this essay");

    let non_zero = predicted.iter().filter(|&&p| p > 0.0).count();
    assert!(
        non_zero > 0,
        "At least some essays should produce non-zero scores: {:?}",
        predicted
    );
}

#[test]
fn calibrate_snn_weights_llm_evidence() {
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/ellipse-sample.json");
    let evidence_path = Path::new("/tmp/bitf-eds-test/llm_evidence.json");

    if !data_path.exists() || !evidence_path.exists() {
        eprintln!("Skipping: missing data files");
        return;
    }

    let samples = load_dataset(&data_path).unwrap();
    let evidence_json = std::fs::read_to_string(evidence_path).unwrap();
    let llm_evidence: Vec<LlmEvidence> = serde_json::from_str(&evidence_json).unwrap();
    let evidence_map: HashMap<String, Vec<LlmEvidenceItem>> = llm_evidence
        .into_iter()
        .map(|e| (e.id, e.evidence))
        .collect();

    let intent = "academic essay evaluation";
    let framework = criteria::framework_for_intent(intent);
    let agents = agent::spawn_panel(intent, &framework);

    // Build (evidence, expert_score) pairs
    let essay_data: Vec<(Vec<LlmEvidenceItem>, f64)> = samples
        .iter()
        .filter_map(|s| evidence_map.get(&s.id).map(|ev| (ev.clone(), s.expert_score)))
        .collect();

    println!(
        "\nCalibrating SNN weights with LLM evidence ({} essays)...",
        essay_data.len()
    );

    // Score with DEFAULT weights
    let default_config = snn::SNNConfig::default();
    let mut default_pred = Vec::new();
    let mut actual_scores = Vec::new();
    for (evidence, expert_score) in &essay_data {
        default_pred.push(score_essay_llm(
            evidence,
            &agents,
            &framework,
            &default_config,
        ));
        actual_scores.push(*expert_score);
    }
    let default_pearson = pearson_correlation(&default_pred, &actual_scores);
    let default_mae = mean_absolute_error(&default_pred, &actual_scores);

    println!(
        "Default:   Pearson {:.3} | MAE {:.2}",
        default_pearson, default_mae
    );

    // Optimize
    let initial = snn::ScoreWeights::default().to_params();
    let mut initial_with_decay = initial.clone();
    initial_with_decay.push(0.1); // decay_rate

    let agents_clone = agents.clone();
    let framework_clone = framework.clone();
    let essay_data_clone = essay_data.clone();

    let (best_params, best_loss) = nelder_mead(
        &|params: &[f64]| objective_llm(params, &essay_data_clone, &agents_clone, &framework_clone),
        &initial_with_decay,
        3000,
        1e-6,
    );

    println!(
        "Optimized: Pearson {:.3} | Loss {:.4}",
        1.0 - best_loss,
        best_loss
    );

    // Score with OPTIMIZED weights
    let opt_weights = snn::ScoreWeights::from_params(&best_params[..9]);
    let mut opt_config = snn::SNNConfig::default();
    opt_config.weights = opt_weights.clone();
    opt_config.decay_rate = best_params[9].clamp(0.01, 0.5);

    let mut opt_pred = Vec::new();
    for (evidence, _) in &essay_data {
        opt_pred.push(score_essay_llm(
            evidence,
            &agents,
            &framework,
            &opt_config,
        ));
    }
    let opt_pearson = pearson_correlation(&opt_pred, &actual_scores);
    let opt_mae = mean_absolute_error(&opt_pred, &actual_scores);
    let opt_qwk = brain_in_the_fish_core::benchmark::quadratic_weighted_kappa(
        &opt_pred,
        &actual_scores,
        0.0,
        5.0,
    );

    println!("\n| Method             | Pearson r | QWK   | MAE  |");
    println!("|---------------------|-----------|-------|------|");
    println!(
        "| LLM+EDS default     | {:.3}     | -     | {:.2} |",
        default_pearson, default_mae
    );
    println!(
        "| LLM+EDS calibrated  | {:.3}     | {:.3} | {:.2} |",
        opt_pearson, opt_qwk, opt_mae
    );

    // Print optimized weights
    println!("\nOptimized weights:");
    println!("  w_saturation:  {:.3}", opt_weights.w_saturation);
    println!("  w_quality:     {:.3}", opt_weights.w_quality);
    println!("  w_firing:      {:.3}", opt_weights.w_firing);
    println!("  saturation_base: {:.1}", opt_weights.saturation_base);
    println!("  lr_quantified: {:.2}", opt_weights.lr_quantified);
    println!("  lr_evidence:   {:.2}", opt_weights.lr_evidence);
    println!("  lr_citation:   {:.2}", opt_weights.lr_citation);
    println!("  lr_alignment:  {:.2}", opt_weights.lr_alignment);
    println!("  lr_claim:      {:.2}", opt_weights.lr_claim);
    println!("  decay_rate:    {:.3}", opt_config.decay_rate);

    assert!(
        opt_pearson >= default_pearson - 0.01,
        "Calibrated should not be worse: {:.3} vs {:.3}",
        opt_pearson,
        default_pearson
    );
}
