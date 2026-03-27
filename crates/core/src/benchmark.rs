//! Benchmark framework — measure evaluation accuracy against ground truth.
//!
//! Loads labeled datasets, runs the pipeline, computes metrics.
//! Ablation experiments prove each component earns its complexity.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single labeled sample: document text + expert score.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabeledSample {
    pub id: String,
    pub text: String,
    pub expert_score: f64,
    pub max_score: f64,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub rubric: String,
}

/// Results from a benchmark run.
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResults {
    pub name: String,
    pub samples: usize,
    pub pearson_r: f64,
    pub qwk: f64,
    pub mae: f64,
    pub nmae: f64,
    pub rmse: f64,
    pub mean_predicted: f64,
    pub mean_actual: f64,
    pub hallucination_count: usize,
    pub hallucination_rate: f64,
    pub config: BenchmarkConfig,
}

/// Which components are enabled for this run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub use_snn: bool,
    pub use_ontology_alignment: bool,
    pub use_debate: bool,
    pub use_validation: bool,
    pub use_llm_scoring: bool,
    pub use_llm_extraction: bool,
    pub label: String,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            use_snn: true,
            use_ontology_alignment: true,
            use_debate: true,
            use_validation: true,
            use_llm_scoring: false, // default to deterministic
            use_llm_extraction: false,
            label: "full_pipeline".into(),
        }
    }
}

/// Load labeled samples from a JSON file.
/// Format: array of { id, text, expert_score, max_score, domain?, rubric? }
pub fn load_dataset(path: &Path) -> anyhow::Result<Vec<LabeledSample>> {
    let content = std::fs::read_to_string(path)?;
    let samples: Vec<LabeledSample> = serde_json::from_str(&content)?;
    Ok(samples)
}

/// Compute Pearson correlation coefficient.
pub fn pearson_correlation(predicted: &[f64], actual: &[f64]) -> f64 {
    assert_eq!(predicted.len(), actual.len());
    let n = predicted.len() as f64;
    if n < 2.0 {
        return 0.0;
    }

    let mean_p = predicted.iter().sum::<f64>() / n;
    let mean_a = actual.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_p = 0.0;
    let mut var_a = 0.0;

    for i in 0..predicted.len() {
        let dp = predicted[i] - mean_p;
        let da = actual[i] - mean_a;
        cov += dp * da;
        var_p += dp * dp;
        var_a += da * da;
    }

    let denom = (var_p * var_a).sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        cov / denom
    }
}

/// Compute Quadratic Weighted Kappa.
/// Measures agreement between two raters accounting for chance.
pub fn quadratic_weighted_kappa(
    predicted: &[f64],
    actual: &[f64],
    min_score: f64,
    max_score: f64,
) -> f64 {
    assert_eq!(predicted.len(), actual.len());
    let n = predicted.len();
    if n == 0 {
        return 0.0;
    }

    // Discretize to integer scores
    let num_ratings = (max_score - min_score) as usize + 1;

    // Build confusion matrix
    let mut observed = vec![vec![0.0f64; num_ratings]; num_ratings];
    for i in 0..n {
        let p = ((predicted[i] - min_score).round() as usize).min(num_ratings - 1);
        let a = ((actual[i] - min_score).round() as usize).min(num_ratings - 1);
        observed[p][a] += 1.0;
    }

    // Normalize
    let total = n as f64;
    for row in &mut observed {
        for cell in row.iter_mut() {
            *cell /= total;
        }
    }

    // Expected matrix (outer product of marginals)
    let mut row_sums = vec![0.0f64; num_ratings];
    let mut col_sums = vec![0.0f64; num_ratings];
    for i in 0..num_ratings {
        for j in 0..num_ratings {
            row_sums[i] += observed[i][j];
            col_sums[j] += observed[i][j];
        }
    }

    let mut expected = vec![vec![0.0f64; num_ratings]; num_ratings];
    for i in 0..num_ratings {
        for j in 0..num_ratings {
            expected[i][j] = row_sums[i] * col_sums[j];
        }
    }

    // Weight matrix (quadratic)
    let mut weights = vec![vec![0.0f64; num_ratings]; num_ratings];
    for (i, row) in weights.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            let diff = i as f64 - j as f64;
            *cell = diff * diff / ((num_ratings - 1) as f64).powi(2);
        }
    }

    // Kappa
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..num_ratings {
        for j in 0..num_ratings {
            num += weights[i][j] * observed[i][j];
            den += weights[i][j] * expected[i][j];
        }
    }

    if den < 1e-12 {
        1.0
    } else {
        1.0 - num / den
    }
}

