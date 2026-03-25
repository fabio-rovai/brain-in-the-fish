//! Integration test: EDS tools (eds_feed → eds_score → eds_challenge → eds_consensus)

use brain_in_the_fish_core::agent;
use brain_in_the_fish_core::criteria;
use brain_in_the_fish_core::snn;

#[test]
fn test_eds_full_flow() {
    let framework = criteria::academic_essay_framework();
    let agents = agent::spawn_panel("mark this essay", &framework);
    let config = snn::SNNConfig::default();

    let mut networks: Vec<snn::AgentNetwork> = agents.iter()
        .map(|a| snn::AgentNetwork::new(a, &framework.criteria))
        .collect();

    let criterion_id = &framework.criteria[0].id;

    // 1. Feed strong evidence into agent 0
    {
        let neuron = networks[0].neurons.iter_mut()
            .find(|n| n.criterion_id == *criterion_id)
            .unwrap();

        for i in 0..5u32 {
            neuron.clear_refractory();
            neuron.receive_spike(snn::Spike {
                source_id: format!("ev-strong-{}", i),
                strength: 0.9,
                spike_type: snn::SpikeType::QuantifiedData,
                timestep: i,
            }, &config);
        }
    }

    // 2. Feed weaker evidence into agent 1
    {
        let neuron = networks[1].neurons.iter_mut()
            .find(|n| n.criterion_id == *criterion_id)
            .unwrap();

        for i in 0..2u32 {
            neuron.clear_refractory();
            neuron.receive_spike(snn::Spike {
                source_id: format!("ev-weak-{}", i),
                strength: 0.4,
                spike_type: snn::SpikeType::Claim,
                timestep: i,
            }, &config);
        }
    }

    // 3. Score both
    let scores_0 = networks[0].compute_scores(&framework.criteria, &config);
    let scores_1 = networks[1].compute_scores(&framework.criteria, &config);

    let score_0 = scores_0.iter().find(|(cid, _)| cid == criterion_id).unwrap().1.snn_score;
    let score_1 = scores_1.iter().find(|(cid, _)| cid == criterion_id).unwrap().1.snn_score;

    assert!(score_0 > score_1, "Agent with more evidence should score higher: {} vs {}", score_0, score_1);

    // 4. Challenge agent 0
    let pre_challenge = score_0;
    networks[0].inhibit(criterion_id, 0.15);
    let scores_after = networks[0].compute_scores(&framework.criteria, &config);
    let post_challenge = scores_after.iter().find(|(cid, _)| cid == criterion_id).unwrap().1.snn_score;

    assert!(post_challenge < pre_challenge, "Challenge should reduce score: {} -> {}", pre_challenge, post_challenge);

    // 5. Consensus check
    let mut variances = Vec::new();
    for criterion in &framework.criteria {
        let mut scores_for_crit = Vec::new();
        for network in &networks {
            let scores = network.compute_scores(&framework.criteria, &config);
            if let Some((_, s)) = scores.iter().find(|(cid, _)| *cid == criterion.id) {
                scores_for_crit.push(s.snn_score);
            }
        }
        if scores_for_crit.len() >= 2 {
            let mean = scores_for_crit.iter().sum::<f64>() / scores_for_crit.len() as f64;
            let var = scores_for_crit.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores_for_crit.len() as f64;
            variances.push(var);
        }
    }
    let avg_var = variances.iter().sum::<f64>() / variances.len().max(1) as f64;
    assert!(avg_var > 0.0, "Agents with different evidence should not fully converge");
}
