//! Argument graph — converts documents into OWL ontologies for graph-aware scoring.
//!
//! An essay's argument structure is represented as a typed knowledge graph:
//! nodes (claims, evidence, warrants, counters) connected by edges (supports,
//! counters, warrants). The subagent scores individual nodes; the SNN aggregates
//! using graph topology (PageRank-weighted).

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use open_ontologies::graph::GraphStore;

use crate::extract;
use crate::types::{EvalDocument, Section};

/// Types of nodes in the argument graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    Thesis,
    SubClaim,
    Evidence,
    QuantifiedEvidence,
    Citation,
    Counter,
    Rebuttal,
    Structural, // intro, conclusion, transition
}

/// Types of edges in the argument graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    Supports,
    Warrants,
    Counters,
    Rebuts,
    Contains,
    References,
}

/// A node in the argument graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentNode {
    pub iri: String,
    pub node_type: NodeType,
    pub text: String,
    /// Filled by the subagent (0.0 - 1.0).
    pub llm_score: Option<f64>,
    /// Filled by the subagent.
    pub llm_justification: Option<String>,
}

/// A directed edge in the argument graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentEdge {
    pub from: String,
    pub edge_type: EdgeType,
    pub to: String,
}

/// The full argument graph for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentGraph {
    pub doc_id: String,
    pub nodes: Vec<ArgumentNode>,
    pub edges: Vec<ArgumentEdge>,
}

/// Subagent-produced node scores for the benchmark path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphScoreEntry {
    pub id: String,
    pub node_scores: Vec<NodeScoreEntry>,
    /// OWL Turtle produced by /sketch subagent (Option B full mode)
    #[serde(default)]
    pub turtle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScoreEntry {
    pub node_iri: String,
    pub score: f64,
    pub justification: String,
}