/// Compute Mean Absolute Error.
pub fn mean_absolute_error(predicted: &[f64], actual: &[f64]) -> f64 {
    assert_eq!(predicted.len(), actual.len());
    if predicted.is_empty() {
        return 0.0;
    }
    let sum: f64 = predicted
        .iter()
        .zip(actual.iter())
        .map(|(p, a)| (p - a).abs())
        .sum();
    sum / predicted.len() as f64
}

/// Compute Root Mean Square Error.
pub fn rmse(predicted: &[f64], actual: &[f64]) -> f64 {
    assert_eq!(predicted.len(), actual.len());
    if predicted.is_empty() {
        return 0.0;
    }
    let sum: f64 = predicted
        .iter()
        .zip(actual.iter())
        .map(|(p, a)| (p - a).powi(2))
        .sum();
    (sum / predicted.len() as f64).sqrt()
}

/// Standard ablation configs for experimentation.
pub fn ablation_configs() -> Vec<BenchmarkConfig> {
    vec![
        BenchmarkConfig {
            label: "full_pipeline".into(),
            ..Default::default()
        },
        BenchmarkConfig {
            use_snn: false,
            label: "no_snn".into(),
            ..Default::default()
        },
        BenchmarkConfig {
            use_ontology_alignment: false,
            label: "no_alignment".into(),
            ..Default::default()
        },
        BenchmarkConfig {
            use_debate: false,
            label: "no_debate".into(),
            ..Default::default()
        },
        BenchmarkConfig {
            use_validation: false,
            label: "no_validation".into(),
            ..Default::default()
        },
    ]
}

/// Format results as a markdown table.
pub fn results_table(results: &[BenchmarkResults]) -> String {
    let mut t = String::from(
        "| Config | N | Pearson r | QWK | MAE | NMAE | RMSE | Halluc. | Halluc. Rate |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in results {
        t.push_str(&format!(
            "| {} | {} | {:.3} | {:.3} | {:.2} | {:.3} | {:.2} | {} | {:.1}% |\n",
            r.name, r.samples, r.pearson_r, r.qwk, r.mae, r.nmae, r.rmse,
            r.hallucination_count, r.hallucination_rate * 100.0
        ));
    }
    t
}

