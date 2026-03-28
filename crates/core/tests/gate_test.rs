//! Tests for the gate module — verdict system without SNN.

use brain_in_the_fish_core::gate::{self, GateWeights, Verdict};
use brain_in_the_fish_core::argument_graph::{
    ArgumentGraph, ArgumentNode, ArgumentEdge, NodeType, EdgeType,
};

fn make_graph(nodes: Vec<ArgumentNode>, edges: Vec<ArgumentEdge>) -> ArgumentGraph {
    ArgumentGraph {
        doc_id: "test".into(),
        nodes,
        edges,
    }
}

#[test]
fn test_rejected_no_nodes() {
    let graph = make_graph(vec![], vec![]);
    let v = gate::check(8.0, 12.0, &graph, &GateWeights::default());
    assert!(matches!(v, Verdict::Rejected { .. }));
}

#[test]
fn test_rejected_claims_no_evidence() {
    let graph = make_graph(
        vec![
            ArgumentNode { iri: "arg:c1".into(), node_type: NodeType::SubClaim, text: "claim".into(), llm_score: Some(0.5), llm_justification: None, source_text: None, source_span: None },
            ArgumentNode { iri: "arg:c2".into(), node_type: NodeType::SubClaim, text: "claim".into(), llm_score: Some(0.5), llm_justification: None, source_text: None, source_span: None },
        ],
        vec![],
    );
    let v = gate::check(6.0, 12.0, &graph, &GateWeights::default());
    assert!(matches!(v, Verdict::Rejected { .. }));
}

#[test]
fn test_confirmed_good_evidence() {
    let graph = make_graph(
        vec![
            ArgumentNode { iri: "arg:t1".into(), node_type: NodeType::Thesis, text: "thesis".into(), llm_score: Some(0.8), llm_justification: None, source_text: None, source_span: None },
            ArgumentNode { iri: "arg:e1".into(), node_type: NodeType::Evidence, text: "evidence".into(), llm_score: Some(0.8), llm_justification: None, source_text: None, source_span: None },
            ArgumentNode { iri: "arg:e2".into(), node_type: NodeType::Citation, text: "citation".into(), llm_score: Some(0.75), llm_justification: None, source_text: None, source_span: None },
        ],
        vec![
            ArgumentEdge { from: "arg:e1".into(), edge_type: EdgeType::Supports, to: "arg:t1".into() },
            ArgumentEdge { from: "arg:e2".into(), edge_type: EdgeType::Supports, to: "arg:t1".into() },
        ],
    );
    // Score consistent with evidence quality ~0.78
    let v = gate::check(8.0, 12.0, &graph, &GateWeights::default());
    assert!(matches!(v, Verdict::Confirmed { .. }), "Expected Confirmed, got {}", v);
}

#[test]
fn test_flagged_score_exceeds_evidence() {
    let graph = make_graph(
        vec![
            ArgumentNode { iri: "arg:t1".into(), node_type: NodeType::Thesis, text: "thesis".into(), llm_score: Some(0.3), llm_justification: None, source_text: None, source_span: None },
            ArgumentNode { iri: "arg:e1".into(), node_type: NodeType::Evidence, text: "weak evidence".into(), llm_score: Some(0.2), llm_justification: None, source_text: None, source_span: None },
        ],
        vec![
            ArgumentEdge { from: "arg:e1".into(), edge_type: EdgeType::Supports, to: "arg:t1".into() },
        ],
    );
    // High LLM score with weak evidence
    let v = gate::check(10.0, 12.0, &graph, &GateWeights::default());
    assert!(matches!(v, Verdict::Flagged { .. }), "Expected Flagged, got {}", v);
}

#[test]
fn test_quality_tightens_tolerance() {
    // Same structure, different quality → different verdicts
    let make = |quality: f64| {
        make_graph(
            vec![
                ArgumentNode { iri: "arg:t1".into(), node_type: NodeType::Thesis, text: "t".into(), llm_score: Some(quality), llm_justification: None, source_text: None, source_span: None },
                ArgumentNode { iri: "arg:e1".into(), node_type: NodeType::Evidence, text: "e".into(), llm_score: Some(quality), llm_justification: None, source_text: None, source_span: None },
                ArgumentNode { iri: "arg:e2".into(), node_type: NodeType::Evidence, text: "e".into(), llm_score: Some(quality), llm_justification: None, source_text: None, source_span: None },
            ],
            vec![
                ArgumentEdge { from: "arg:e1".into(), edge_type: EdgeType::Supports, to: "arg:t1".into() },
                ArgumentEdge { from: "arg:e2".into(), edge_type: EdgeType::Supports, to: "arg:t1".into() },
            ],
        )
    };

    let w = GateWeights::default();

    // High quality evidence: LLM score 7/10 should be confirmed
    let v_high = gate::check(7.0, 10.0, &make(0.75), &w);
    // Low quality evidence: same LLM score should be flagged
    let v_low = gate::check(7.0, 10.0, &make(0.20), &w);

    assert!(matches!(v_high, Verdict::Confirmed { .. }), "High quality should confirm, got {}", v_high);
    assert!(matches!(v_low, Verdict::Flagged { .. }), "Low quality should flag, got {}", v_low);
}

#[test]
fn test_custom_weights() {
    let graph = make_graph(
        vec![
            ArgumentNode { iri: "arg:t1".into(), node_type: NodeType::Thesis, text: "t".into(), llm_score: Some(0.5), llm_justification: None, source_text: None, source_span: None },
            ArgumentNode { iri: "arg:e1".into(), node_type: NodeType::Evidence, text: "e".into(), llm_score: Some(0.5), llm_justification: None, source_text: None, source_span: None },
        ],
        vec![
            ArgumentEdge { from: "arg:e1".into(), edge_type: EdgeType::Supports, to: "arg:t1".into() },
        ],
    );

    // Very tight tolerance should flag more
    let tight = GateWeights { gate_a: 0.01, gate_b: 0.01 };
    let v = gate::check(8.0, 12.0, &graph, &tight);
    assert!(matches!(v, Verdict::Flagged { .. }), "Tight weights should flag, got {}", v);

    // Very loose tolerance should confirm more
    let loose = GateWeights { gate_a: 0.20, gate_b: 0.10 };
    let v = gate::check(8.0, 12.0, &graph, &loose);
    assert!(matches!(v, Verdict::Confirmed { .. }), "Loose weights should confirm, got {}", v);
}