/// Build an argument graph from subagent-produced OWL Turtle + node scores.
/// Loads Turtle into GraphStore, extracts nodes and edges via SPARQL.
pub fn build_from_turtle(
    doc_id: &str,
    turtle: &str,
    node_scores: &[NodeScoreEntry],
) -> anyhow::Result<ArgumentGraph> {
    let store = GraphStore::new();
    store.load_turtle(turtle, Some("http://brain-in-the-fish.dev/arg/"))?;

    // Extract nodes via SPARQL
    let node_query = r#"
        PREFIX arg: <http://brain-in-the-fish.dev/arg/>
        SELECT ?node ?type ?text WHERE {
            ?node a ?type .
            OPTIONAL { ?node arg:hasText ?text }
            FILTER(?type IN (
                arg:Thesis, arg:SubClaim, arg:Evidence, arg:QuantifiedEvidence,
                arg:Citation, arg:Counter, arg:Rebuttal, arg:Structural
            ))
        }
    "#;

    let node_results = store.sparql_select(node_query)?;
    let node_json: serde_json::Value = serde_json::from_str(&node_results)?;

    let mut nodes = Vec::new();
    let score_map: HashMap<String, &NodeScoreEntry> = node_scores.iter()
        .map(|ns| (ns.node_iri.clone(), ns))
        .collect();

    // GraphStore returns {"results": [{...}], "variables": [...]}
    // Values are wrapped in angle brackets: "<http://...>"
    let bindings = node_json["results"].as_array()
        .or_else(|| node_json["results"]["bindings"].as_array());

    if let Some(bindings) = bindings {
        for binding in bindings {
            let raw_iri = binding["node"].as_str().unwrap_or("");
            let iri = raw_iri.trim_start_matches('<').trim_end_matches('>').to_string();
            let raw_type = binding["type"].as_str().unwrap_or("");
            let type_iri = raw_type.trim_start_matches('<').trim_end_matches('>');
            let raw_text = binding["text"].as_str().unwrap_or("");
            let text = raw_text.trim_start_matches('"').trim_end_matches('"').to_string();

            // Skip non-argument-node types (owl:Class, owl:ObjectProperty, etc.)
            if !type_iri.contains("brain-in-the-fish.dev/arg/") {
                continue;
            }
            let node_type = match type_iri {
                t if t.contains("Thesis") => NodeType::Thesis,
                t if t.contains("QuantifiedEvidence") => NodeType::QuantifiedEvidence,
                t if t.contains("Citation") => NodeType::Citation,
                t if t.contains("Evidence") => NodeType::Evidence,
                t if t.contains("Counter") => NodeType::Counter,
                t if t.contains("Rebuttal") => NodeType::Rebuttal,
                t if t.contains("Structural") => NodeType::Structural,
                t if t.contains("SubClaim") => NodeType::SubClaim,
                _ => continue, // Skip unknown types
            };

            // Match node scores by IRI (try both full IRI and short form)
            let short_iri = iri.replace("http://brain-in-the-fish.dev/arg/", "arg:");
            let ns = score_map.get(&iri)
                .or_else(|| score_map.get(&short_iri));

            nodes.push(ArgumentNode {
                iri: short_iri.clone(),
                node_type,
                text,
                llm_score: ns.map(|n| n.score.clamp(0.0, 1.0)),
                llm_justification: ns.map(|n| n.justification.clone()),
            });
        }
    }

    // Extract edges via SPARQL
    let edge_query = r#"
        PREFIX arg: <http://brain-in-the-fish.dev/arg/>
        SELECT ?from ?prop ?to WHERE {
            ?from ?prop ?to .
            FILTER(?prop IN (arg:supports, arg:warrants, arg:counters, arg:rebuts, arg:contains, arg:references))
        }
    "#;

    let edge_results = store.sparql_select(edge_query)?;
    let edge_json: serde_json::Value = serde_json::from_str(&edge_results)?;

    let mut edges = Vec::new();
    let edge_bindings = edge_json["results"].as_array()
        .or_else(|| edge_json["results"]["bindings"].as_array());

    if let Some(bindings) = edge_bindings {
        for binding in bindings {
            let from = binding["from"].as_str().unwrap_or("")
                .trim_start_matches('<').trim_end_matches('>')
                .replace("http://brain-in-the-fish.dev/arg/", "arg:");
            let prop = binding["prop"].as_str().unwrap_or("")
                .trim_start_matches('<').trim_end_matches('>');
            let to = binding["to"].as_str().unwrap_or("")
                .trim_start_matches('<').trim_end_matches('>')
                .replace("http://brain-in-the-fish.dev/arg/", "arg:");

            let edge_type = match prop {
                p if p.contains("supports") => EdgeType::Supports,
                p if p.contains("warrants") => EdgeType::Warrants,
                p if p.contains("counters") => EdgeType::Counters,
                p if p.contains("rebuts") => EdgeType::Rebuts,
                p if p.contains("contains") => EdgeType::Contains,
                p if p.contains("references") => EdgeType::References,
                _ => continue,
            };

            edges.push(ArgumentEdge { from, edge_type, to });
        }
    }

    Ok(ArgumentGraph {
        doc_id: doc_id.to_string(),
        nodes,
        edges,
    })
}

