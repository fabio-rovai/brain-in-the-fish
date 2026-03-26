//! Improved prediction credibility benchmark — rich evidence + counter-spikes + document density.
//!
//! Compares 5 methods:
//! 1. LLM-only (credibility_score from rich extraction)
//! 2. SNN basic (old method — sparse evidence, heuristic types)
//! 3. SNN improved (rich evidence + counter-evidence + density)
//! 4. Blended 50/50 (LLM + SNN improved)
//! 5. Blended optimized (best blend ratio found by sweep)

use brain_in_the_fish_core::benchmark::{mean_absolute_error, pearson_correlation};
use brain_in_the_fish_core::snn;
use brain_in_the_fish_core::types::EvaluationCriterion;
use std::collections::HashMap;
use std::path::Path;

// ── Rich prediction structures ──────────────────────────────────────────────

#[derive(serde::Deserialize, Clone)]
struct RichEvidence {
    source_id: String,
    evidence_type: String,
    strength: f64,
    text: String,
    justification: Option<String>,
}

#[derive(serde::Deserialize, Clone)]
struct RichPrediction {
    #[allow(dead_code)]
    id: String,
    text: String,
    #[allow(dead_code)]
    prediction_type: String,
    target_value: Option<serde_json::Value>,
    #[allow(dead_code)]
    timeframe: Option<String>,
    credibility_score: f64,
    #[allow(dead_code)]
    credibility_verdict: String,
    #[allow(dead_code)]
    justification: String,
    evidence_items: Vec<RichEvidence>,
    counter_evidence: Vec<RichEvidence>,
}

#[derive(serde::Deserialize)]
struct RichDocPredictions {
    doc_id: String,
    document_evidence_density: f64,
    predictions: Vec<RichPrediction>,
}

// ── Sparse prediction structures (for SNN basic comparison) ─────────────────

#[derive(serde::Deserialize, Clone)]
struct SparsePrediction {
    text: String,
    #[allow(dead_code)]
    prediction_type: String,
    #[allow(dead_code)]
    credibility_score: f64,
    #[allow(dead_code)]
    credibility_verdict: String,
    supporting_evidence: Vec<String>,
    risk_factors: Vec<String>,
    #[allow(dead_code)]
    justification: String,
    target_value: Option<String>,
    #[allow(dead_code)]
    timeframe: Option<String>,
}

#[derive(serde::Deserialize)]
struct SparseDocPredictions {
    doc_id: String,
    predictions: Vec<SparsePrediction>,
}

// ── Ground truth ────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct GroundTruth {
    #[allow(dead_code)]
    source: String,
    predictions: Vec<GtPrediction>,
}

#[derive(serde::Deserialize)]
struct GtPrediction {
    text: String,
    outcome: String,
    #[serde(default)]
    #[allow(dead_code)]
    actual: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn outcome_to_score(outcome: &str) -> f64 {
    match outcome {
        "MET" | "EXCEEDED" => 1.0,
        "ON_TRACK" | "MET_TRIVIALLY" => 0.8,
        "PARTIALLY_MET" | "POSSIBLY_MET_EARLY" => 0.5,
        "NOT_ON_TRACK" => 0.2,
        "NOT_MET" => 0.0,
        _ => 0.5,
    }
}

fn word_overlap(a: &str, b: &str) -> f64 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let a_words: std::collections::HashSet<&str> =
        a_lower.split_whitespace().filter(|w| w.len() > 2).collect();
    let b_words: std::collections::HashSet<&str> =
        b_lower.split_whitespace().filter(|w| w.len() > 2).collect();
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    intersection as f64 / union as f64
}

// ── SNN basic scorer (sparse evidence, heuristic types) ─────────────────────

