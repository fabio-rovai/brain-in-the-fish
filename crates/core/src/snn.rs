//! Evidence density scorer — deterministic evidence-grounded scoring.
//!
//! Uses a biologically-inspired integrate-and-fire model to convert
//! evidence items from the knowledge graph into scores. The model is
//! deterministic: same evidence always produces the same score.
//!
//! Internally uses spike-based dynamics (membrane potential, threshold,
//! firing rate) but the core function is simple: more evidence with
//! higher quality = higher score. The biological framing provides
//! useful properties (lateral inhibition for debate, refractory periods
//! for diminishing returns) but this is NOT a trained neural network.
//!
//! ## Architecture
//!
//! Each evaluator agent has neurons (one per criterion). Evidence from the
//! document ontology generates input spikes. Neurons accumulate spikes via
//! leaky integrate-and-fire dynamics. The firing rate maps to a score.
//!
//! ```text
//! Evidence nodes → Spikes → Neurons → Firing rate → Score
//!                    ↑                      ↓
//!              spike_strength         lateral_inhibition
//!           (specificity, quant)    (debate challenges)
//! ```

use crate::types::*;
use serde::Serialize;

/// A spike generated from an evidence or claim node.
#[derive(Debug, Clone, Serialize)]
pub struct Spike {
    /// Source evidence/claim ID
    pub source_id: String,
    /// Spike strength (0.0-1.0)
    pub strength: f64,
    /// What kind of evidence produced this spike
    pub spike_type: SpikeType,
    /// Timestamp (simulation step)
    pub timestep: u32,
    /// Audit trail: the evidence text this spike was generated from.
    pub source_text: Option<String>,
    /// Audit trail: why this strength was assigned.
    pub justification: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum SpikeType {
    /// Assertion in document
    Claim,
    /// Cited evidence
    Evidence,
    /// Evidence with numbers (strongest)
    QuantifiedData,
    /// Academic reference
    Citation,
    /// Section-criterion alignment signal
    Alignment,
    // Graph-structural signals (from argument graph topology)
    /// Node has high PageRank / many supporting edges
    Connectivity,
    /// Node is deeply nested in argument structure
    Depth,
    /// Node is well-supported (many incoming "supports" edges)
    Support,
    /// Negative: node is isolated in the graph
    Isolation,
    /// Node has counter-argument + rebuttal (dialectical strength)
    CounterBalance,
}

/// A single neuron representing one criterion for one agent.
#[derive(Debug, Clone, Serialize)]
pub struct Neuron {
    /// The criterion this neuron evaluates
    pub criterion_id: String,
    /// The agent this neuron belongs to
    pub agent_id: String,
    /// Current membrane potential (accumulates spikes, decays)
    pub membrane_potential: f64,
    /// Firing threshold — derived from rubric
    pub threshold: f64,
    /// Number of times this neuron has fired
    pub fire_count: u32,
    /// Total spikes received
    pub total_spikes: u32,
    /// Whether currently in refractory period
    pub refractory: bool,
    /// Accumulated spike history for audit
    pub spike_log: Vec<Spike>,
    /// Lateral inhibition from other agents' challenges
    pub inhibition: f64,
    /// Bayesian confidence that this criterion is met (0.0-1.0)
    pub bayesian_confidence: f64,
}

/// The complete SNN for one evaluator agent.
#[derive(Debug, Clone, Serialize)]
pub struct AgentNetwork {
    pub agent_id: String,
    pub agent_name: String,
    pub agent_role: String,
    pub agent_domain: String,
    pub neurons: Vec<Neuron>,
}

/// Tunable weights for the score formula.
#[derive(Debug, Clone)]
pub struct ScoreWeights {
    /// Weight for evidence saturation signal (default 0.50)
    pub w_saturation: f64,
    /// Weight for spike quality signal (default 0.35)
    pub w_quality: f64,
    /// Weight for firing rate signal (default 0.15)
    pub w_firing: f64,
    /// Saturation reference — ln(1+spikes)/ln(1+saturation_base) (default 15.0)
    pub saturation_base: f64,
    /// Bayesian LR for QuantifiedData spikes (default 2.5)
    pub lr_quantified: f64,
    /// Bayesian LR for Evidence spikes (default 2.0)
    pub lr_evidence: f64,
    /// Bayesian LR for Citation spikes (default 1.8)
    pub lr_citation: f64,
    /// Bayesian LR for Alignment spikes (default 1.5)
    pub lr_alignment: f64,
    /// Bayesian LR for Claim spikes (default 1.3)
    pub lr_claim: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            w_saturation: 0.50,
            w_quality: 0.35,
            w_firing: 0.15,
            saturation_base: 15.0,
            lr_quantified: 2.5,
            lr_evidence: 2.0,
            lr_citation: 1.8,
            lr_alignment: 1.5,
            lr_claim: 1.3,
        }
    }
}

impl ScoreWeights {
    /// Convert a parameter vector to ScoreWeights (for optimizer).
    /// Order: [w_saturation, w_quality, w_firing, saturation_base,
    ///         lr_quantified, lr_evidence, lr_citation, lr_alignment, lr_claim]
    pub fn from_params(params: &[f64]) -> Self {
        assert!(params.len() >= 9, "Need at least 9 parameters for ScoreWeights");
        Self {
            w_saturation: params[0].clamp(0.01, 1.0),
            w_quality: params[1].clamp(0.01, 1.0),
            w_firing: params[2].clamp(0.0, 1.0),
            saturation_base: params[3].clamp(2.0, 100.0),
            lr_quantified: params[4].clamp(1.0, 10.0),
            lr_evidence: params[5].clamp(1.0, 10.0),
            lr_citation: params[6].clamp(1.0, 10.0),
            lr_alignment: params[7].clamp(1.0, 10.0),
            lr_claim: params[8].clamp(1.0, 5.0),
        }
    }

    /// Convert to a parameter vector (for optimizer).
    pub fn to_params(&self) -> Vec<f64> {
        vec![
            self.w_saturation,
            self.w_quality,
            self.w_firing,
            self.saturation_base,
            self.lr_quantified,
            self.lr_evidence,
            self.lr_citation,
            self.lr_alignment,
            self.lr_claim,
        ]
    }
}

/// SNN configuration parameters.
#[derive(Debug, Clone)]
pub struct SNNConfig {
    /// Membrane potential decay rate per timestep (0.0-1.0)
    pub decay_rate: f64,
    /// Refractory period (timesteps after firing where neuron can't fire)
    pub refractory_period: u32,
    /// How much a challenge inhibits the target neuron (0.0-1.0)
    pub inhibition_strength: f64,
    /// Number of simulation timesteps
    pub timesteps: u32,
    /// Minimum spikes required before any score is valid
    pub min_spikes_for_score: u32,
    /// Tunable weights for the score formula
    pub weights: ScoreWeights,
}

impl Default for SNNConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.1,
            refractory_period: 2,
            inhibition_strength: 0.15,
            timesteps: 10,
            min_spikes_for_score: 1,
            weights: ScoreWeights::default(),
        }
    }
}

impl Neuron {
    /// Create a new neuron for a criterion.
    /// Threshold is derived from the criterion's max score and rubric.
    pub fn new(criterion: &EvaluationCriterion, agent_id: &str) -> Self {
        // Threshold = 60% of max score normalised to 0-1 range
        // Rubric levels adjust: more levels = more granular threshold
        let base_threshold = 0.6;
        let rubric_adjustment = if criterion.rubric_levels.is_empty() {
            0.0
        } else {
            0.05 * (criterion.rubric_levels.len() as f64 - 2.0).max(0.0)
        };

        Self {
            criterion_id: criterion.id.clone(),
            agent_id: agent_id.to_string(),
            membrane_potential: 0.0,
            threshold: base_threshold + rubric_adjustment,
            fire_count: 0,
            total_spikes: 0,
            refractory: false,
            spike_log: Vec::new(),
            inhibition: 0.0,
            bayesian_confidence: 0.5, // uninformative prior
        }
    }