/// Build an argument graph from an EvalDocument.
/// Uses the extract module to find claims/evidence, then infers edges.
pub fn build_from_document(doc: &EvalDocument) -> ArgumentGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut thesis_iri: Option<String> = None;

    for (si, section) in doc.sections.iter().enumerate() {
        let section_iri = format!("arg:section_{}", si);

        // Section as structural node
        nodes.push(ArgumentNode {
            iri: section_iri.clone(),
            node_type: NodeType::Structural,
            text: section.title.clone(),
            llm_score: None,
            llm_justification: None,
        });

        // Extract items from this section
        let items = extract::extract_all(&section.text);
        let mut section_claim_iris: Vec<String> = Vec::new();

        for (ii, item) in items.iter().enumerate() {
            let node_iri = format!("arg:s{}_{}", si, ii);
            let node_type = match item.item_type {
                extract::ExtractedType::Claim | extract::ExtractedType::Argument => {
                    if thesis_iri.is_none() && item.confidence > 0.7 {
                        thesis_iri = Some(node_iri.clone());
                        NodeType::Thesis
                    } else {
                        NodeType::SubClaim
                    }
                }
                extract::ExtractedType::Statistic => NodeType::QuantifiedEvidence,
                extract::ExtractedType::Citation => NodeType::Citation,
                extract::ExtractedType::Evidence => NodeType::Evidence,
                extract::ExtractedType::Prediction | extract::ExtractedType::Commitment => {
                    NodeType::SubClaim
                }
            };

            nodes.push(ArgumentNode {
                iri: node_iri.clone(),
                node_type: node_type.clone(),
                text: item.text.clone(),
                llm_score: None,
                llm_justification: None,
            });

            // Section contains this node
            edges.push(ArgumentEdge {
                from: section_iri.clone(),
                edge_type: EdgeType::Contains,
                to: node_iri.clone(),
            });

            // Evidence/citation supports the nearest claim
            match node_type {
                NodeType::Evidence
                | NodeType::QuantifiedEvidence
                | NodeType::Citation => {
                    if let Some(last_claim) = section_claim_iris.last() {
                        edges.push(ArgumentEdge {
                            from: node_iri.clone(),
                            edge_type: EdgeType::Supports,
                            to: last_claim.clone(),
                        });
                    }
                }
                NodeType::SubClaim | NodeType::Thesis => {
                    section_claim_iris.push(node_iri.clone());
                    // Sub-claims support the thesis
                    if let Some(t) = &thesis_iri
                        && node_iri != *t
                    {
                        edges.push(ArgumentEdge {
                            from: node_iri.clone(),
                            edge_type: EdgeType::Supports,
                            to: t.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        // Also process existing claims/evidence from the section
        add_section_items(&mut nodes, &mut edges, section, si, &thesis_iri, &mut section_claim_iris);
    }

    // If no thesis found, promote the first claim
    if thesis_iri.is_none()
        && let Some(node) = nodes.iter_mut().find(|n| n.node_type == NodeType::SubClaim)
    {
        node.node_type = NodeType::Thesis;
    }

    ArgumentGraph {
        doc_id: doc.title.clone(),
        nodes,
        edges,
    }
}

/// Add nodes from section's pre-existing claims/evidence fields.
fn add_section_items(
    nodes: &mut Vec<ArgumentNode>,
    edges: &mut Vec<ArgumentEdge>,
    section: &Section,
    si: usize,
    thesis_iri: &Option<String>,
    section_claim_iris: &mut Vec<String>,
) {
    let section_iri = format!("arg:section_{}", si);

    for (ci, claim) in section.claims.iter().enumerate() {
        let iri = format!("arg:s{}_c{}", si, ci);
        // Skip if we already have a node with similar text
        if nodes.iter().any(|n| n.text == claim.text) {
            continue;
        }
        nodes.push(ArgumentNode {
            iri: iri.clone(),
            node_type: NodeType::SubClaim,
            text: claim.text.clone(),
            llm_score: None,
            llm_justification: None,
        });
        edges.push(ArgumentEdge {
            from: section_iri.clone(),
            edge_type: EdgeType::Contains,
            to: iri.clone(),
        });
        section_claim_iris.push(iri.clone());
        if let Some(t) = thesis_iri {
            edges.push(ArgumentEdge {
                from: iri,
                edge_type: EdgeType::Supports,
                to: t.clone(),
            });
        }
    }

    for (ei, ev) in section.evidence.iter().enumerate() {
        let iri = format!("arg:s{}_e{}", si, ei);
        if nodes.iter().any(|n| n.text == ev.text) {
            continue;
        }
        let node_type = if ev.has_quantified_outcome {
            NodeType::QuantifiedEvidence
        } else if ev.evidence_type == "citation" {
            NodeType::Citation
        } else {
            NodeType::Evidence
        };
        nodes.push(ArgumentNode {
            iri: iri.clone(),
            node_type,
            text: ev.text.clone(),
            llm_score: None,
            llm_justification: None,
        });
        edges.push(ArgumentEdge {
            from: section_iri.clone(),
            edge_type: EdgeType::Contains,
            to: iri.clone(),
        });
        if let Some(last_claim) = section_claim_iris.last() {
            edges.push(ArgumentEdge {
                from: iri,
                edge_type: EdgeType::Supports,
                to: last_claim.clone(),
            });
        }
    }
}

/// Build an argument graph from raw text (convenience for benchmark).
pub fn build_from_text(text: &str, doc_id: &str) -> ArgumentGraph {
    let mut doc = EvalDocument::new(doc_id.to_string(), "essay".into());
    let word_count = text.split_whitespace().count() as u32;
    let mut section = Section {
        id: "s0".into(),
        title: "main".into(),
        text: text.to_string(),
        word_count,
        page_range: None,
        claims: vec![],
        evidence: vec![],
        subsections: vec![],
    };
    let extracted = extract::extract_all(text);
    let (claims, evidence) = extract::to_claims_and_evidence(&extracted);
    section.claims = claims;
    section.evidence = evidence;
    doc.sections.push(section);
    doc.total_word_count = Some(word_count);
    build_from_document(&doc)
}

/// Build an argument graph directly from subagent node scores.
/// The subagent determines the graph structure — no regex extraction needed.
/// Nodes are inferred from the scores: first node = thesis, subsequent = sub-claims/evidence.
/// Edges: each node supports the thesis, sequential nodes support each other.
pub fn build_from_node_scores(doc_id: &str, node_scores: &[NodeScoreEntry]) -> ArgumentGraph {
    if node_scores.is_empty() {
        return ArgumentGraph {
            doc_id: doc_id.to_string(),
            nodes: vec![],
            edges: vec![],
        };
    }

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut thesis_iri: Option<String> = None;

    for (i, ns) in node_scores.iter().enumerate() {
        // Infer node type from position and justification
        let node_type = if i == 0 {
            NodeType::Thesis
        } else if ns.justification.to_lowercase().contains("evidence")
            || ns.justification.to_lowercase().contains("statistic")
            || ns.justification.to_lowercase().contains("data")
            || ns.justification.to_lowercase().contains("citation")
            || ns.justification.to_lowercase().contains("quote")
        {
            if ns.justification.to_lowercase().contains("statistic")
                || ns.justification.to_lowercase().contains("quantif")
                || ns.justification.to_lowercase().contains("number")
            {
                NodeType::QuantifiedEvidence
            } else if ns.justification.to_lowercase().contains("citation")
                || ns.justification.to_lowercase().contains("quote")
                || ns.justification.to_lowercase().contains("reference")
            {
                NodeType::Citation
            } else {
                NodeType::Evidence
            }
        } else if ns.justification.to_lowercase().contains("counter") {
            NodeType::Counter
        } else if ns.justification.to_lowercase().contains("conclusion")
            || ns.justification.to_lowercase().contains("closing")
        {
            NodeType::Structural
        } else {
            NodeType::SubClaim
        };

        let iri = ns.node_iri.clone();

        if i == 0 {
            thesis_iri = Some(iri.clone());
        }

        nodes.push(ArgumentNode {
            iri: iri.clone(),
            node_type,
            text: ns.justification.clone(), // use justification as text since we don't have the raw text
            llm_score: Some(ns.score.clamp(0.0, 1.0)),
            llm_justification: Some(ns.justification.clone()),
        });

        // Edges: evidence/sub-claims support the thesis
        if let Some(t_iri) = &thesis_iri
            && i > 0
            && iri != *t_iri
        {
            edges.push(ArgumentEdge {
                from: iri.clone(),
                edge_type: EdgeType::Supports,
                to: t_iri.clone(),
            });
        }

        // Sequential support: node N supports node N-1 (argument chain)
        if i > 1 {
            let prev_iri = node_scores[i - 1].node_iri.clone();
            edges.push(ArgumentEdge {
                from: iri,
                edge_type: EdgeType::Supports,
                to: prev_iri,
            });
        }
    }

    ArgumentGraph {
        doc_id: doc_id.to_string(),
        nodes,
        edges,
    }
}

/// Generate OWL Turtle representation of the argument graph.
pub fn to_turtle(graph: &ArgumentGraph) -> String {
    let mut ttl = String::from(
        "@prefix arg: <http://brain-in-the-fish.dev/arg/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n\
         arg:ArgumentNode a owl:Class .\n\
         arg:Thesis rdfs:subClassOf arg:ArgumentNode .\n\
         arg:SubClaim rdfs:subClassOf arg:ArgumentNode .\n\
         arg:Evidence rdfs:subClassOf arg:ArgumentNode .\n\
         arg:QuantifiedEvidence rdfs:subClassOf arg:Evidence .\n\
         arg:Citation rdfs:subClassOf arg:Evidence .\n\
         arg:Counter rdfs:subClassOf arg:ArgumentNode .\n\
         arg:Rebuttal rdfs:subClassOf arg:ArgumentNode .\n\
         arg:Structural rdfs:subClassOf arg:ArgumentNode .\n\n\
         arg:supports a owl:ObjectProperty .\n\
         arg:warrants a owl:ObjectProperty .\n\
         arg:counters a owl:ObjectProperty .\n\
         arg:rebuts a owl:ObjectProperty .\n\
         arg:contains a owl:ObjectProperty .\n\
         arg:references a owl:ObjectProperty .\n\
         arg:hasScore a owl:DatatypeProperty .\n\
         arg:hasText a owl:DatatypeProperty .\n\n",
    );

    for node in &graph.nodes {
        let class = match node.node_type {
            NodeType::Thesis => "arg:Thesis",
            NodeType::SubClaim => "arg:SubClaim",
            NodeType::Evidence => "arg:Evidence",
            NodeType::QuantifiedEvidence => "arg:QuantifiedEvidence",
            NodeType::Citation => "arg:Citation",
            NodeType::Counter => "arg:Counter",
            NodeType::Rebuttal => "arg:Rebuttal",
            NodeType::Structural => "arg:Structural",
        };
        let escaped_text = node.text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ");
        ttl.push_str(&format!(
            "<{}> a {} ;\n    arg:hasText \"{}\"^^xsd:string",
            node.iri, class, escaped_text
        ));
        if let Some(score) = node.llm_score {
            ttl.push_str(&format!(" ;\n    arg:hasScore \"{:.3}\"^^xsd:float", score));
        }
        ttl.push_str(" .\n\n");
    }

    for edge in &graph.edges {
        let prop = match edge.edge_type {
            EdgeType::Supports => "arg:supports",
            EdgeType::Warrants => "arg:warrants",
            EdgeType::Counters => "arg:counters",
            EdgeType::Rebuts => "arg:rebuts",
            EdgeType::Contains => "arg:contains",
            EdgeType::References => "arg:references",
        };
        ttl.push_str(&format!("<{}> {} <{}> .\n", edge.from, prop, edge.to));
    }

    ttl
}

/// Load the argument graph into an open-ontologies GraphStore.
pub fn load_into_graph_store(graph: &ArgumentGraph) -> anyhow::Result<Arc<GraphStore>> {
    let store = Arc::new(GraphStore::new());
    let ttl = to_turtle(graph);
    store.load_turtle(&ttl, Some("http://brain-in-the-fish.dev/arg/"))?;
    Ok(store)
}

/// Compute PageRank over the argument graph.
/// Returns a map from node IRI to PageRank weight (0.0 - 1.0, normalized).
pub fn compute_pagerank(graph: &ArgumentGraph, damping: f64, iterations: usize) -> HashMap<String, f64> {
    let n = graph.nodes.len();
    if n == 0 {
        return HashMap::new();
    }

    let iris: Vec<&str> = graph.nodes.iter().map(|n| n.iri.as_str()).collect();
    let iri_to_idx: HashMap<&str, usize> = iris.iter().enumerate().map(|(i, iri)| (*iri, i)).collect();

    // Build adjacency: only scoring-relevant edges (Supports, Warrants, Rebuts)
    let mut outlinks: Vec<Vec<usize>> = vec![vec![]; n];
    let mut inlinks: Vec<Vec<usize>> = vec![vec![]; n];
    for edge in &graph.edges {
        match edge.edge_type {
            EdgeType::Supports | EdgeType::Warrants | EdgeType::Rebuts => {
                if let (Some(&from), Some(&to)) = (iri_to_idx.get(edge.from.as_str()), iri_to_idx.get(edge.to.as_str())) {
                    outlinks[from].push(to);
                    inlinks[to].push(from);
                }
            }
            _ => {} // Contains, References, Counters don't transfer PageRank
        }
    }

    let mut scores = vec![1.0 / n as f64; n];
    let floor = 1.0 / (n as f64 * 10.0); // minimum weight to prevent zero

    for _ in 0..iterations {
        let mut new_scores = vec![(1.0 - damping) / n as f64; n];
        for i in 0..n {
            if outlinks[i].is_empty() {
                // Dangling node: distribute evenly
                let share = damping * scores[i] / n as f64;
                for ns in &mut new_scores {
                    *ns += share;
                }
            } else {
                let share = damping * scores[i] / outlinks[i].len() as f64;
                for &target in &outlinks[i] {
                    new_scores[target] += share;
                }
            }
        }
        scores = new_scores;
    }

    // Normalize to [0, 1] and apply floor
    let max = scores.iter().cloned().fold(0.0f64, f64::max);
    let mut result = HashMap::new();
    for (i, iri) in iris.iter().enumerate() {
        let normalized = if max > 0.0 { scores[i] / max } else { floor };
        result.insert(iri.to_string(), normalized.max(floor));
    }
    result
}

/// Compute structural metrics from the argument graph.
/// These become additional signals for the SNN.
pub struct GraphMetrics {
    pub node_count: usize,
    pub edge_count: usize,
    pub claim_count: usize,
    pub evidence_count: usize,
    pub max_depth: usize,
    pub connectivity: f64,      // fraction of nodes with >=1 edge
    pub evidence_coverage: f64, // fraction of claims with supporting evidence
    pub has_counter: bool,
    pub has_rebuttal: bool,
}

pub fn compute_metrics(graph: &ArgumentGraph) -> GraphMetrics {
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();

    let claim_count = graph.nodes.iter()
        .filter(|n| matches!(n.node_type, NodeType::Thesis | NodeType::SubClaim))
        .count();
    let evidence_count = graph.nodes.iter()
        .filter(|n| matches!(n.node_type, NodeType::Evidence | NodeType::QuantifiedEvidence | NodeType::Citation))
        .count();

    // Connectivity: fraction of non-structural nodes with at least one scoring edge
    let non_structural: Vec<&str> = graph.nodes.iter()
        .filter(|n| n.node_type != NodeType::Structural)
        .map(|n| n.iri.as_str())
        .collect();
    let connected = non_structural.iter().filter(|iri| {
        graph.edges.iter().any(|e| {
            matches!(e.edge_type, EdgeType::Supports | EdgeType::Warrants | EdgeType::Counters | EdgeType::Rebuts)
                && (e.from == **iri || e.to == **iri)
        })
    }).count();
    let connectivity = if non_structural.is_empty() {
        0.0
    } else {
        connected as f64 / non_structural.len() as f64
    };

    // Evidence coverage: fraction of claims that have at least one incoming Supports edge
    let claims_with_support = graph.nodes.iter()
        .filter(|n| matches!(n.node_type, NodeType::Thesis | NodeType::SubClaim))
        .filter(|n| {
            graph.edges.iter().any(|e| e.to == n.iri && e.edge_type == EdgeType::Supports)
        })
        .count();
    let evidence_coverage = if claim_count == 0 {
        0.0
    } else {
        claims_with_support as f64 / claim_count as f64
    };

    // Depth: longest chain of Supports edges
    let max_depth = compute_max_depth(graph);

    let has_counter = graph.nodes.iter().any(|n| n.node_type == NodeType::Counter);
    let has_rebuttal = graph.nodes.iter().any(|n| n.node_type == NodeType::Rebuttal);

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

fn compute_max_depth(graph: &ArgumentGraph) -> usize {
    let iri_to_idx: HashMap<&str, usize> = graph.nodes.iter()
        .enumerate()
        .map(|(i, n)| (n.iri.as_str(), i))
        .collect();

    let mut children: Vec<Vec<usize>> = vec![vec![]; graph.nodes.len()];
    for edge in &graph.edges {
        if edge.edge_type == EdgeType::Supports
            && let (Some(&from), Some(&to)) = (iri_to_idx.get(edge.from.as_str()), iri_to_idx.get(edge.to.as_str()))
        {
            children[to].push(from); // supports flows toward target
        }
    }

    fn dfs(node: usize, children: &[Vec<usize>], visited: &mut Vec<bool>) -> usize {
        visited[node] = true;
        let mut max = 0;
        for &child in &children[node] {
            if !visited[child] {
                max = max.max(1 + dfs(child, children, visited));
            }
        }
        max
    }

    let mut max_depth = 0;
    for i in 0..graph.nodes.len() {
        let mut visited = vec![false; graph.nodes.len()];
        max_depth = max_depth.max(dfs(i, &children, &mut visited));
    }
    max_depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_from_text_empty() {
        let graph = build_from_text("", "test");
        assert!(graph.nodes.is_empty() || graph.nodes.len() == 1); // just the section node
    }

    #[test]
    fn test_build_from_text_simple() {
        let text = "According to Smith (2020), GDP grew 3.5% in Q4. This demonstrates that the policy was effective.";
        let graph = build_from_text(text, "test");
        assert!(!graph.nodes.is_empty());
        assert!(!graph.edges.is_empty());
    }

    #[test]
    fn test_pagerank_simple() {
        let graph = ArgumentGraph {
            doc_id: "test".into(),
            nodes: vec![
                ArgumentNode { iri: "a".into(), node_type: NodeType::Thesis, text: "thesis".into(), llm_score: None, llm_justification: None },
                ArgumentNode { iri: "b".into(), node_type: NodeType::SubClaim, text: "claim".into(), llm_score: None, llm_justification: None },
                ArgumentNode { iri: "c".into(), node_type: NodeType::Evidence, text: "evidence".into(), llm_score: None, llm_justification: None },
            ],
            edges: vec![
                ArgumentEdge { from: "b".into(), edge_type: EdgeType::Supports, to: "a".into() },
                ArgumentEdge { from: "c".into(), edge_type: EdgeType::Supports, to: "b".into() },
            ],
        };
        let pr = compute_pagerank(&graph, 0.85, 20);
        // Thesis should have highest PageRank (most supported)
        assert!(pr["a"] > pr["c"], "Thesis should rank higher than leaf evidence");
    }

    #[test]
    fn test_build_from_turtle() {
        let ttl = r#"
@prefix arg: <http://brain-in-the-fish.dev/arg/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

arg:ArgumentNode a owl:Class .
arg:Thesis rdfs:subClassOf arg:ArgumentNode .
arg:SubClaim rdfs:subClassOf arg:ArgumentNode .
arg:Evidence rdfs:subClassOf arg:ArgumentNode .

arg:supports a owl:ObjectProperty .
arg:hasText a owl:DatatypeProperty .

arg:thesis_1 a arg:Thesis ;
    arg:hasText "Test thesis" .
arg:claim_2 a arg:SubClaim ;
    arg:hasText "Test claim" ;
    arg:supports arg:thesis_1 .
arg:evidence_3 a arg:Evidence ;
    arg:hasText "Test evidence" ;
    arg:supports arg:claim_2 .
"#;
        let scores = vec![
            NodeScoreEntry { node_iri: "arg:thesis_1".into(), score: 0.8, justification: "good".into() },
            NodeScoreEntry { node_iri: "arg:claim_2".into(), score: 0.6, justification: "ok".into() },
            NodeScoreEntry { node_iri: "arg:evidence_3".into(), score: 0.7, justification: "solid".into() },
        ];
        // Debug: check what's in the store
        let store = GraphStore::new();
        let count = store.load_turtle(ttl, Some("http://brain-in-the-fish.dev/arg/")).unwrap();
        eprintln!("Loaded {} triples", count);
        let all = store.sparql_select("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 20").unwrap();
        eprintln!("All triples: {}", all);
        let typed = store.sparql_select("PREFIX arg: <http://brain-in-the-fish.dev/arg/> SELECT ?node ?type WHERE { ?node a ?type }").unwrap();
        eprintln!("Typed: {}", typed);

        let graph = build_from_turtle("test", ttl, &scores).expect("should parse");
        eprintln!("Nodes: {:?}", graph.nodes.iter().map(|n| (&n.iri, &n.node_type)).collect::<Vec<_>>());
        eprintln!("Edges: {:?}", graph.edges.iter().map(|e| (&e.from, &e.edge_type, &e.to)).collect::<Vec<_>>());
        assert!(!graph.nodes.is_empty(), "Should find nodes, got 0");
        assert!(!graph.edges.is_empty(), "Should find edges, got 0");
        assert!(graph.nodes.iter().any(|n| n.llm_score.is_some()), "Scores should be matched");
    }

    #[test]
    fn test_to_turtle_valid() {
        let graph = build_from_text("Smith (2020) found that test scores improved by 15%.", "test");
        let ttl = to_turtle(&graph);
        assert!(ttl.contains("arg:ArgumentNode"));
        assert!(ttl.contains("arg:supports") || ttl.contains("arg:contains"));
    }

    #[test]
    fn test_graph_metrics() {
        let graph = ArgumentGraph {
            doc_id: "test".into(),
            nodes: vec![
                ArgumentNode { iri: "t".into(), node_type: NodeType::Thesis, text: "thesis".into(), llm_score: None, llm_justification: None },
                ArgumentNode { iri: "c1".into(), node_type: NodeType::SubClaim, text: "claim1".into(), llm_score: None, llm_justification: None },
                ArgumentNode { iri: "e1".into(), node_type: NodeType::Evidence, text: "evidence1".into(), llm_score: None, llm_justification: None },
            ],
            edges: vec![
                ArgumentEdge { from: "c1".into(), edge_type: EdgeType::Supports, to: "t".into() },
                ArgumentEdge { from: "e1".into(), edge_type: EdgeType::Supports, to: "c1".into() },
            ],
        };
        let m = compute_metrics(&graph);
        assert_eq!(m.claim_count, 2);
        assert_eq!(m.evidence_count, 1);
        assert!(m.connectivity > 0.9);
        assert!(m.evidence_coverage > 0.0);
    }
}