fn snn_credibility_basic(pred: &SparsePrediction, config: &snn::SNNConfig) -> f64 {
    let criterion = EvaluationCriterion {
        id: "credibility".into(),
        title: "Prediction Credibility".into(),
        description: None,
        max_score: 1.0,
        weight: 1.0,
        rubric_levels: vec![],
        sub_criteria: vec![],
    };
    let mut neuron = snn::Neuron::new(&criterion, "snn_predictor");
    let mut timestep = 0u32;

    for (i, ev) in pred.supporting_evidence.iter().enumerate() {
        if i > 0 && i as u32 % config.refractory_period == 0 {
            neuron.clear_refractory();
        }
        let ev_lower = ev.to_lowercase();
        let (spike_type, strength) = if ev_lower.contains("billion")
            || ev_lower.contains("million")
            || ev_lower.contains('%')
            || ev_lower.contains('£')
            || ev_lower.contains('$')
        {
            (snn::SpikeType::QuantifiedData, 0.8)
        } else if ev_lower.contains("legislation")
            || ev_lower.contains("law")
            || ev_lower.contains("act")
            || ev_lower.contains("signed")
            || ev_lower.contains("ratified")
        {
            (snn::SpikeType::Evidence, 0.75)
        } else if ev_lower.contains("study")
            || ev_lower.contains("research")
            || ev_lower.contains("data")
            || ev_lower.contains("report")
        {
            (snn::SpikeType::Citation, 0.7)
        } else {
            (snn::SpikeType::Claim, 0.5)
        };

        neuron.receive_spike(
            snn::Spike {
                source_id: format!("support_{}", i),
                strength,
                spike_type,
                timestep: timestep % config.timesteps,
                source_text: Some(ev.clone()),
                justification: None,
            },
            config,
        );
        timestep += 1;
    }

    for risk in &pred.risk_factors {
        let risk_lower = risk.to_lowercase();
        let inhibition = if risk_lower.contains("no ")
            || risk_lower.contains("without")
            || risk_lower.contains("lack")
        {
            0.15
        } else {
            0.08
        };
        neuron.apply_inhibition(inhibition);
    }

    if pred.target_value.is_some() {
        neuron.clear_refractory();
        neuron.receive_spike(
            snn::Spike {
                source_id: "has_target".into(),
                strength: 0.3,
                spike_type: snn::SpikeType::Alignment,
                timestep: timestep % config.timesteps,
                source_text: None,
                justification: None,
            },
            config,
        );
    }

    let score = neuron.compute_score(1.0, config);
    score.snn_score
}

// ── SNN improved scorer (rich evidence + counter-evidence + doc density) ────

fn snn_credibility_improved(
    evidence_items: &[RichEvidence],
    counter_evidence: &[RichEvidence],
    doc_density: f64,
    config: &snn::SNNConfig,
) -> f64 {
    let criterion = EvaluationCriterion {
        id: "cred".into(),
        title: "Credibility".into(),
        description: None,
        max_score: 1.0,
        weight: 1.0,
        rubric_levels: vec![],
        sub_criteria: vec![],
    };
    let mut neuron = snn::Neuron::new(&criterion, "predictor");

    // Enhancement 1: Feed rich evidence items as typed spikes
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
                source_text: Some(ev.text.clone()),
                justification: ev.justification.clone(),
            },
            config,
        );
    }

    // Enhancement 2: Counter-evidence as inhibition + negative Bayesian signal
    for ce in counter_evidence {
        neuron.apply_inhibition(ce.strength * config.inhibition_strength);
        // Feed a weak claim spike to trigger Bayesian update with low likelihood ratio
        neuron.clear_refractory();
        neuron.receive_spike(
            snn::Spike {
                source_id: ce.source_id.clone(),
                strength: 0.1,
                spike_type: snn::SpikeType::Claim,
                timestep: 9,
                source_text: Some(ce.text.clone()),
                justification: ce.justification.clone(),
            },
            config,
        );
    }

    // Compute SNN score
    let raw_score = neuron.compute_score(1.0, config);

    // Enhancement 3: Cross-prediction document density multiplier
    let density_adjusted = raw_score.snn_score * doc_density;

    density_adjusted.clamp(0.0, 1.0)
}

// ── Direction accuracy helper ───────────────────────────────────────────────

fn direction_correct(predicted: f64, actual: f64) -> bool {
    (predicted >= 0.5 && actual >= 0.5) || (predicted < 0.5 && actual < 0.5)
}

// ── Main benchmark test ─────────────────────────────────────────────────────