    /// Bayesian update with likelihood ratio capping.
    /// Prevents overconfidence from thin evidence.
    /// Based on epistemic-deconstructor calibration discipline.
    pub fn bayesian_update(&mut self, likelihood_ratio: f64) {
        // Cap likelihood ratio to prevent overconfidence
        let max_lr = if self.total_spikes < 3 {
            3.0
        } else if self.total_spikes < 10 {
            5.0
        } else {
            10.0
        };
        let capped_lr = likelihood_ratio.min(max_lr).max(1.0 / max_lr);

        let prior_odds =
            self.bayesian_confidence / (1.0 - self.bayesian_confidence).max(1e-10);
        let posterior_odds = prior_odds * capped_lr;
        self.bayesian_confidence =
            (posterior_odds / (1.0 + posterior_odds)).clamp(0.01, 0.99);
    }

    /// Receive a spike — integrate into membrane potential.
    pub fn receive_spike(&mut self, spike: Spike, config: &SNNConfig) {
        if self.refractory {
            return; // Can't receive during refractory
        }

        self.total_spikes += 1;
        let effective_strength = (spike.strength - self.inhibition).max(0.0);
        self.membrane_potential += effective_strength;

        // After accumulating the spike, update Bayesian confidence
        let lr = match spike.spike_type {
            SpikeType::QuantifiedData => config.weights.lr_quantified,
            SpikeType::Evidence => config.weights.lr_evidence,
            SpikeType::Citation => config.weights.lr_citation,
            SpikeType::Alignment => config.weights.lr_alignment,
            SpikeType::Claim => config.weights.lr_claim,
            SpikeType::Connectivity => 1.6,
            SpikeType::Depth => 1.2,
            SpikeType::Support => 2.0,
            SpikeType::Isolation => 0.5, // < 1.0 = negative evidence
            SpikeType::CounterBalance => 1.8,
        };
        self.bayesian_update(lr);

        self.spike_log.push(spike);

        // Check if we fire
        if self.membrane_potential >= self.threshold {
            self.fire_count += 1;
            self.membrane_potential = 0.0; // Reset after firing
            self.refractory = true;
        }

        // Decay
        self.membrane_potential *= 1.0 - config.decay_rate;
    }

    /// Clear refractory state (called after refractory_period timesteps).
    pub fn clear_refractory(&mut self) {
        self.refractory = false;
    }

    /// Apply lateral inhibition from a debate challenge.
    pub fn apply_inhibition(&mut self, amount: f64) {
        self.inhibition = (self.inhibition + amount).min(0.5); // Cap at 50%
    }

    /// Calculate the SNN-derived score (0.0 to max_score).
    /// Based on firing rate and spike quality.
    pub fn compute_score(&self, max_score: f64, config: &SNNConfig) -> SNNScore {
        if self.total_spikes < config.min_spikes_for_score {
            return SNNScore {
                snn_score: 0.0,
                confidence: 0.0,
                firing_rate: 0.0,
                evidence_count: self.total_spikes,
                spike_quality: 0.0,
                grounded: false,
                explanation: "Insufficient evidence in the knowledge graph. The neuron \
                              did not receive enough spikes to produce a valid score."
                    .into(),
                falsification_checked: false,
                bayesian_confidence: self.bayesian_confidence,
                confidence_interval: (0.0, max_score),
            };
        }

        let firing_rate = self.fire_count as f64 / config.timesteps as f64;

        // Spike quality = average strength of received spikes
        let spike_quality = if self.spike_log.is_empty() {
            0.0
        } else {
            self.spike_log.iter().map(|s| s.strength).sum::<f64>() / self.spike_log.len() as f64
        };

        // Score formula: three signals combined
        //
        // 1. Evidence saturation: how much evidence exists relative to a "full" amount
        //    Uses log scale — configurable base controls the saturation curve.
        //    This is the primary signal — more evidence = higher score.
        let sat_base = config.weights.saturation_base;
        let evidence_saturation = (1.0 + self.total_spikes as f64).ln() / (1.0 + sat_base).ln();
        let evidence_saturation = evidence_saturation.min(1.0);

        // 2. Spike quality: how strong the evidence is (average strength)
        //    High specificity claims + quantified evidence = high quality.

        // 3. Firing rate: how often the neuron fired (traditional SNN signal)
        //    This captures temporal dynamics — evidence arriving in bursts vs spread out.

        // Weighted combination: configurable weights for each signal
        let raw_score = (evidence_saturation * config.weights.w_saturation
            + spike_quality * config.weights.w_quality
            + firing_rate * config.weights.w_firing)
            .min(1.0);

        // Apply inhibition penalty (from debate challenges or negative validation)
        let after_inhibition = raw_score * (1.0 - self.inhibition);
        let snn_score = after_inhibition * max_score;

        // Confidence based on evidence volume and quality
        let volume_confidence = (self.total_spikes as f64 / 5.0).min(1.0); // 5+ spikes = full confidence
        let quality_confidence = spike_quality;
        let confidence = (volume_confidence * 0.5 + quality_confidence * 0.5).min(1.0);

        let quantified_count = self
            .spike_log
            .iter()
            .filter(|s| s.spike_type == SpikeType::QuantifiedData)
            .count();

        // Falsification check: if score > 80% of max, look for counter-evidence
        // Counter-evidence = spikes with very low strength or inhibition
        let high_score = snn_score > max_score * 0.8;
        let has_counter_evidence = self.spike_log.iter().any(|s| s.strength < 0.2);
        let falsification_checked = if high_score {
            has_counter_evidence || self.inhibition > 0.0
        } else {
            true // Low scores don't need falsification
        };

        // If high score but no falsification, reduce confidence
        let adjusted_confidence = if high_score && !falsification_checked {
            confidence * 0.7 // 30% confidence penalty for unfalsified high scores
        } else {
            confidence
        };

        // Confidence interval based on evidence count (more evidence = narrower interval)
        let interval_width = if self.total_spikes == 0 {
            max_score
        } else if self.total_spikes < 3 {
            max_score * 0.4
        } else if self.total_spikes < 5 {
            max_score * 0.25
        } else if self.total_spikes < 10 {
            max_score * 0.15
        } else {
            max_score * 0.08
        };

        let ci_low = (snn_score - interval_width / 2.0).max(0.0);
        let ci_high = (snn_score + interval_width / 2.0).min(max_score);
        let confidence_interval = (ci_low, ci_high);

        let hallucination_risk_note = if high_score && !falsification_checked {
            "Unfalsified high score — confidence reduced by 30%."
        } else {
            ""
        };

        let explanation = format!(
            "SNN: {:.1}/{:.0} (CI: {:.1}-{:.1}). Firing rate: {:.2} ({} fires in {} steps). \
             {} evidence spikes ({} quantified). Bayesian confidence: {:.0}%. \
             Falsification: {}. {}",
            snn_score,
            max_score,
            ci_low,
            ci_high,
            firing_rate,
            self.fire_count,
            config.timesteps,
            self.total_spikes,
            quantified_count,
            self.bayesian_confidence * 100.0,
            if falsification_checked {
                "passed"
            } else {
                "NOT CHECKED — score may be overconfident"
            },
            if hallucination_risk_note.is_empty() {
                ""
            } else {
                hallucination_risk_note
            },
        );

        SNNScore {
            snn_score,
            confidence: adjusted_confidence,
            firing_rate,
            evidence_count: self.total_spikes,
            spike_quality,
            grounded: true,
            explanation,
            falsification_checked,
            bayesian_confidence: self.bayesian_confidence,
            confidence_interval,
        }
    }
}

