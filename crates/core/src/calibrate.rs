//! Ontology-grounded calibration — store anchor essays in the knowledge graph
//! and use structural similarity to calibrate new scores.
//!
//! This is where open-ontologies actually earns its keep for essay scoring:
//! the ontology stores what each score level LOOKS LIKE as structured evidence,
//! and new essays are scored by comparing their evidence profile to anchors.

use crate::adaptive_extract::{quick_adaptive_extract, AdaptiveEvidence};
use crate::benchmark::LabeledSample;
use open_ontologies::graph::GraphStore;
use std::sync::Arc;

/// An anchor essay stored in the ontology with its evidence profile.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnchorEssay {
    pub id: String,
    pub score: f64,
    pub evidence_profile: EvidenceProfile,
}

/// Structured evidence profile — what the ontology stores per anchor.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvidenceProfile {
    pub word_count: usize,
    pub num_counter_arguments: usize,
    pub num_complex_sentences: usize,
    pub num_cohesion_devices: usize,
    pub num_vocabulary_markers: usize,
    pub num_topic_sentences: usize,
    pub num_personal_voice: usize,
    pub num_rhetorical_devices: usize,
    pub has_thesis: bool,
    pub total_evidence: usize,
    pub avg_quality: f64,
}

impl EvidenceProfile {
    /// Build profile from adaptive extraction results.
    pub fn from_adaptive(items: &[AdaptiveEvidence], text: &str) -> Self {
        let count = |cat: &str| items.iter().filter(|i| i.category == cat).count();
        let total = items.len();
        let avg_q = if total > 0 {
            items.iter().map(|i| i.quality).sum::<f64>() / total as f64
        } else {
            0.0
        };

        Self {
            word_count: text.split_whitespace().count(),
            num_counter_arguments: count("counter_arguments"),
            num_complex_sentences: count("sentence_variety"),
            num_cohesion_devices: count("cohesion_devices"),
            num_vocabulary_markers: count("vocabulary_sophistication"),
            num_topic_sentences: count("topic_sentences"),
            num_personal_voice: count("personal_voice"),
            num_rhetorical_devices: count("rhetorical_devices"),
            has_thesis: count("argument_thesis") > 0,
            total_evidence: total,
            avg_quality: avg_q,
        }
    }

    /// Compute similarity to another profile (0.0-1.0).
    /// Uses cosine similarity over normalised feature vectors.
    pub fn similarity(&self, other: &EvidenceProfile) -> f64 {
        let features_self = self.as_vec();
        let features_other = other.as_vec();

        let dot: f64 = features_self
            .iter()
            .zip(&features_other)
            .map(|(a, b)| a * b)
            .sum();
        let mag_a: f64 = features_self.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mag_b: f64 = features_other.iter().map(|x| x * x).sum::<f64>().sqrt();

        if mag_a < 1e-10 || mag_b < 1e-10 {
            return 0.0;
        }
        (dot / (mag_a * mag_b)).clamp(0.0, 1.0)
    }

    fn as_vec(&self) -> Vec<f64> {
        vec![
            (self.word_count as f64 / 500.0).min(2.0),
            self.num_counter_arguments as f64,
            self.num_complex_sentences as f64 / 5.0,
            self.num_cohesion_devices as f64,
            self.num_vocabulary_markers as f64,
            self.num_topic_sentences as f64,
            self.num_personal_voice as f64,
            self.num_rhetorical_devices as f64 / 5.0,
            if self.has_thesis { 1.0 } else { 0.0 },
            self.avg_quality,
        ]
    }
}

/// Store anchor essays in the ontology graph.
pub fn load_anchors(graph: &Arc<GraphStore>, anchors: &[AnchorEssay]) -> anyhow::Result<usize> {
    let mut turtle = String::from(
        "@prefix cal: <http://brain-in-the-fish.dev/calibration/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n",
    );

    for anchor in anchors {
        let safe_id = anchor
            .id
            .replace(|c: char| !c.is_alphanumeric(), "_");
        let p = &anchor.evidence_profile;
        turtle.push_str(&format!(
            "cal:{safe_id} a cal:AnchorEssay ;\n\
             \tcal:score \"{:.2}\"^^xsd:decimal ;\n\
             \tcal:wordCount \"{}\"^^xsd:integer ;\n\
             \tcal:counterArguments \"{}\"^^xsd:integer ;\n\
             \tcal:complexSentences \"{}\"^^xsd:integer ;\n\
             \tcal:cohesionDevices \"{}\"^^xsd:integer ;\n\
             \tcal:vocabularyMarkers \"{}\"^^xsd:integer ;\n\
             \tcal:topicSentences \"{}\"^^xsd:integer ;\n\
             \tcal:personalVoice \"{}\"^^xsd:integer ;\n\
             \tcal:rhetoricalDevices \"{}\"^^xsd:integer ;\n\
             \tcal:hasThesis \"{}\"^^xsd:boolean ;\n\
             \tcal:totalEvidence \"{}\"^^xsd:integer ;\n\
             \tcal:avgQuality \"{:.4}\"^^xsd:decimal .\n\n",
            anchor.score,
            p.word_count,
            p.num_counter_arguments,
            p.num_complex_sentences,
            p.num_cohesion_devices,
            p.num_vocabulary_markers,
            p.num_topic_sentences,
            p.num_personal_voice,
            p.num_rhetorical_devices,
            p.has_thesis,
            p.total_evidence,
            p.avg_quality
        ));
    }

    graph.load_turtle(&turtle, None)
}