/// Compute per-group (per-rubric) benchmark results from samples and predictions.
/// Groups samples by rubric field, computes metrics for each group.
pub fn per_group_results(
    samples: &[LabeledSample],
    predicted: &[f64],
) -> Vec<BenchmarkResults> {
    use std::collections::BTreeMap;

    assert_eq!(samples.len(), predicted.len());

    // Group indices by rubric
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, sample) in samples.iter().enumerate() {
        let key = if sample.rubric.is_empty() {
            "default".to_string()
        } else {
            sample.rubric.clone()
        };
        groups.entry(key).or_default().push(i);
    }

    let mut results = Vec::new();
    for (rubric, indices) in &groups {
        if indices.len() < 2 {
            continue;
        }
        let pred: Vec<f64> = indices.iter().map(|&i| predicted[i]).collect();
        let actual: Vec<f64> = indices.iter().map(|&i| samples[i].expert_score).collect();
        let max_score_val = indices
            .iter()
            .map(|&i| samples[i].max_score)
            .fold(0.0f64, f64::max);
        let score_range = max_score_val; // min is always 0

        let pearson_r = pearson_correlation(&pred, &actual);
        let qwk = quadratic_weighted_kappa(&pred, &actual, 0.0, max_score_val);
        let mae_val = mean_absolute_error(&pred, &actual);
        let nmae = if score_range > 0.0 {
            mae_val / score_range
        } else {
            0.0
        };
        let rmse_val = rmse(&pred, &actual);
        let mean_predicted = pred.iter().sum::<f64>() / pred.len() as f64;
        let mean_actual = actual.iter().sum::<f64>() / actual.len() as f64;

        // Hallucination detection
        let mut hallucination_count = 0usize;
        for &i in indices {
            let ms = samples[i].max_score;
            if ms > 0.0 {
                let normalized_pred = predicted[i] / ms;
                let normalized_actual = samples[i].expert_score / ms;
                if (normalized_pred - normalized_actual).abs() > 0.3 {
                    hallucination_count += 1;
                }
            }
        }
        let hallucination_rate = hallucination_count as f64 / indices.len() as f64;

        results.push(BenchmarkResults {
            name: format!("rubric:{}", rubric),
            samples: indices.len(),
            pearson_r,
            qwk,
            mae: mae_val,
            nmae,
            rmse: rmse_val,
            mean_predicted,
            mean_actual,
            hallucination_count,
            hallucination_rate,
            config: BenchmarkConfig::default(),
        });
    }
    results
}