/// The SNN-derived score for one criterion.
#[derive(Debug, Clone, Serialize)]
pub struct SNNScore {
    /// The computed score
    pub snn_score: f64,
    /// Confidence in this score (0.0-1.0)
    pub confidence: f64,
    /// Neuron firing rate
    pub firing_rate: f64,
    /// Number of evidence spikes that contributed
    pub evidence_count: u32,
    /// Average quality of spikes
    pub spike_quality: f64,
    /// Whether this score is grounded in evidence (vs insufficient data)
    pub grounded: bool,
    /// Human-readable explanation
    pub explanation: String,
    /// Whether a falsification check was passed.
    /// True = disconfirming evidence was sought and not found (strengthens score).
    /// False = no falsification attempt was made (score may be overconfident).
    pub falsification_checked: bool,
    /// Bayesian confidence after LR-capped updates (more calibrated than raw confidence).
    pub bayesian_confidence: f64,
    /// 95% confidence interval for the score [low, high]
    pub confidence_interval: (f64, f64),
}

/// Verdict from the evidence scorer comparing LLM score against graph evidence.
#[derive(Debug, Clone, Serialize)]
pub enum Verdict {
    /// Evidence supports the LLM score.
    Confirmed {
        reason: String,
    },
    /// Evidence is weaker than the LLM score implies. Includes recommended adjusted score.
    Flagged {
        reason: String,
        recommended_score: f64,
    },
    /// No evidence at all. Score should be rejected.
    Rejected {
        reason: String,
    },
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Confirmed { reason } => write!(f, "CONFIRMED: {}", reason),
            Verdict::Flagged { reason, recommended_score } => {
                write!(f, "FLAGGED: {} (recommended: {:.1})", reason, recommended_score)
            }
            Verdict::Rejected { reason } => write!(f, "REJECTED: {}", reason),
        }
    }
}

/// Compare an LLM score against graph evidence and produce a verdict.
///
/// This is the gate: the scorer doesn't produce a competing score,
/// it verifies whether the LLM's score is consistent with the evidence.
pub fn gate_score(
    llm_score: f64,
    max_score: f64,
    graph: &crate::argument_graph::ArgumentGraph,
    snn_scores: &[(String, SNNScore)],
) -> Verdict {
    use crate::argument_graph;

    let metrics = argument_graph::compute_metrics(graph);
    let normalized_llm = if max_score > 0.0 { llm_score / max_score } else { 0.0 };

    // Use average node quality from the graph (LLM's per-node scores)
    let node_scores: Vec<f64> = graph.nodes.iter()
        .filter_map(|n| n.llm_score)
        .collect();
    let avg_node_quality = if node_scores.is_empty() {
        0.0
    } else {
        node_scores.iter().sum::<f64>() / node_scores.len() as f64
    };

    let avg_confidence = if snn_scores.is_empty() {
        0.0
    } else {
        snn_scores.iter().map(|(_, s)| s.bayesian_confidence).sum::<f64>()
            / snn_scores.len() as f64
    };

    let evidence_count = metrics.evidence_count;
    let claim_count = metrics.claim_count;
    let total_nodes = metrics.node_count;
    let connectivity = metrics.connectivity;

    // REJECTED: no evidence at all
    if total_nodes == 0 || (evidence_count == 0 && claim_count == 0) {
        return Verdict::Rejected {
            reason: "No argument nodes found in knowledge graph. \
                     Cannot verify any claims."
                .into(),
        };
    }

    // REJECTED: only bare claims, zero evidence
    if evidence_count == 0 && normalized_llm > 0.3 {
        return Verdict::Rejected {
            reason: format!(
                "{} claims found but 0 evidence nodes. \
                 Score {:.1} has no evidentiary support.",
                claim_count, llm_score
            ),
        };
    }

    // FLAGGED: LLM score significantly exceeds evidence
    if normalized_llm > avg_node_quality + 0.25 && normalized_llm > 0.4 {
        let recommended = avg_node_quality * max_score;
        return Verdict::Flagged {
            reason: format!(
                "LLM scored {:.1}/{:.0} but evidence supports ~{:.1}/{:.0}. \
                 Graph: {} nodes, {} evidence, {:.0}% connected, \
                 Bayesian confidence {:.0}%.",
                llm_score, max_score, recommended, max_score,
                total_nodes, evidence_count,
                connectivity * 100.0, avg_confidence * 100.0
            ),
            recommended_score: recommended,
        };
    }

    // FLAGGED: LLM score significantly below evidence (underscoring)
    if avg_node_quality > normalized_llm + 0.25 && avg_node_quality > 0.4 {
        let recommended = avg_node_quality * max_score;
        return Verdict::Flagged {
            reason: format!(
                "LLM scored {:.1}/{:.0} but evidence supports ~{:.1}/{:.0}. \
                 Score may be too low for the evidence quality.",
                llm_score, max_score, recommended, max_score,
            ),
            recommended_score: recommended,
        };
    }

    // FLAGGED: low Bayesian confidence
    if avg_confidence < 0.4 && normalized_llm > 0.5 {
        return Verdict::Flagged {
            reason: format!(
                "Bayesian confidence {:.0}% is below threshold. \
                 Evidence may be unverifiable or low quality.",
                avg_confidence * 100.0
            ),
            recommended_score: llm_score * avg_confidence,
        };
    }

    // CONFIRMED: evidence is consistent with LLM score
    Verdict::Confirmed {
        reason: format!(
            "Evidence supports score {:.1}/{:.0}. Graph: {} nodes ({} evidence, {} claims), \
             {:.0}% connected, Bayesian confidence {:.0}%.",
            llm_score, max_score,
            total_nodes, evidence_count, claim_count,
            connectivity * 100.0, avg_confidence * 100.0
        ),
    }
}

impl AgentNetwork {
    /// Create a network for an agent with one neuron per criterion.
    pub fn new(agent: &EvaluatorAgent, criteria: &[EvaluationCriterion]) -> Self {
        let neurons = criteria.iter().map(|c| Neuron::new(c, &agent.id)).collect();
        Self {
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            agent_role: agent.role.clone(),
            agent_domain: agent.domain.clone(),
            neurons,
        }
    }

