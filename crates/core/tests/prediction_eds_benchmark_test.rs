//! Prediction credibility benchmark — LLM-extracted predictions scored via SNN.

use brain_in_the_fish_core::benchmark::{mean_absolute_error, pearson_correlation};
use brain_in_the_fish_core::snn;
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize)]
struct DocPredictions {
    doc_id: String,
    predictions: Vec<LlmPrediction>,
}

#[derive(serde::Deserialize, Clone)]
struct LlmPrediction {
    text: String,
    #[allow(dead_code)]
    prediction_type: String,
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

/// Convert an outcome string to a numerical score (1.0 = fully met, 0.0 = not met)
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

/// Score a prediction's credibility using the SNN.
/// Supporting evidence -> positive spikes, risk factors -> weak/inhibitory spikes.
fn snn_credibility(pred: &LlmPrediction, config: &snn::SNNConfig) -> f64 {
    let mut neuron = snn::Neuron::new(
        &brain_in_the_fish_core::types::EvaluationCriterion {
            id: "credibility".into(),
            title: "Prediction Credibility".into(),
            description: None,
            max_score: 1.0,
            weight: 1.0,
            rubric_levels: vec![],
            sub_criteria: vec![],
        },
        "snn_predictor",
    );

    let mut timestep = 0u32;

    // Supporting evidence -> positive spikes
    for (i, ev) in pred.supporting_evidence.iter().enumerate() {
        if i > 0 && i as u32 % config.refractory_period == 0 {
            neuron.clear_refractory();
        }
        // Determine spike type from evidence text
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

    // Risk factors -> inhibition
    for risk in &pred.risk_factors {
        let risk_lower = risk.to_lowercase();
        let inhibition = if risk_lower.contains("no ")
            || risk_lower.contains("without")
            || risk_lower.contains("lack")
        {
            0.15 // strong risk
        } else {
            0.08 // moderate risk
        };
        neuron.apply_inhibition(inhibition);
    }

    // Has target value? Slight boost if quantified
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

    // Compute score (max 1.0)
    let score = neuron.compute_score(1.0, config);
    score.snn_score
}

/// Simple word overlap matching (Jaccard on words > 2 chars)
fn word_overlap(a: &str, b: &str) -> f64 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let a_words: std::collections::HashSet<&str> = a_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    let b_words: std::collections::HashSet<&str> = b_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    intersection as f64 / union as f64
}

#[test]
fn prediction_credibility_eds_benchmark() {
    let pred_path = Path::new("/tmp/bitf-prediction-test/llm_predictions.json");
    let docs_dir = Path::new("/tmp/bitf-prediction-test/docs");

    if !pred_path.exists() || !docs_dir.exists() {
        eprintln!("Skipping: missing prediction test data");
        return;
    }

    let pred_json = std::fs::read_to_string(pred_path).unwrap();
    let doc_preds: Vec<DocPredictions> = serde_json::from_str(&pred_json).unwrap();
    let pred_map: HashMap<String, Vec<LlmPrediction>> = doc_preds
        .into_iter()
        .map(|d| (d.doc_id, d.predictions))
        .collect();

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

    let mut llm_scores = Vec::new();
    let mut snn_scores = Vec::new();
    let mut blended_scores = Vec::new();
    let mut actual_scores = Vec::new();

    let mut llm_correct = 0usize;
    let mut snn_correct = 0usize;
    let mut blended_correct = 0usize;
    let mut total = 0usize;

    println!(
        "\n{:<35} | Outcome    | LLM  | SNN  | Blend | LLM? | SNN? | Bld?",
        "Prediction"
    );
    println!("{}", "-".repeat(110));

    for (doc_id, gt_preds) in &gt_map {
        let llm_preds = match pred_map.get(doc_id) {
            Some(p) => p,
            None => continue,
        };

        for gp in gt_preds {
            let gt_text_end = 60.min(gp.text.len());
            let gt_text = &gp.text[..gt_text_end];
            let actual = outcome_to_score(&gp.outcome);

            // Match to LLM prediction by word overlap
            let best = llm_preds
                .iter()
                .map(|lp| (lp, word_overlap(gt_text, &lp.text)))
                .filter(|(_, score)| *score > 0.15)
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            if let Some((lp, _)) = best {
                let llm_cred = lp.credibility_score;
                let snn_cred = snn_credibility(lp, &config);
                let blended = llm_cred * 0.5 + snn_cred * 0.5;

                llm_scores.push(llm_cred);
                snn_scores.push(snn_cred);
                blended_scores.push(blended);
                actual_scores.push(actual);

                // Direction accuracy: skeptical (<0.5) of failures, supportive (>=0.5) of successes
                let llm_dir = (llm_cred >= 0.5 && actual >= 0.5)
                    || (llm_cred < 0.5 && actual < 0.5);
                let snn_dir = (snn_cred >= 0.5 && actual >= 0.5)
                    || (snn_cred < 0.5 && actual < 0.5);
                let bld_dir =
                    (blended >= 0.5 && actual >= 0.5) || (blended < 0.5 && actual < 0.5);

                if llm_dir {
                    llm_correct += 1;
                }
                if snn_dir {
                    snn_correct += 1;
                }
                if bld_dir {
                    blended_correct += 1;
                }
                total += 1;

                let display_end = 35.min(gp.text.len());
                println!(
                    "{:<35} | {:<10} | {:.0}%  | {:.0}%  | {:.0}%  | {}  | {}  | {}",
                    &gp.text[..display_end],
                    &gp.outcome,
                    llm_cred * 100.0,
                    snn_cred * 100.0,
                    blended * 100.0,
                    if llm_dir { "Y" } else { "N" },
                    if snn_dir { "Y" } else { "N" },
                    if bld_dir { "Y" } else { "N" },
                );
            }
        }
    }

    assert!(total > 0, "No predictions matched to ground truth");

    println!(
        "\n| Method  | Direction accuracy | Pearson r | MAE  |"
    );
    println!("|---------|-------------------|-----------|------|");
    println!(
        "| LLM     | {}/{} ({:.0}%)        | {:.3}     | {:.2} |",
        llm_correct,
        total,
        llm_correct as f64 / total as f64 * 100.0,
        pearson_correlation(&llm_scores, &actual_scores),
        mean_absolute_error(&llm_scores, &actual_scores)
    );
    println!(
        "| SNN     | {}/{} ({:.0}%)        | {:.3}     | {:.2} |",
        snn_correct,
        total,
        snn_correct as f64 / total as f64 * 100.0,
        pearson_correlation(&snn_scores, &actual_scores),
        mean_absolute_error(&snn_scores, &actual_scores)
    );
    println!(
        "| Blended | {}/{} ({:.0}%)        | {:.3}     | {:.2} |",
        blended_correct,
        total,
        blended_correct as f64 / total as f64 * 100.0,
        pearson_correlation(&blended_scores, &actual_scores),
        mean_absolute_error(&blended_scores, &actual_scores)
    );
}