/// Parse an Oxigraph literal string like `"2.0"^^<http://...#decimal>` into f64.
fn parse_oxigraph_literal(raw: &str) -> f64 {
    // Format: "value"^^<datatype_iri>
    let trimmed = raw.trim_matches('"');
    if let Some(idx) = trimmed.find("\"^^") {
        trimmed[..idx]
            .trim_matches('"')
            .parse::<f64>()
            .unwrap_or(0.0)
    } else {
        trimmed.parse::<f64>().unwrap_or(0.0)
    }
}

/// Find the most similar anchor to a new essay's evidence profile using SPARQL.
pub fn find_nearest_anchor(
    graph: &Arc<GraphStore>,
    profile: &EvidenceProfile,
) -> anyhow::Result<Option<(String, f64, f64)>> {
    let query = r#"
        PREFIX cal: <http://brain-in-the-fish.dev/calibration/>
        SELECT ?anchor ?score ?wc ?ca ?cs ?cd ?vm ?ts ?pv ?rd ?ht ?te ?aq WHERE {
            ?anchor a cal:AnchorEssay ;
                cal:score ?score ;
                cal:wordCount ?wc ;
                cal:counterArguments ?ca ;
                cal:complexSentences ?cs ;
                cal:cohesionDevices ?cd ;
                cal:vocabularyMarkers ?vm ;
                cal:topicSentences ?ts ;
                cal:personalVoice ?pv ;
                cal:rhetoricalDevices ?rd ;
                cal:hasThesis ?ht ;
                cal:totalEvidence ?te ;
                cal:avgQuality ?aq .
        }
    "#;

    let result_json = graph.sparql_select(query)?;
    let parsed: serde_json::Value = serde_json::from_str(&result_json)?;
    let results = parsed
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut best: Option<(String, f64, f64)> = None;

    for row in &results {
        let obj = match row.as_object() {
            Some(o) => o,
            None => continue,
        };

        let get_f64 = |key: &str| -> f64 {
            obj.get(key)
                .and_then(|v| v.as_str())
                .map(parse_oxigraph_literal)
                .unwrap_or(0.0)
        };

        let anchor_profile = EvidenceProfile {
            word_count: get_f64("wc") as usize,
            num_counter_arguments: get_f64("ca") as usize,
            num_complex_sentences: get_f64("cs") as usize,
            num_cohesion_devices: get_f64("cd") as usize,
            num_vocabulary_markers: get_f64("vm") as usize,
            num_topic_sentences: get_f64("ts") as usize,
            num_personal_voice: get_f64("pv") as usize,
            num_rhetorical_devices: get_f64("rd") as usize,
            has_thesis: get_f64("ht") > 0.5,
            total_evidence: get_f64("te") as usize,
            avg_quality: get_f64("aq"),
        };

        let sim = profile.similarity(&anchor_profile);
        let score = get_f64("score");
        let anchor_id = obj
            .get("anchor")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        if best.is_none() || sim > best.as_ref().unwrap().2 {
            best = Some((anchor_id, score, sim));
        }
    }

    Ok(best)
}

/// Build anchor essays from labeled samples.
pub fn build_anchors(
    samples: &[LabeledSample],
    intent: &str,
    count_per_band: usize,
) -> Vec<AnchorEssay> {
    use std::collections::HashMap;

    // Group by score band (rounded to nearest 0.5)
    let mut bands: HashMap<i32, Vec<&LabeledSample>> = HashMap::new();
    for s in samples {
        let band = (s.expert_score * 2.0).round() as i32;
        bands.entry(band).or_default().push(s);
    }

    let mut anchors = Vec::new();
    for band_samples in bands.values() {
        for sample in band_samples.iter().take(count_per_band) {
            let items = quick_adaptive_extract(&sample.text, intent);
            anchors.push(AnchorEssay {
                id: sample.id.clone(),
                score: sample.expert_score,
                evidence_profile: EvidenceProfile::from_adaptive(&items, &sample.text),
            });
        }
    }

    anchors.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    anchors
}