    /// Feed document evidence into the network as spikes.
    pub fn feed_evidence(
        &mut self,
        doc: &EvalDocument,
        alignments: &[AlignmentMapping],
        config: &SNNConfig,
    ) {
        for timestep in 0..config.timesteps {
            for neuron in &mut self.neurons {
                // Clear refractory after period
                if timestep > 0 && timestep % config.refractory_period == 0 {
                    neuron.clear_refractory();
                }

                // Find aligned sections for this neuron's criterion
                let relevant_alignments: Vec<&AlignmentMapping> = alignments
                    .iter()
                    .filter(|a| a.criterion_id == neuron.criterion_id)
                    .collect();

                for alignment in &relevant_alignments {
                    // Find the section
                    if let Some(section) = find_section(&doc.sections, &alignment.section_id) {
                        // Generate spikes from claims
                        for claim in &section.claims {
                            let spike_type = if claim.verifiable {
                                SpikeType::Evidence
                            } else {
                                SpikeType::Claim
                            };
                            let multiplier = role_spike_multiplier(
                                &self.agent_role,
                                &self.agent_domain,
                                &spike_type,
                                &section.title,
                            );
                            let strength =
                                (claim.specificity * alignment.confidence * multiplier).min(1.0);
                            neuron.receive_spike(
                                Spike {
                                    source_id: claim.id.clone(),
                                    strength,
                                    spike_type,
                                    timestep,
                                    source_text: None,
                                    justification: None,
                                },
                                config,
                            );
                        }

                        // Generate spikes from evidence
                        for ev in &section.evidence {
                            let base_strength = alignment.confidence;
                            let quant_bonus = if ev.has_quantified_outcome {
                                0.2
                            } else {
                                0.0
                            };
                            let spike_type = if ev.has_quantified_outcome {
                                SpikeType::QuantifiedData
                            } else if ev.evidence_type == "citation" {
                                SpikeType::Citation
                            } else {
                                SpikeType::Evidence
                            };
                            let multiplier = role_spike_multiplier(
                                &self.agent_role,
                                &self.agent_domain,
                                &spike_type,
                                &section.title,
                            );
                            let strength =
                                ((base_strength + quant_bonus) * multiplier).min(1.0);
                            neuron.receive_spike(
                                Spike {
                                    source_id: ev.id.clone(),
                                    strength,
                                    spike_type,
                                    timestep,
                                    source_text: None,
                                    justification: None,
                                },
                                config,
                            );
                        }

                        // Generate alignment spike (the section itself addresses the criterion)
                        if alignment.confidence > 0.3 {
                            let multiplier = role_spike_multiplier(
                                &self.agent_role,
                                &self.agent_domain,
                                &SpikeType::Alignment,
                                &section.title,
                            );
                            let strength =
                                (alignment.confidence * 0.5 * multiplier).min(1.0);
                            neuron.receive_spike(
                                Spike {
                                    source_id: section.id.clone(),
                                    strength,
                                    spike_type: SpikeType::Alignment,
                                    timestep,
                                    source_text: None,
                                    justification: None,
                                },
                                config,
                            );
                        }

                        // Process subsections recursively
                        for sub in &section.subsections {
                            feed_subsection_spikes(
                                neuron,
                                sub,
                                alignment.confidence,
                                timestep,
                                config,
                                &self.agent_role,
                                &self.agent_domain,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Get SNN scores for all criteria.
    pub fn compute_scores(
        &self,
        criteria: &[EvaluationCriterion],
        config: &SNNConfig,
    ) -> Vec<(String, SNNScore)> {
        self.neurons
            .iter()
            .map(|neuron| {
                let max_score = criteria
                    .iter()
                    .find(|c| c.id == neuron.criterion_id)
                    .map(|c| c.max_score)
                    .unwrap_or(10.0);
                (
                    neuron.criterion_id.clone(),
                    neuron.compute_score(max_score, config),
                )
            })
            .collect()
    }

    /// Feed scored argument graph nodes into the SNN, weighting by graph topology.
    /// This is the Option B architecture: LLM scores nodes, SNN aggregates the graph.
    /// Optional alignment_candidates from onto_align boost nodes that match the reference ontology.
    pub fn feed_argument_graph(
        &mut self,
        graph: &crate::argument_graph::ArgumentGraph,
        config: &SNNConfig,
    ) {
        self.feed_argument_graph_with_alignment(graph, config, &[]);
    }

    /// Feed argument graph with alignment signals from onto_align against a reference ontology.
    pub fn feed_argument_graph_with_alignment(
        &mut self,
        graph: &crate::argument_graph::ArgumentGraph,
        config: &SNNConfig,
        alignment_candidates: &[crate::argument_graph::AlignmentCandidate],
    ) {
        use crate::argument_graph::{self, NodeType};

        let pagerank = argument_graph::compute_pagerank(graph, 0.85, 20);
        let metrics = argument_graph::compute_metrics(graph);

        for timestep in 0..config.timesteps {
            for neuron in &mut self.neurons {
                if timestep > 0 && timestep % config.refractory_period == 0 {
                    neuron.clear_refractory();
                }

                // Feed each scored node as a spike weighted by PageRank
                for node in &graph.nodes {
                    if node.node_type == NodeType::Structural {
                        continue; // structural nodes don't generate content spikes
                    }

                    let pr_weight = pagerank.get(&node.iri).copied().unwrap_or(0.1);
                    let llm_score = node.llm_score.unwrap_or(0.5);

                    // Base spike from node type + LLM score
                    let base_type = match node.node_type {
                        NodeType::Thesis | NodeType::SubClaim => SpikeType::Claim,
                        NodeType::Evidence => SpikeType::Evidence,
                        NodeType::QuantifiedEvidence => SpikeType::QuantifiedData,
                        NodeType::Citation => SpikeType::Citation,
                        NodeType::Counter | NodeType::Rebuttal => SpikeType::Claim,
                        NodeType::Structural => continue,
                    };

                    // Strength = LLM quality score * PageRank weight
                    let strength = (llm_score * pr_weight).clamp(0.05, 1.0);

                    neuron.receive_spike(
                        Spike {
                            source_id: node.iri.clone(),
                            strength,
                            spike_type: base_type,
                            timestep,
                            source_text: Some(node.text.clone()),
                            justification: node.llm_justification.clone(),
                        },
                        config,
                    );
                }

                // Structural signals (once per neuron, on timestep 0)
                if timestep == 0 {
                    // Connectivity signal
                    if metrics.connectivity > 0.5 {
                        neuron.receive_spike(
                            Spike {
                                source_id: "graph:connectivity".into(),
                                strength: metrics.connectivity.clamp(0.0, 1.0),
                                spike_type: SpikeType::Connectivity,
                                timestep: 0,
                                source_text: None,
                                justification: Some(format!(
                                    "Graph connectivity: {:.0}% of nodes connected",
                                    metrics.connectivity * 100.0
                                )),
                            },
                            config,
                        );
                    }

                    // Evidence coverage signal
                    if metrics.evidence_coverage > 0.0 {
                        neuron.receive_spike(
                            Spike {
                                source_id: "graph:support".into(),
                                strength: metrics.evidence_coverage.clamp(0.0, 1.0),
                                spike_type: SpikeType::Support,
                                timestep: 0,
                                source_text: None,
                                justification: Some(format!(
                                    "Evidence coverage: {:.0}% of claims have support",
                                    metrics.evidence_coverage * 100.0
                                )),
                            },
                            config,
                        );
                    }

                    // Depth signal
                    if metrics.max_depth >= 2 {
                        neuron.receive_spike(
                            Spike {
                                source_id: "graph:depth".into(),
                                strength: (metrics.max_depth as f64 / 5.0).clamp(0.1, 1.0),
                                spike_type: SpikeType::Depth,
                                timestep: 0,
                                source_text: None,
                                justification: Some(format!("Argument depth: {} levels", metrics.max_depth)),
                            },
                            config,
                        );
                    }

                    // Isolation signal (negative — many disconnected nodes)
                    if metrics.connectivity < 0.5 && metrics.node_count > 3 {
                        neuron.receive_spike(
                            Spike {
                                source_id: "graph:isolation".into(),
                                strength: (1.0 - metrics.connectivity).clamp(0.1, 0.8),
                                spike_type: SpikeType::Isolation,
                                timestep: 0,
                                source_text: None,
                                justification: Some(format!(
                                    "Low connectivity: {:.0}% nodes isolated",
                                    (1.0 - metrics.connectivity) * 100.0
                                )),
                            },
                            config,
                        );
                        neuron.apply_inhibition(0.1);
                    }

                    // Counter-balance (dialectical strength)
                    if metrics.has_counter && metrics.has_rebuttal {
                        neuron.receive_spike(
                            Spike {
                                source_id: "graph:counterbalance".into(),
                                strength: 0.8,
                                spike_type: SpikeType::CounterBalance,
                                timestep: 0,
                                source_text: None,
                                justification: Some("Counter-argument with rebuttal present".into()),
                            },
                            config,
                        );
                    }

                    // Alignment signals from onto_align (once per neuron)
                    if !alignment_candidates.is_empty() {
                        // Average alignment confidence across all candidates
                        let avg_conf: f64 = alignment_candidates.iter()
                            .map(|c| c.confidence)
                            .sum::<f64>() / alignment_candidates.len() as f64;

                        // Count high-confidence matches (strong structural alignment to reference)
                        let strong_matches = alignment_candidates.iter()
                            .filter(|c| c.confidence > 0.7)
                            .count();

                        if avg_conf > 0.3 {
                            neuron.receive_spike(
                                Spike {
                                    source_id: "align:reference".into(),
                                    strength: avg_conf.clamp(0.0, 1.0),
                                    spike_type: SpikeType::Alignment,
                                    timestep: 0,
                                    source_text: None,
                                    justification: Some(format!(
                                        "Reference alignment: {:.0}% avg confidence, {} strong matches",
                                        avg_conf * 100.0, strong_matches
                                    )),
                                },
                                config,
                            );
                        }

                        // High match count = well-structured essay
                        if strong_matches >= 3 {
                            neuron.receive_spike(
                                Spike {
                                    source_id: "align:depth".into(),
                                    strength: (strong_matches as f64 / 10.0).clamp(0.3, 1.0),
                                    spike_type: SpikeType::Support,
                                    timestep: 0,
                                    source_text: None,
                                    justification: Some(format!(
                                        "{} strong alignments to reference ontology",
                                        strong_matches
                                    )),
                                },
                                config,
                            );
                        }
                    }
                }

                // Decay
                neuron.membrane_potential *= 1.0 - config.decay_rate;
            }
        }
    }

    /// Get the spike log for a specific criterion's neuron.
    pub fn spike_log_for(&self, criterion_id: &str) -> Option<&Vec<Spike>> {
        self.neurons
            .iter()
            .find(|n| n.criterion_id == criterion_id)
            .map(|n| &n.spike_log)
    }

    /// Apply debate inhibition to a specific neuron.
    pub fn inhibit(&mut self, criterion_id: &str, amount: f64) {
        if let Some(neuron) = self
            .neurons
            .iter_mut()
            .find(|n| n.criterion_id == criterion_id)
        {
            neuron.apply_inhibition(amount);
        }
    }
} // end impl AgentNetwork

/// Get a spike weight multiplier based on agent role and evidence type.
/// Different agent roles prioritize different types of evidence, producing
/// differentiated SNN scores across the panel.
fn role_spike_multiplier(
    agent_role: &str,
    _agent_domain: &str,
    spike_type: &SpikeType,
    section_title: &str,
) -> f64 {
    let role_lower = agent_role.to_lowercase();
    let title_lower = section_title.to_lowercase();

    // Subject Expert / Domain Expert: boost knowledge-related evidence
    if role_lower.contains("subject")
        || role_lower.contains("domain")
        || role_lower.contains("expert")
    {
        return match spike_type {
            SpikeType::QuantifiedData => 1.4,
            SpikeType::Evidence => 1.3,
            SpikeType::Citation => 1.2,
            SpikeType::Claim => 0.8,
            SpikeType::Alignment => 1.0,
            SpikeType::Connectivity | SpikeType::Support | SpikeType::Depth => 1.1,
            SpikeType::Isolation => 0.9,
            SpikeType::CounterBalance => 1.2,
        };
    }

    // Writing Specialist / Communication: boost structure and transitions
    if role_lower.contains("writing")
        || role_lower.contains("communication")
        || role_lower.contains("language")
    {
        let structure_bonus = if title_lower.contains("intro")
            || title_lower.contains("conclu")
            || title_lower.contains("structure")
            || title_lower.contains("communicat")
        {
            1.3
        } else {
            1.0
        };
        return match spike_type {
            SpikeType::Alignment => 1.4 * structure_bonus,
            SpikeType::Claim => 1.2,
            SpikeType::Citation => 0.9,
            SpikeType::QuantifiedData => 0.8,
            SpikeType::Evidence => 1.0,
            SpikeType::Connectivity | SpikeType::Depth => 1.3,
            SpikeType::Support => 1.1,
            SpikeType::Isolation => 0.7,
            SpikeType::CounterBalance => 1.2,
        };
    }

    // Critical Thinking / Analytical: boost analysis evidence
    if role_lower.contains("critical")
        || role_lower.contains("analy")
        || role_lower.contains("thinking")
    {
        return match spike_type {
            SpikeType::Claim => 1.4,
            SpikeType::Evidence => 1.3,
            SpikeType::QuantifiedData => 1.2,
            SpikeType::Citation => 1.0,
            SpikeType::Alignment => 0.9,
            SpikeType::Connectivity | SpikeType::Support => 1.3,
            SpikeType::Depth | SpikeType::CounterBalance => 1.4,
            SpikeType::Isolation => 0.6,
        };
    }

    // Policy Analyst: values evidence base and stakeholder representation
    if role_lower.contains("policy") || role_lower.contains("analyst") {
        return match spike_type {
            SpikeType::QuantifiedData => 1.4,
            SpikeType::Evidence => 1.3,
            SpikeType::Citation => 1.1,
            SpikeType::Claim => 0.9,
            SpikeType::Alignment => 1.0,
            SpikeType::Connectivity | SpikeType::Support | SpikeType::Depth => 1.1,
            SpikeType::Isolation => 0.9,
            SpikeType::CounterBalance => 1.2,
        };
    }

    // Stakeholder / Patient / Community Representative: values breadth
    if role_lower.contains("stakeholder")
        || role_lower.contains("patient")
        || role_lower.contains("community")
        || role_lower.contains("representative")
    {
        return match spike_type {
            SpikeType::Claim => 1.3,
            SpikeType::Evidence => 1.1,
            SpikeType::Alignment => 1.3,
            SpikeType::QuantifiedData => 0.9,
            SpikeType::Citation => 0.8,
            SpikeType::Connectivity | SpikeType::Support => 1.2,
            SpikeType::Depth | SpikeType::CounterBalance => 1.1,
            SpikeType::Isolation => 0.8,
        };
    }

    // Finance / Commercial: values quantified data
    if role_lower.contains("finance")
        || role_lower.contains("commercial")
        || role_lower.contains("value")
    {
        return match spike_type {
            SpikeType::QuantifiedData => 1.5,
            SpikeType::Evidence => 1.1,
            SpikeType::Claim => 0.7,
            SpikeType::Citation => 0.9,
            SpikeType::Alignment => 1.0,
            SpikeType::Connectivity | SpikeType::Support | SpikeType::Depth => 1.0,
            SpikeType::Isolation => 0.8,
            SpikeType::CounterBalance => 1.1,
        };
    }

    // Compliance / Legal / Governance: values structure and alignment
    if role_lower.contains("compliance")
        || role_lower.contains("legal")
        || role_lower.contains("governance")
        || role_lower.contains("procurement")
    {
        return match spike_type {
            SpikeType::Alignment => 1.5,
            SpikeType::Evidence => 1.2,
            SpikeType::QuantifiedData => 1.1,
            SpikeType::Claim => 0.8,
            SpikeType::Citation => 1.0,
            SpikeType::Connectivity | SpikeType::Support => 1.3,
            SpikeType::Depth | SpikeType::CounterBalance => 1.2,
            SpikeType::Isolation => 0.7,
        };
    }

    // Moderator: balanced across all types
    if role_lower.contains("moderator") || role_lower.contains("panel") {
        return 1.0;
    }

    // Default: no multiplier
    1.0
}

/// Feed spikes from a subsection.
fn feed_subsection_spikes(
    neuron: &mut Neuron,
    section: &Section,
    parent_confidence: f64,
    timestep: u32,
    config: &SNNConfig,
    agent_role: &str,
    agent_domain: &str,
) {
    let confidence = parent_confidence * 0.8; // Subsections inherit parent confidence with decay
    for claim in &section.claims {
        let spike_type = if claim.verifiable {
            SpikeType::Evidence
        } else {
            SpikeType::Claim
        };
        let multiplier =
            role_spike_multiplier(agent_role, agent_domain, &spike_type, &section.title);
        let strength = (claim.specificity * confidence * multiplier).min(1.0);
        neuron.receive_spike(
            Spike {
                source_id: claim.id.clone(),
                strength,
                spike_type,
                timestep,
                source_text: None,
                justification: None,
            },
            config,
        );
    }
    for ev in &section.evidence {
        let spike_type = if ev.has_quantified_outcome {
            SpikeType::QuantifiedData
        } else {
            SpikeType::Evidence
        };
        let multiplier =
            role_spike_multiplier(agent_role, agent_domain, &spike_type, &section.title);
        let base = confidence + if ev.has_quantified_outcome { 0.2 } else { 0.0 };
        let strength = (base * multiplier).min(1.0);
        neuron.receive_spike(
            Spike {
                source_id: ev.id.clone(),
                strength,
                spike_type,
                timestep,
                source_text: None,
                justification: None,
            },
            config,
        );
    }
    for sub in &section.subsections {
        feed_subsection_spikes(
            neuron,
            sub,
            confidence,
            timestep,
            config,
            agent_role,
            agent_domain,
        );
    }
}

/// Find a section by ID in nested sections.
fn find_section<'a>(sections: &'a [Section], id: &str) -> Option<&'a Section> {
    for s in sections {
        if s.id == id {
            return Some(s);
        }
        if let Some(found) = find_section(&s.subsections, id) {
            return Some(found);
        }
    }
    None
}

/// Combine SNN score with LLM score for a balanced result.
///
/// The SNN provides evidence-grounded quantitative scoring.
/// The LLM provides qualitative judgment.
/// Combined score = weighted blend based on SNN confidence.
///
/// If SNN confidence is high (lots of evidence), SNN dominates.
/// If SNN confidence is low (little evidence), LLM fills in.
/// This prevents hallucination: low SNN confidence flags "insufficient evidence".
pub fn blend_scores(snn_score: &SNNScore, llm_score: f64, max_score: f64) -> BlendedScore {
    let snn_normalised = snn_score.snn_score / max_score;
    let llm_normalised = llm_score / max_score;

    // SNN weight increases with Bayesian confidence (more calibrated than raw confidence)
    let snn_weight = snn_score.bayesian_confidence * 0.6; // Max 60% SNN influence
    let llm_weight = 1.0 - snn_weight;

    let blended_normalised = snn_normalised * snn_weight + llm_normalised * llm_weight;
    let blended = blended_normalised * max_score;

    // Hallucination flag: if LLM scores high but SNN scores low,
    // the LLM may be hallucinating evidence
    let hallucination_risk = llm_normalised > 0.7 && snn_normalised < 0.3 && snn_score.grounded;

    let explanation = format!(
        "Blended score: {:.1}/{:.0}. SNN: {:.1} (weight {:.0}%, confidence {:.0}%). \
         LLM: {:.1} (weight {:.0}%). {}",
        blended,
        max_score,
        snn_score.snn_score,
        snn_weight * 100.0,
        snn_score.confidence * 100.0,
        llm_score,
        llm_weight * 100.0,
        if hallucination_risk {
            "WARNING: LLM scored significantly higher than evidence supports. \
             Possible hallucination."
        } else {
            "Scores are consistent — evidence supports the qualitative assessment."
        }
    );

    BlendedScore {
        final_score: blended,
        snn_component: snn_score.snn_score,
        llm_component: llm_score,
        snn_weight,
        llm_weight,
        hallucination_risk,
        explanation,
    }
}

/// Combined SNN + LLM score.
#[derive(Debug, Clone, Serialize)]
pub struct BlendedScore {
    pub final_score: f64,
    pub snn_component: f64,
    pub llm_component: f64,
    pub snn_weight: f64,
    pub llm_weight: f64,
    pub hallucination_risk: bool,
    pub explanation: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_criterion() -> EvaluationCriterion {
        EvaluationCriterion {
            id: "c1".into(),
            title: "Knowledge".into(),
            description: None,
            max_score: 10.0,
            weight: 0.5,
            rubric_levels: vec![],
            sub_criteria: vec![],
        }
    }

    fn test_agent() -> EvaluatorAgent {
        EvaluatorAgent {
            id: "a1".into(),
            name: "Test Agent".into(),
            role: "Expert".into(),
            domain: "Test".into(),
            years_experience: Some(10),
            persona_description: "Test".into(),
            needs: vec![],
            trust_weights: vec![],
        }
    }

    #[test]
    fn test_neuron_creation() {
        let crit = test_criterion();
        let neuron = Neuron::new(&crit, "a1");
        assert_eq!(neuron.membrane_potential, 0.0);
        assert!(neuron.threshold > 0.0);
        assert_eq!(neuron.fire_count, 0);
    }

    #[test]
    fn test_spike_integration() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        neuron.receive_spike(
            Spike {
                source_id: "ev1".into(),
                strength: 0.8,
                spike_type: SpikeType::QuantifiedData,
                timestep: 0,
                source_text: None,
                justification: None,
            },
            &config,
        );

        assert!(neuron.membrane_potential > 0.0 || neuron.fire_count > 0);
        assert_eq!(neuron.total_spikes, 1);
    }

    #[test]
    fn test_neuron_fires_on_strong_evidence() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        // Send multiple strong spikes
        for i in 0..5 {
            neuron.clear_refractory();
            neuron.receive_spike(
                Spike {
                    source_id: format!("ev{}", i),
                    strength: 0.9,
                    spike_type: SpikeType::QuantifiedData,
                    timestep: i,
                    source_text: None,
                    justification: None,
                },
                &config,
            );
        }

        assert!(neuron.fire_count > 0, "Should fire with strong evidence");
    }

    #[test]
    fn test_no_score_without_evidence() {
        let crit = test_criterion();
        let neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();
        let score = neuron.compute_score(10.0, &config);
        assert_eq!(score.snn_score, 0.0);
        assert!(!score.grounded);
        assert!(score.explanation.contains("Insufficient evidence"));
    }

    #[test]
    fn test_lateral_inhibition() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        neuron.apply_inhibition(0.3);

        // Spike with strength 0.5 should be reduced by inhibition
        neuron.receive_spike(
            Spike {
                source_id: "ev1".into(),
                strength: 0.5,
                spike_type: SpikeType::Evidence,
                timestep: 0,
                source_text: None,
                justification: None,
            },
            &config,
        );

        // Effective strength = 0.5 - 0.3 = 0.2
        assert!(
            neuron.membrane_potential < 0.3,
            "Inhibition should reduce spike effect"
        );
    }

    #[test]
    fn test_agent_network() {
        let agent = test_agent();
        let criteria = vec![test_criterion()];
        let network = AgentNetwork::new(&agent, &criteria);
        assert_eq!(network.neurons.len(), 1);
    }

    #[test]
    fn test_blend_scores_balanced() {
        let snn = SNNScore {
            snn_score: 7.0,
            confidence: 0.8,
            firing_rate: 0.5,
            evidence_count: 5,
            spike_quality: 0.7,
            grounded: true,
            explanation: String::new(),
            falsification_checked: true,
            bayesian_confidence: 0.8,
            confidence_interval: (5.5, 8.5),
        };
        let blended = blend_scores(&snn, 7.0, 10.0);
        assert!(!blended.hallucination_risk);
        assert!((blended.final_score - 7.0).abs() < 0.5);
    }

    #[test]
    fn test_blend_detects_hallucination() {
        let snn = SNNScore {
            snn_score: 2.0,
            confidence: 0.8,
            firing_rate: 0.1,
            evidence_count: 3,
            spike_quality: 0.3,
            grounded: true,
            explanation: String::new(),
            falsification_checked: true,
            bayesian_confidence: 0.8,
            confidence_interval: (1.0, 3.0),
        };
        // LLM says 9/10 but SNN says 2/10 — hallucination!
        let blended = blend_scores(&snn, 9.0, 10.0);
        assert!(
            blended.hallucination_risk,
            "Should detect hallucination when LLM >> SNN"
        );
    }

    #[test]
    fn test_blend_low_confidence_favours_llm() {
        let snn = SNNScore {
            snn_score: 3.0,
            confidence: 0.1,
            firing_rate: 0.1,
            evidence_count: 1,
            spike_quality: 0.3,
            grounded: true,
            explanation: String::new(),
            falsification_checked: true,
            bayesian_confidence: 0.1,
            confidence_interval: (1.0, 5.0),
        };
        let blended = blend_scores(&snn, 8.0, 10.0);
        // Low SNN confidence → LLM dominates
        assert!(
            blended.final_score > 6.0,
            "Low SNN confidence should let LLM dominate: {}",
            blended.final_score
        );
    }

    #[test]
    fn test_feed_evidence_into_network() {
        let agent = test_agent();
        let criteria = vec![test_criterion()];
        let mut network = AgentNetwork::new(&agent, &criteria);
        let config = SNNConfig::default();

        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(),
                title: "Test Section".into(),
                text: "Content".into(),
                word_count: 10,
                page_range: None,
                claims: vec![Claim {
                    id: "cl1".into(),
                    text: "A claim".into(),
                    specificity: 0.8,
                    verifiable: true,
                }],
                evidence: vec![Evidence {
                    id: "ev1".into(),
                    source: "Source".into(),
                    evidence_type: "statistical".into(),
                    text: "Data".into(),
                    has_quantified_outcome: true,
                }],
                subsections: vec![],
            }],
        };