/// Create a small synthetic benchmark dataset for testing.
/// Returns 10 samples with realistic essay texts and scores.
pub fn synthetic_dataset() -> Vec<LabeledSample> {
    vec![
        LabeledSample {
            id: "syn_01".into(),
            text: "Quantitative easing (QE) was introduced by the Bank of England in 2009 as a response to the global financial crisis. According to Joyce et al. (2012), the initial round of QE reduced long-term gilt yields by approximately 100 basis points. This essay argues that while QE was necessary as an emergency measure, its prolonged use has created significant distributional consequences that undermine its net benefit. The Bank of England's own analysis (Haldane, 2014) estimates that QE increased household wealth by £600 billion, but this wealth accrued disproportionately to the top 5% of asset holders. Furthermore, Bernanke (2020) notes that the transmission mechanism from asset prices to real economic activity remains weak. In conclusion, QE achieved its primary objective of preventing deflation but failed to address the structural weaknesses in the UK economy that necessitated its use.".into(),
            expert_score: 7.5, max_score: 10.0, domain: "economics".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_02".into(),
            text: "QE is when the government prints money. It was used after the financial crisis. Some people think it was good and some think it was bad. The economy got better after QE so it probably worked. Banks got more money which they could lend to people. In conclusion QE was a good policy.".into(),
            expert_score: 3.0, max_score: 10.0, domain: "economics".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_03".into(),
            text: "The impact of quantitative easing on income inequality represents a critical but under-examined dimension of monetary policy evaluation. While traditional analysis focuses on macroeconomic aggregates such as GDP growth and inflation targeting (Svensson, 2003), this essay contends that a comprehensive assessment must incorporate distributional effects. Drawing on Piketty's (2014) framework of capital accumulation, I demonstrate that QE systematically advantages holders of financial assets over wage earners. The Gini coefficient for wealth inequality in the UK increased from 0.62 to 0.68 between 2010 and 2016 (ONS, 2017), a period coinciding with active QE programmes. However, counterfactual analysis by Bunn et al. (2018) suggests that without QE, unemployment would have risen by an additional 1.5 percentage points, disproportionately affecting lower-income households. This tension between asset price inflation and employment effects reveals the fundamental trade-off at the heart of unconventional monetary policy.".into(),
            expert_score: 8.5, max_score: 10.0, domain: "economics".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_04".into(),
            text: "This essay will discuss quantitative easing and its effects on the economy. Quantitative easing is a monetary policy tool. The Bank of England used it. It involves buying government bonds. The aim is to increase money supply. There are arguments for and against. Some economists support it. Others are critical. The evidence is mixed. Overall it had some positive effects but also some negative ones.".into(),
            expert_score: 4.0, max_score: 10.0, domain: "economics".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_05".into(),
            text: "Evaluating QE requires distinguishing between its immediate crisis-management function and its longer-term structural implications. In the acute phase (2009-2012), QE served as an effective circuit-breaker: Kapetanios et al. (2012) estimate it prevented a GDP decline of 1.5% and Bridges and Thomas (2012) find it reduced corporate bond spreads by 70-150 basis points. The mechanism operated primarily through the portfolio balance channel rather than the bank lending channel, a finding confirmed across multiple studies (Christensen and Rudebusch, 2012; Joyce et al., 2011). However, the transition from emergency to sustained policy (2012-2022) raises distinct concerns. First, the diminishing marginal returns: each successive round of QE produced smaller yield reductions (Meaning et al., 2017). Second, the financial stability risks from persistently low rates: Turner (2016) argues this fuelled speculative asset bubbles and increased systemic fragility. Third, the fiscal dominance concern: with the Bank holding over 30% of outstanding gilts, the boundary between monetary and fiscal policy became blurred (Goodhart, 2020).".into(),
            expert_score: 9.0, max_score: 10.0, domain: "economics".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_06".into(),
            text: "Climate change is the biggest challenge facing humanity today. Everyone knows that temperatures are rising and ice caps are melting. We need to do something about it immediately. Renewable energy is the solution. Solar and wind power can replace fossil fuels completely. Some people disagree but they are wrong. The science is settled. Governments should ban all fossil fuels immediately.".into(),
            expert_score: 2.5, max_score: 10.0, domain: "environmental".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_07".into(),
            text: "The efficacy of carbon taxation as a climate mitigation instrument depends critically on design parameters that are often glossed over in policy debates. Nordhaus (2017) advocates a gradually escalating carbon price starting at $30/tonne CO2, while Stern (2006) argues for immediate higher pricing reflecting the social cost of carbon. This divergence stems from differing discount rates: Nordhaus applies a market-based 5% rate while Stern uses an ethical 1.4% rate. Empirical evidence from British Columbia's carbon tax, implemented in 2008, shows a 5-15% reduction in fuel consumption without measurable GDP impact (Murray and Rivers, 2015). However, the distributional effects remain contested: Metcalf (2019) finds carbon taxes are regressive without revenue recycling, while Klenert et al. (2018) demonstrate that dividend recycling can make them progressive.".into(),
            expert_score: 8.0, max_score: 10.0, domain: "environmental".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_08".into(),
            text: "Healthcare systems around the world face increasing pressure from aging populations and rising costs. The NHS in the UK provides universal coverage funded through taxation. This model has advantages and disadvantages compared to insurance-based systems like in the United States or Germany. Some studies show that single-payer systems achieve better health outcomes per dollar spent (Commonwealth Fund, 2021). Other research suggests that market competition drives innovation. The debate continues about which approach is best. In my opinion, universal coverage is a human right and all countries should provide it.".into(),
            expert_score: 5.0, max_score: 10.0, domain: "health_policy".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_09".into(),
            text: "Artificial intelligence is amazing and will change everything. It can do lots of things that humans do. ChatGPT can write essays and answer questions. Self-driving cars will make roads safer. AI in healthcare will cure diseases. However some people worry about job losses. This is understandable but technology always creates new jobs. In the past, the industrial revolution created more jobs than it destroyed. The same will happen with AI.".into(),
            expert_score: 3.5, max_score: 10.0, domain: "technology".into(), rubric: "academic".into(),
        },
        LabeledSample {
            id: "syn_10".into(),
            text: "The deployment of large language models in educational assessment raises fundamental questions about construct validity that the NLP community has inadequately addressed. When Automated Essay Scoring (AES) systems evaluate student writing, they operationalize 'writing quality' through proxy features: lexical sophistication (Kyle and Crossley, 2015), syntactic complexity (Lu, 2010), and discourse coherence (Crossley and McNamara, 2016). However, these proxies may reward surface features at the expense of genuine argumentation quality. Perelman (2014) famously demonstrated that nonsensical essays with sophisticated vocabulary received high scores from commercial AES systems. More recent work by Uto et al. (2023) shows that fine-tuned LLMs achieve QWK scores of 0.75-0.85 against human raters, comparable to inter-rater reliability, but this agreement may reflect shared biases rather than valid assessment. The implications for fairness are significant: Bridgeman et al. (2012) found that AES systems consistently underscored essays by non-native English speakers who made cogent arguments with simpler syntax.".into(),
            expert_score: 9.5, max_score: 10.0, domain: "education".into(), rubric: "academic".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pearson_perfect_correlation() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r = pearson_correlation(&a, &b);
        assert!(
            (r - 1.0).abs() < 0.001,
            "Perfect correlation should be 1.0, got {}",
            r
        );
    }

    #[test]
    fn test_pearson_negative_correlation() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let r = pearson_correlation(&a, &b);
        assert!(
            (r - (-1.0)).abs() < 0.001,
            "Perfect negative should be -1.0, got {}",
            r
        );
    }

    #[test]
    fn test_pearson_no_correlation() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![3.0, 3.0, 3.0, 3.0, 3.0];
        let r = pearson_correlation(&a, &b);
        assert!(r.abs() < 0.001, "No variance should give 0, got {}", r);
    }

    #[test]
    fn test_qwk_perfect_agreement() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let k = quadratic_weighted_kappa(&a, &b, 1.0, 5.0);
        assert!(
            (k - 1.0).abs() < 0.001,
            "Perfect agreement should be 1.0, got {}",
            k
        );
    }

    #[test]
    fn test_qwk_partial_agreement() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![2.0, 2.0, 3.0, 4.0, 4.0]; // close but not perfect
        let k = quadratic_weighted_kappa(&a, &b, 1.0, 5.0);
        assert!(
            k > 0.5 && k < 1.0,
            "Partial agreement kappa should be 0.5-1.0, got {}",
            k
        );
    }

    #[test]
    fn test_mae() {
        let predicted = vec![3.0, 5.0, 7.0];
        let actual = vec![2.0, 5.0, 8.0];
        let m = mean_absolute_error(&predicted, &actual);
        assert!((m - 0.6667).abs() < 0.01, "MAE should be ~0.67, got {}", m);
    }

    #[test]
    fn test_rmse() {
        let predicted = vec![3.0, 5.0, 7.0];
        let actual = vec![2.0, 5.0, 8.0];
        let r = rmse(&predicted, &actual);
        assert!(
            (r - 0.8165).abs() < 0.01,
            "RMSE should be ~0.82, got {}",
            r
        );
    }

    #[test]
    fn test_synthetic_dataset() {
        let data = synthetic_dataset();
        assert_eq!(data.len(), 10);
        for sample in &data {
            assert!(sample.expert_score > 0.0 && sample.expert_score <= sample.max_score);
            assert!(!sample.text.is_empty());
        }
    }

    #[test]
    fn test_ablation_configs() {
        let configs = ablation_configs();
        assert_eq!(configs.len(), 5);
        assert!(configs[0].use_snn); // full pipeline
        assert!(!configs[1].use_snn); // no_snn
    }

    #[test]
    fn test_results_table() {
        let results = vec![BenchmarkResults {
            name: "test".into(),
            samples: 10,
            pearson_r: 0.85,
            qwk: 0.78,
            mae: 0.5,
            nmae: 0.05,
            rmse: 0.7,
            mean_predicted: 6.0,
            mean_actual: 6.2,
            hallucination_count: 2,
            hallucination_rate: 0.2,
            config: BenchmarkConfig::default(),
        }];
        let table = results_table(&results);
        assert!(table.contains("0.850"));
        assert!(table.contains("0.780"));
        assert!(table.contains("0.050"));
        assert!(table.contains("20.0%"));
    }
}