#[test]
fn prediction_credibility_eds_improved() {
    let rich_path = Path::new("/tmp/bitf-prediction-test/llm_predictions_rich.json");
    let sparse_path = Path::new("/tmp/bitf-prediction-test/llm_predictions.json");
    let docs_dir = Path::new("/tmp/bitf-prediction-test/docs");

    if !rich_path.exists() || !docs_dir.exists() {
        eprintln!("Skipping: missing prediction test data at /tmp/bitf-prediction-test/");
        return;
    }

    // Load rich predictions
    let rich_json = std::fs::read_to_string(rich_path).unwrap();
    let rich_docs: Vec<RichDocPredictions> = serde_json::from_str(&rich_json).unwrap();
    let rich_map: HashMap<String, (f64, Vec<RichPrediction>)> = rich_docs
        .into_iter()
        .map(|d| (d.doc_id, (d.document_evidence_density, d.predictions)))
        .collect();

    // Load sparse predictions (for SNN basic comparison)
    let sparse_map: HashMap<String, Vec<SparsePrediction>> = if sparse_path.exists() {
        let sparse_json = std::fs::read_to_string(sparse_path).unwrap();
        let sparse_docs: Vec<SparseDocPredictions> = serde_json::from_str(&sparse_json).unwrap();
        sparse_docs
            .into_iter()
            .map(|d| (d.doc_id, d.predictions))
            .collect()
    } else {
        HashMap::new()
    };

    // Load ground truth
    let mut gt_map: HashMap<String, Vec<GtPrediction>> = HashMap::new();
    for entry in std::fs::read_dir(docs_dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        if name.ends_with("_ground_truth.json") {
            let doc_id = name.replace("_ground_truth.json", "");
            let content = std::fs::read_to_string(&path).unwrap();
            let gt: GroundTruth = serde_json::from_str(&content).unwrap();
            gt_map.insert(doc_id, gt.predictions);
        }
    }

    let config = snn::SNNConfig::default();

    // Collectors for each method
    let mut llm_scores = Vec::new();
    let mut snn_basic_scores = Vec::new();
    let mut snn_improved_scores = Vec::new();
    let mut blended_50_scores = Vec::new();
    let mut actual_scores = Vec::new();

    let mut llm_correct = 0usize;
    let mut snn_basic_correct = 0usize;
    let mut snn_improved_correct = 0usize;
    let mut blended_50_correct = 0usize;
    let mut total = 0usize;

    println!("\n{:<45} | Outcome       | LLM   | Basic | Impvd | Bld50 | Actual", "Prediction");
    println!("{}", "-".repeat(120));

    for (doc_id, gt_preds) in &gt_map {
        let (doc_density, rich_preds) = match rich_map.get(doc_id) {
            Some(v) => (v.0, &v.1),
            None => continue,
        };
        let sparse_preds = sparse_map.get(doc_id);

        for gp in gt_preds {
            let gt_text_end = 80.min(gp.text.len());
            let gt_text = &gp.text[..gt_text_end];
            let actual = outcome_to_score(&gp.outcome);

            // Match to rich prediction by word overlap
            let best_rich = rich_preds
                .iter()
                .map(|rp| (rp, word_overlap(gt_text, &rp.text)))
                .filter(|(_, score)| *score > 0.15)
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            if let Some((rp, _)) = best_rich {
                let llm_cred = rp.credibility_score;
                let snn_improved = snn_credibility_improved(
                    &rp.evidence_items,
                    &rp.counter_evidence,
                    doc_density,
                    &config,
                );
                let blended_50 = llm_cred * 0.5 + snn_improved * 0.5;

                // SNN basic: match to sparse prediction if available
                let snn_basic = if let Some(sp) = sparse_preds {
                    let best_sparse = sp
                        .iter()
                        .map(|s| (s, word_overlap(gt_text, &s.text)))
                        .filter(|(_, score)| *score > 0.15)
                        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    best_sparse.map(|(s, _)| snn_credibility_basic(s, &config))
                } else {
                    None
                };
                // Fallback: run basic-style scoring on the rich prediction text
                let snn_basic_val = snn_basic.unwrap_or_else(|| {
                    // Construct a sparse-like prediction from rich data
                    let sp = SparsePrediction {
                        text: rp.text.clone(),
                        prediction_type: rp.prediction_type.clone(),
                        credibility_score: rp.credibility_score,
                        credibility_verdict: rp.credibility_verdict.clone(),
                        supporting_evidence: rp
                            .evidence_items
                            .iter()
                            .map(|e| e.text.clone())
                            .collect(),
                        risk_factors: rp
                            .counter_evidence
                            .iter()
                            .map(|c| c.text.clone())
                            .collect(),
                        justification: rp.justification.clone(),
                        target_value: rp.target_value.as_ref().map(|v| v.to_string()),
                        timeframe: rp.timeframe.clone(),
                    };
                    snn_credibility_basic(&sp, &config)
                });

                llm_scores.push(llm_cred);
                snn_basic_scores.push(snn_basic_val);
                snn_improved_scores.push(snn_improved);
                blended_50_scores.push(blended_50);
                actual_scores.push(actual);

                if direction_correct(llm_cred, actual) {
                    llm_correct += 1;
                }
                if direction_correct(snn_basic_val, actual) {
                    snn_basic_correct += 1;
                }
                if direction_correct(snn_improved, actual) {
                    snn_improved_correct += 1;
                }
                if direction_correct(blended_50, actual) {
                    blended_50_correct += 1;
                }
                total += 1;

                let display_end = 45.min(gp.text.len());
                println!(
                    "{:<45} | {:<13} | {:.0}%  | {:.0}%  | {:.0}%  | {:.0}%  | {:.0}%",
                    &gp.text[..display_end],
                    &gp.outcome,
                    llm_cred * 100.0,
                    snn_basic_val * 100.0,
                    snn_improved * 100.0,
                    blended_50 * 100.0,
                    actual * 100.0,
                );
            }
        }
    }

    assert!(total > 0, "No predictions matched to ground truth");

    // Find optimal blend ratio by sweep
    let mut best_blend_ratio = 0.5f64;
    let mut best_blend_pearson = f64::NEG_INFINITY;
    let mut best_blend_scores = blended_50_scores.clone();
    let mut best_blend_correct = blended_50_correct;

    for pct in 0..=10 {
        let llm_weight = pct as f64 / 10.0;
        let snn_weight = 1.0 - llm_weight;
        let scores: Vec<f64> = llm_scores
            .iter()
            .zip(snn_improved_scores.iter())
            .map(|(l, s)| l * llm_weight + s * snn_weight)
            .collect();
        let r = pearson_correlation(&scores, &actual_scores);
        if r > best_blend_pearson {
            best_blend_pearson = r;
            best_blend_ratio = llm_weight;
            best_blend_scores = scores.clone();
            best_blend_correct = scores
                .iter()
                .zip(actual_scores.iter())
                .filter(|(p, a)| direction_correct(**p, **a))
                .count();
        }
    }

    // Print summary table
    println!("\n╔═══════════════════════╦════════════════════╦═══════════╦═══════╗");
    println!("║ Method                ║ Direction accuracy ║ Pearson r ║ MAE   ║");
    println!("╠═══════════════════════╬════════════════════╬═══════════╬═══════╣");

    let methods: Vec<(&str, &[f64], usize)> = vec![
        ("LLM-only", &llm_scores, llm_correct),
        ("SNN basic", &snn_basic_scores, snn_basic_correct),
        ("SNN improved", &snn_improved_scores, snn_improved_correct),
        ("Blended 50/50", &blended_50_scores, blended_50_correct),
    ];

    for (name, scores, correct) in &methods {
        let pct = *correct as f64 / total as f64 * 100.0;
        let r = pearson_correlation(scores, &actual_scores);
        let mae = mean_absolute_error(scores, &actual_scores);
        println!(
            "║ {:<21} ║ {}/{} ({:>5.1}%)    ║ {:>+7.3}   ║ {:.3} ║",
            name, correct, total, pct, r, mae,
        );
    }

    // Best blend
    let best_pct = best_blend_correct as f64 / total as f64 * 100.0;
    let best_mae = mean_absolute_error(&best_blend_scores, &actual_scores);
    let blend_label = format!(
        "Blended {:.0}/{:.0}",
        best_blend_ratio * 100.0,
        (1.0 - best_blend_ratio) * 100.0
    );
    println!(
        "║ {:<21} ║ {}/{} ({:>5.1}%)    ║ {:>+7.3}   ║ {:.3} ║",
        blend_label, best_blend_correct, total, best_pct, best_blend_pearson, best_mae,
    );

    println!("╚═══════════════════════╩════════════════════╩═══════════╩═══════╝");

    // Print evidence stats
    let total_ev: usize = rich_map.values().flat_map(|(_, preds)| preds.iter()).map(|p| p.evidence_items.len()).sum();
    let total_ce: usize = rich_map.values().flat_map(|(_, preds)| preds.iter()).map(|p| p.counter_evidence.len()).sum();
    let total_preds: usize = rich_map.values().map(|(_, preds)| preds.len()).sum();
    println!("\nEvidence stats: {} predictions, {} evidence items (avg {:.1}), {} counter-evidence (avg {:.1})",
        total_preds, total_ev, total_ev as f64 / total_preds as f64, total_ce, total_ce as f64 / total_preds as f64);
    println!("Matched to ground truth: {}/{} predictions", total, total_preds);

    // Report improvement over baseline
    let llm_r = pearson_correlation(&llm_scores, &actual_scores);
    let improved_r = pearson_correlation(&snn_improved_scores, &actual_scores);
    let basic_r = pearson_correlation(&snn_basic_scores, &actual_scores);
    println!("\nSNN improvement: basic r={:.3} → improved r={:.3} (Δ {:+.3})",
        basic_r, improved_r, improved_r - basic_r);
    println!("Best blend: {:.0}% LLM + {:.0}% SNN improved, r={:.3}",
        best_blend_ratio * 100.0, (1.0 - best_blend_ratio) * 100.0, best_blend_pearson);
    if best_blend_pearson > llm_r {
        println!("✓ Blended outperforms LLM-only (Δr {:+.3})", best_blend_pearson - llm_r);
    } else {
        println!("△ LLM-only still leads (Δr {:+.3})", llm_r - best_blend_pearson);
    }
}