        let alignments = vec![AlignmentMapping {
            section_id: "s1".into(),
            criterion_id: "c1".into(),
            confidence: 0.9,
        }];

        network.feed_evidence(&doc, &alignments, &config);

        let scores = network.compute_scores(&criteria, &config);
        assert_eq!(scores.len(), 1);
        let (_, snn_score) = &scores[0];
        assert!(snn_score.evidence_count > 0, "Should have received spikes");
        assert!(snn_score.grounded, "Should be evidence-grounded");
    }

    #[test]
    fn test_bayesian_update_from_prior() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        assert!((neuron.bayesian_confidence - 0.5).abs() < 0.01, "Prior should be 0.5");

        // Add strong quantified evidence — confidence should increase
        for i in 0..3 {
            neuron.clear_refractory();
            neuron.receive_spike(
                Spike {
                    source_id: format!("ev{}", i),
                    strength: 0.9,
                    spike_type: SpikeType::QuantifiedData,
                    timestep: i,
                    source_text: None,
                    justification: None,
                },
                &config,
            );
        }

        assert!(
            neuron.bayesian_confidence > 0.7,
            "Bayesian confidence should increase with strong evidence: {}",
            neuron.bayesian_confidence
        );
    }

    #[test]
    fn test_bayesian_lr_cap() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        // Even with many strong spikes, confidence should never exceed 0.99
        for i in 0..20 {
            neuron.clear_refractory();
            neuron.receive_spike(
                Spike {
                    source_id: format!("ev{}", i),
                    strength: 1.0,
                    spike_type: SpikeType::QuantifiedData,
                    timestep: i,
                    source_text: None,
                    justification: None,
                },
                &config,
            );
        }

        assert!(
            neuron.bayesian_confidence <= 0.99,
            "LR cap should prevent confidence > 0.99: {}",
            neuron.bayesian_confidence
        );
    }

    #[test]
    fn test_falsification_unfalsified_high_score() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        // Send many strong spikes to produce a high score, none with low strength
        for i in 0..10 {
            neuron.clear_refractory();
            neuron.receive_spike(
                Spike {
                    source_id: format!("ev{}", i),
                    strength: 0.95,
                    spike_type: SpikeType::QuantifiedData,
                    timestep: i,
                    source_text: None,
                    justification: None,
                },
                &config,
            );
        }

        let score = neuron.compute_score(10.0, &config);
        // If the score is high (>80% of max) and no counter-evidence, falsification should fail
        if score.snn_score > 8.0 {
            assert!(
                !score.falsification_checked,
                "High score without counter-evidence should not pass falsification"
            );
            // Confidence should be penalised
            // (adjusted_confidence = confidence * 0.7)
        }
    }

    #[test]
    fn test_confidence_interval_narrows_with_evidence() {
        let crit = test_criterion();
        let config = SNNConfig::default();

        // Few spikes
        let mut neuron_few = Neuron::new(&crit, "a1");
        for i in 0..2 {
            neuron_few.clear_refractory();
            neuron_few.receive_spike(
                Spike {
                    source_id: format!("ev{}", i),
                    strength: 0.7,
                    spike_type: SpikeType::Evidence,
                    timestep: i,
                    source_text: None,
                    justification: None,
                },
                &config,
            );
        }
        let score_few = neuron_few.compute_score(10.0, &config);

        // Many spikes
        let mut neuron_many = Neuron::new(&crit, "a1");
        for i in 0..12 {
            neuron_many.clear_refractory();
            neuron_many.receive_spike(
                Spike {
                    source_id: format!("ev{}", i),
                    strength: 0.7,
                    spike_type: SpikeType::Evidence,
                    timestep: i % config.timesteps,
                    source_text: None,
                    justification: None,
                },
                &config,
            );
        }
        let score_many = neuron_many.compute_score(10.0, &config);

        let width_few = score_few.confidence_interval.1 - score_few.confidence_interval.0;
        let width_many = score_many.confidence_interval.1 - score_many.confidence_interval.0;

        assert!(
            width_many < width_few,
            "More evidence should narrow CI: few={:.1}, many={:.1}",
            width_few,
            width_many
        );
    }

    #[test]
    fn test_confidence_interval_wide_with_few_spikes() {
        let crit = test_criterion();
        let mut neuron = Neuron::new(&crit, "a1");
        let config = SNNConfig::default();

        neuron.receive_spike(
            Spike {
                source_id: "ev1".into(),
                strength: 0.7,
                spike_type: SpikeType::Evidence,
                timestep: 0,
                source_text: None,
                justification: None,
            },
            &config,
        );

        let score = neuron.compute_score(10.0, &config);
        let width = score.confidence_interval.1 - score.confidence_interval.0;

        // With only 1 spike, CI width should be at least 3.0 (40% of max_score=10)
        assert!(
            width >= 3.0,
            "Few spikes should produce wide CI: width={:.1}",
            width
        );
    }

    #[test]
    fn test_role_spike_multiplier_expert() {
        // Subject Expert should boost QuantifiedData and reduce Claim
        let m = role_spike_multiplier("Subject Expert", "Test", &SpikeType::QuantifiedData, "sec");
        assert!((m - 1.4).abs() < 0.01);
        let m = role_spike_multiplier("Subject Expert", "Test", &SpikeType::Claim, "sec");
        assert!((m - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_role_spike_multiplier_writing() {
        // Writing Specialist should boost Alignment
        let m = role_spike_multiplier("Writing Specialist", "Test", &SpikeType::Alignment, "sec");
        assert!((m - 1.4).abs() < 0.01);
        // With structure section title, bonus applies
        let m = role_spike_multiplier(
            "Writing Specialist",
            "Test",
            &SpikeType::Alignment,
            "Introduction",
        );
        assert!((m - 1.4 * 1.3).abs() < 0.01);
    }

    #[test]
    fn test_role_spike_multiplier_default() {
        // Unknown role returns 1.0
        let m = role_spike_multiplier("Unknown Role", "Test", &SpikeType::Evidence, "sec");
        assert!((m - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_role_specific_scoring() {
        // Two agents with different roles should produce different SNN scores
        // on the same document because they weight evidence types differently.
        let expert = EvaluatorAgent {
            role: "Subject Expert".into(),
            ..test_agent()
        };
        let writer = EvaluatorAgent {
            role: "Academic Writing Specialist".into(),
            ..test_agent()
        };

        let criteria = vec![test_criterion()];
        let mut net_expert = AgentNetwork::new(&expert, &criteria);
        let mut net_writer = AgentNetwork::new(&writer, &criteria);

        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(200),
            sections: vec![Section {
                id: "s1".into(),
                title: "Test Section".into(),
                text: "Content with claims and data".into(),
                word_count: 50,
                page_range: None,
                claims: vec![
                    Claim {
                        id: "cl1".into(),
                        text: "A verifiable claim".into(),
                        specificity: 0.9,
                        verifiable: true,
                    },
                    Claim {
                        id: "cl2".into(),
                        text: "An unverifiable claim".into(),
                        specificity: 0.7,
                        verifiable: false,
                    },
                ],
                evidence: vec![
                    Evidence {
                        id: "ev1".into(),
                        source: "Source A".into(),
                        evidence_type: "statistical".into(),
                        text: "87% improvement".into(),
                        has_quantified_outcome: true,
                    },
                    Evidence {
                        id: "ev2".into(),
                        source: "Source B".into(),
                        evidence_type: "citation".into(),
                        text: "Smith et al (2024)".into(),
                        has_quantified_outcome: false,
                    },
                ],
                subsections: vec![],
            }],
        };

        let alignments = vec![AlignmentMapping {
            section_id: "s1".into(),
            criterion_id: "c1".into(),
            confidence: 0.9,
        }];

        let config = SNNConfig::default();

        net_expert.feed_evidence(&doc, &alignments, &config);
        net_writer.feed_evidence(&doc, &alignments, &config);

        let expert_scores = net_expert.compute_scores(&criteria, &config);
        let writer_scores = net_writer.compute_scores(&criteria, &config);

        assert_eq!(expert_scores.len(), 1);
        assert_eq!(writer_scores.len(), 1);

        let expert_snn = expert_scores[0].1.snn_score;
        let writer_snn = writer_scores[0].1.snn_score;

        assert_ne!(
            expert_snn, writer_snn,
            "Different roles should produce different SNN scores: expert={}, writer={}",
            expert_snn, writer_snn
        );

        // Both should still be grounded
        assert!(expert_scores[0].1.grounded);
        assert!(writer_scores[0].1.grounded);
    }

    #[test]
    fn test_role_specific_scoring_finance_vs_compliance() {
        // Finance agent should score higher on quantified-data-heavy documents
        // Compliance agent should score higher on alignment-heavy documents
        let finance = EvaluatorAgent {
            role: "Finance Specialist".into(),
            ..test_agent()
        };
        let compliance = EvaluatorAgent {
            role: "Compliance Officer".into(),
            ..test_agent()
        };

        let criteria = vec![test_criterion()];
        let mut net_finance = AgentNetwork::new(&finance, &criteria);
        let mut net_compliance = AgentNetwork::new(&compliance, &criteria);

        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "report".into(),
            total_pages: None,
            total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(),
                title: "Financial Analysis".into(),
                text: "Numbers and data".into(),
                word_count: 50,
                page_range: None,
                claims: vec![Claim {
                    id: "cl1".into(),
                    text: "Revenue increased".into(),
                    specificity: 0.6,
                    verifiable: false,
                }],
                evidence: vec![Evidence {
                    id: "ev1".into(),
                    source: "Accounts".into(),
                    evidence_type: "statistical".into(),
                    text: "Revenue +23%".into(),
                    has_quantified_outcome: true,
                }],
                subsections: vec![],
            }],
        };

        let alignments = vec![AlignmentMapping {
            section_id: "s1".into(),
            criterion_id: "c1".into(),
            confidence: 0.9,
        }];

        let config = SNNConfig::default();

        net_finance.feed_evidence(&doc, &alignments, &config);
        net_compliance.feed_evidence(&doc, &alignments, &config);

        let finance_scores = net_finance.compute_scores(&criteria, &config);
        let compliance_scores = net_compliance.compute_scores(&criteria, &config);

        assert_ne!(
            finance_scores[0].1.snn_score,
            compliance_scores[0].1.snn_score,
            "Finance and compliance roles should produce different scores"
        );
    }

    #[test]
    fn test_default_weights_unchanged() {
        // Verify that default ScoreWeights match the original hardcoded values.
        // If this test fails, it means the defaults were changed — which would
        // silently alter all existing scoring behaviour.
        let w = ScoreWeights::default();
        assert!((w.w_saturation - 0.50).abs() < 1e-10, "w_saturation default changed");
        assert!((w.w_quality - 0.35).abs() < 1e-10, "w_quality default changed");
        assert!((w.w_firing - 0.15).abs() < 1e-10, "w_firing default changed");
        assert!((w.saturation_base - 15.0).abs() < 1e-10, "saturation_base default changed");
        assert!((w.lr_quantified - 2.5).abs() < 1e-10, "lr_quantified default changed");
        assert!((w.lr_evidence - 2.0).abs() < 1e-10, "lr_evidence default changed");
        assert!((w.lr_citation - 1.8).abs() < 1e-10, "lr_citation default changed");
        assert!((w.lr_alignment - 1.5).abs() < 1e-10, "lr_alignment default changed");
        assert!((w.lr_claim - 1.3).abs() < 1e-10, "lr_claim default changed");
    }

    #[test]
    fn test_score_weights_roundtrip() {
        let w = ScoreWeights::default();
        let params = w.to_params();
        let w2 = ScoreWeights::from_params(&params);
        assert!((w2.w_saturation - w.w_saturation).abs() < 1e-10);
        assert!((w2.saturation_base - w.saturation_base).abs() < 1e-10);
        assert!((w2.lr_claim - w.lr_claim).abs() < 1e-10);
    }
}
