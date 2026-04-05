//! Argument graph — build from document, compute structural metrics.

use std::collections::{HashMap, HashSet};

use crate::types::{
    ArgEdge, ArgGraph, ArgNode, Document, EdgeType, GraphMetrics, NodeType,
};

/// Build an argument graph from a document's claims and evidence.
///
/// Structure: thesis node -> section subclaim nodes -> claim nodes + evidence nodes.
pub fn build_from_document(doc: &Document) -> ArgGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Thesis node
    let thesis_iri = format!("arg:thesis-{}", doc.id);
    nodes.push(ArgNode {
        iri: thesis_iri.clone(),
        node_type: NodeType::Thesis,
        text: format!("Main argument of: {}", doc.title),
        llm_score: None,
        llm_justification: None,
    });

    for section in &doc.sections {
        // Section subclaim
        let sec_iri = format!("arg:sec-{}", section.id);
        nodes.push(ArgNode {
            iri: sec_iri.clone(),
            node_type: NodeType::SubClaim,
            text: section.title.clone(),
            llm_score: None,
            llm_justification: None,
        });
        edges.push(ArgEdge {
            from: sec_iri.clone(),
            edge_type: EdgeType::Supports,
            to: thesis_iri.clone(),
        });

        // Claims as subclaims of the section
        for claim in &section.claims {
            let claim_iri = format!("arg:{}", claim.id);
            nodes.push(ArgNode {
                iri: claim_iri.clone(),
                node_type: NodeType::SubClaim,
                text: claim.text.clone(),
                llm_score: None,
                llm_justification: None,
            });
            edges.push(ArgEdge {
                from: claim_iri.clone(),
                edge_type: EdgeType::Supports,
                to: sec_iri.clone(),
            });
        }

        // Evidence nodes supporting the section
        for ev in &section.evidence {
            let ev_iri = format!("arg:{}", ev.id);
            let node_type = if ev.has_quantified_outcome {
                NodeType::QuantifiedEvidence
            } else if ev.evidence_type == "citation" {
                NodeType::Citation
            } else {
                NodeType::Evidence
            };
            nodes.push(ArgNode {
                iri: ev_iri.clone(),
                node_type,
                text: ev.text.clone(),
                llm_score: None,
                llm_justification: None,
            });
            edges.push(ArgEdge {
                from: ev_iri,
                edge_type: EdgeType::Supports,
                to: sec_iri.clone(),
            });
        }
    }

    ArgGraph {
        doc_id: doc.id.clone(),
        nodes,
        edges,
    }
}

/// Compute structural metrics from an argument graph.
pub fn compute_metrics(graph: &ArgGraph) -> GraphMetrics {
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();

    if node_count == 0 {
        return GraphMetrics::default();
    }

    let claim_count = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::Thesis | NodeType::SubClaim))
        .count();
    let evidence_count = graph
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.node_type,
                NodeType::Evidence | NodeType::QuantifiedEvidence | NodeType::Citation
            )
        })
        .count();

    // Connectivity: fraction of non-structural nodes with at least one edge
    let non_structural: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.node_type != NodeType::Structural)
        .map(|n| n.iri.as_str())
        .collect();

    let connected = non_structural
        .iter()
        .filter(|iri| {
            graph.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    EdgeType::Supports | EdgeType::Warrants | EdgeType::Counters | EdgeType::Rebuts
                ) && (e.from == **iri || e.to == **iri)
            })
        })
        .count();

    let connectivity = if non_structural.is_empty() {
        0.0
    } else {
        connected as f64 / non_structural.len() as f64
    };

    // Evidence coverage: fraction of claims with at least one supporting evidence
    let claims_with_evidence = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::Thesis | NodeType::SubClaim))
        .filter(|claim| {
            graph.edges.iter().any(|e| {
                e.to == claim.iri
                    && e.edge_type == EdgeType::Supports
                    && graph.nodes.iter().any(|n| {
                        n.iri == e.from
                            && matches!(
                                n.node_type,
                                NodeType::Evidence
                                    | NodeType::QuantifiedEvidence
                                    | NodeType::Citation
                            )
                    })
            })
        })
        .count();

    let evidence_coverage = if claim_count > 0 {
        claims_with_evidence as f64 / claim_count as f64
    } else {
        0.0
    };

    // Max depth via BFS from thesis
    let max_depth = compute_max_depth(graph);

    let has_counter = graph
        .nodes
        .iter()
        .any(|n| n.node_type == NodeType::Counter);
    let has_rebuttal = graph
        .nodes
        .iter()
        .any(|n| n.node_type == NodeType::Rebuttal);

    GraphMetrics {
        node_count,
        edge_count,
        claim_count,
        evidence_count,
        max_depth,
        connectivity,
        evidence_coverage,
        has_counter,
        has_rebuttal,
    }
}

fn compute_max_depth(graph: &ArgGraph) -> usize {
    // Build adjacency: from -> to edges (reverse direction for depth from thesis)
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if edge.edge_type == EdgeType::Supports {
            // "from supports to" means "from" is a child of "to"
            children.entry(edge.to.as_str()).or_default().push(edge.from.as_str());
        }
    }

    // Find thesis nodes
    let theses: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Thesis)
        .map(|n| n.iri.as_str())
        .collect();

    let mut max_depth = 0;
    for thesis in &theses {
        let depth = bfs_depth(thesis, &children);
        if depth > max_depth {
            max_depth = depth;
        }
    }
    max_depth
}

fn bfs_depth(root: &str, children: &HashMap<&str, Vec<&str>>) -> usize {
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((root, 0usize));
    visited.insert(root);
    let mut max_d = 0;

    while let Some((node, depth)) = queue.pop_front() {
        if depth > max_d {
            max_d = depth;
        }
        if let Some(kids) = children.get(node) {
            for &kid in kids {
                if visited.insert(kid) {
                    queue.push_back((kid, depth + 1));
                }
            }
        }
    }
    max_d
}