/// Score a new essay by finding the nearest anchor and adjusting.
/// Returns: (calibrated_score, nearest_anchor_score, similarity)
pub fn calibrated_score(
    graph: &Arc<GraphStore>,
    text: &str,
    intent: &str,
    subagent_score: f64,
    max_score: f64,
) -> (f64, Option<f64>, f64) {
    let items = quick_adaptive_extract(text, intent);
    let profile = EvidenceProfile::from_adaptive(&items, text);

    match find_nearest_anchor(graph, &profile) {
        Ok(Some((_, anchor_score, similarity))) => {
            // Blend: pull subagent score toward nearest anchor score
            // weighted by similarity (high similarity = strong pull)
            let pull_strength = similarity * 0.3; // max 30% adjustment
            let adjustment = (anchor_score - subagent_score) * pull_strength;
            let calibrated = (subagent_score + adjustment).clamp(1.0, max_score);
            (calibrated, Some(anchor_score), similarity)
        }
        _ => (subagent_score, None, 0.0), // no anchors = no adjustment
    }
}

/// Generate a calibration summary for the shoal prompt.
pub fn calibration_prompt_section(
    graph: &Arc<GraphStore>,
    text: &str,
    intent: &str,
) -> String {
    let items = quick_adaptive_extract(text, intent);
    let profile = EvidenceProfile::from_adaptive(&items, text);

    match find_nearest_anchor(graph, &profile) {
        Ok(Some((_id, score, sim))) => {
            format!(
                "**Calibration anchor:** Most similar stored essay scored {:.1} \
                 (similarity: {:.0}%). Use this as a reference point — if this \
                 essay is similar quality, score near {:.1}.\n",
                score,
                sim * 100.0,
                score
            )
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_evidence_profile_from_text() {
        let text = "I believe that education is important. However, some people disagree. \
                    Furthermore, the evidence shows significant improvements. \
                    Because students learn better when engaged, we should invest more.";
        let items = quick_adaptive_extract(text, "grade this essay");
        let profile = EvidenceProfile::from_adaptive(&items, text);
        assert!(profile.has_thesis);
        assert!(profile.num_counter_arguments > 0);
        assert!(profile.num_cohesion_devices > 0);
        assert!(profile.word_count > 20);
    }

    #[test]
    fn test_profile_similarity() {
        let a = EvidenceProfile {
            word_count: 400,
            num_counter_arguments: 3,
            num_complex_sentences: 5,
            num_cohesion_devices: 2,
            num_vocabulary_markers: 1,
            num_topic_sentences: 4,
            num_personal_voice: 1,
            num_rhetorical_devices: 2,
            has_thesis: true,
            total_evidence: 18,
            avg_quality: 0.6,
        };
        let b = EvidenceProfile {
            word_count: 420,
            num_counter_arguments: 2,
            num_complex_sentences: 6,
            num_cohesion_devices: 3,
            num_vocabulary_markers: 1,
            num_topic_sentences: 4,
            num_personal_voice: 1,
            num_rhetorical_devices: 1,
            has_thesis: true,
            total_evidence: 18,
            avg_quality: 0.65,
        };
        let c = EvidenceProfile {
            word_count: 50,
            num_counter_arguments: 0,
            num_complex_sentences: 0,
            num_cohesion_devices: 0,
            num_vocabulary_markers: 0,
            num_topic_sentences: 1,
            num_personal_voice: 0,
            num_rhetorical_devices: 0,
            has_thesis: false,
            total_evidence: 1,
            avg_quality: 0.2,
        };

        let sim_ab = a.similarity(&b);
        let sim_ac = a.similarity(&c);
        assert!(
            sim_ab > sim_ac,
            "Similar profiles should have higher similarity: ab={} ac={}",
            sim_ab,
            sim_ac
        );
        assert!(sim_ab > 0.9, "Very similar profiles: {}", sim_ab);
    }

    #[test]
    fn test_load_and_query_anchors() {
        let graph = Arc::new(GraphStore::new());
        let anchors = vec![
            AnchorEssay {
                id: "anchor_low".into(),
                score: 2.0,
                evidence_profile: EvidenceProfile {
                    word_count: 100,
                    num_counter_arguments: 0,
                    num_complex_sentences: 1,
                    num_cohesion_devices: 0,
                    num_vocabulary_markers: 0,
                    num_topic_sentences: 1,
                    num_personal_voice: 0,
                    num_rhetorical_devices: 0,
                    has_thesis: false,
                    total_evidence: 2,
                    avg_quality: 0.3,
                },
            },
            AnchorEssay {
                id: "anchor_high".into(),
                score: 4.0,
                evidence_profile: EvidenceProfile {
                    word_count: 500,
                    num_counter_arguments: 3,
                    num_complex_sentences: 8,
                    num_cohesion_devices: 4,
                    num_vocabulary_markers: 2,
                    num_topic_sentences: 5,
                    num_personal_voice: 2,
                    num_rhetorical_devices: 3,
                    has_thesis: true,
                    total_evidence: 28,
                    avg_quality: 0.7,
                },
            },
        ];

        let triples = load_anchors(&graph, &anchors).unwrap();
        assert!(triples > 0, "Should load anchor triples");

        // Query: a weak profile should match anchor_low
        let weak_profile = EvidenceProfile {
            word_count: 80,
            num_counter_arguments: 0,
            num_complex_sentences: 0,
            num_cohesion_devices: 0,
            num_vocabulary_markers: 0,
            num_topic_sentences: 1,
            num_personal_voice: 0,
            num_rhetorical_devices: 0,
            has_thesis: false,
            total_evidence: 1,
            avg_quality: 0.2,
        };
        let result = find_nearest_anchor(&graph, &weak_profile).unwrap();
        assert!(result.is_some());
        let (_, score, _) = result.unwrap();
        assert!(
            (score - 2.0).abs() < 0.1,
            "Weak profile should match low anchor, got {}",
            score
        );
    }

    #[test]
    fn test_calibrated_score_adjusts() {
        let graph = Arc::new(GraphStore::new());
        let anchors = vec![AnchorEssay {
            id: "mid".into(),
            score: 3.0,
            evidence_profile: EvidenceProfile {
                word_count: 300,
                num_counter_arguments: 1,
                num_complex_sentences: 3,
                num_cohesion_devices: 1,
                num_vocabulary_markers: 0,
                num_topic_sentences: 3,
                num_personal_voice: 1,
                num_rhetorical_devices: 1,
                has_thesis: true,
                total_evidence: 10,
                avg_quality: 0.5,
            },
        }];
        load_anchors(&graph, &anchors).unwrap();

        // Subagent underscored at 2.0 but profile matches 3.0 anchor
        let text = "I believe education matters. However, some disagree. \
                    Furthermore, studies show improvement. Because students \
                    learn when engaged, we need investment.";
        let (cal, anchor, _sim) = calibrated_score(&graph, text, "grade essay", 2.0, 5.0);
        assert!(
            cal > 2.0,
            "Should pull score up toward anchor: cal={}",
            cal
        );
        assert!(anchor.is_some());
    }

    #[test]
    fn test_parse_oxigraph_literal() {
        assert!(
            (parse_oxigraph_literal("\"3.50\"^^<http://www.w3.org/2001/XMLSchema#decimal>") - 3.5)
                .abs()
                < 0.01
        );
        assert!((parse_oxigraph_literal("\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>") - 42.0).abs() < 0.01);
        assert!((parse_oxigraph_literal("42.0") - 42.0).abs() < 0.01);
    }

    #[test]
    fn test_build_anchors() {
        let samples = vec![
            LabeledSample {
                id: "s1".into(),
                text: "I believe this is important. However, others disagree.".into(),
                expert_score: 3.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
            LabeledSample {
                id: "s2".into(),
                text: "Short.".into(),
                expert_score: 1.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
        ];
        let anchors = build_anchors(&samples, "grade essay", 2);
        assert_eq!(anchors.len(), 2);
        // Should be sorted by score
        assert!(anchors[0].score <= anchors[1].score);
    }

    #[test]
    fn test_calibration_prompt_section() {
        let graph = Arc::new(GraphStore::new());
        let anchors = vec![AnchorEssay {
            id: "ref".into(),
            score: 4.0,
            evidence_profile: EvidenceProfile {
                word_count: 400,
                num_counter_arguments: 2,
                num_complex_sentences: 5,
                num_cohesion_devices: 3,
                num_vocabulary_markers: 1,
                num_topic_sentences: 4,
                num_personal_voice: 1,
                num_rhetorical_devices: 2,
                has_thesis: true,
                total_evidence: 18,
                avg_quality: 0.6,
            },
        }];
        load_anchors(&graph, &anchors).unwrap();

        let text = "I believe education is important. However, some disagree. \
                    Furthermore, studies demonstrate considerable improvement. \
                    Because learning works, we should invest.";
        let section = calibration_prompt_section(&graph, text, "grade essay");
        assert!(
            section.contains("Calibration anchor"),
            "Should produce calibration section: {}",
            section
        );
        assert!(section.contains("4.0"), "Should reference anchor score");
    }
}
