//! Graph visualization — production-quality D3.js knowledge graph.
//!
//! Two extraction paths:
//! 1. [`extract_graph_from_store`] — pulls ALL triples from the Oxigraph triple store via SPARQL
//! 2. [`extract_graph_data`] — fallback from an [`EvaluationSession`] struct (no store needed)
//!
//! Both produce a [`GraphData`] which is injected into a self-contained HTML page
//! with a D3.js force-directed interactive graph, detail panel, lineage sidebar,
//! and keyboard shortcuts.

use crate::types::*;
use open_ontologies::graph::GraphStore;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Data types
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    /// Raw IRI or technical identifier — shown as a tag in the detail panel.
    pub tag: String,
    pub node_type: String,
    pub group: u32,
    pub size: f64,
    pub details: String,
    /// All properties as key-value pairs for the detail panel.
    pub properties: Vec<(String, String)>,
    /// IDs of connected nodes (for the detail panel).
    pub connected: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineageEvent {
    pub stage: String,
    pub description: String,
    pub timestamp: String,
    pub details: String,
    pub icon: String,
}

// ============================================================================
// Node type classification
// ============================================================================

/// Determine the node type string from an RDF type IRI.
fn classify_rdf_type(type_iri: &str) -> &'static str {
    if type_iri.contains("Document") {
        "Document"
    } else if type_iri.contains("Section") {
        "Section"
    } else if type_iri.contains("Claim") {
        "Claim"
    } else if type_iri.contains("Evidence") {
        "Evidence"
    } else if type_iri.contains("EvaluationFramework") {
        "Framework"
    } else if type_iri.contains("EvaluationCriterion") {
        "Criterion"
    } else if type_iri.contains("RubricLevel") {
        "RubricLevel"
    } else if type_iri.contains("EvaluatorAgent") {
        "Agent"
    } else if type_iri.contains("Need") {
        "Need"
    } else if type_iri.contains("TrustRelation") {
        "Trust"
    } else if type_iri.contains("Score") && !type_iri.contains("Moderated") {
        "Score"
    } else if type_iri.contains("Challenge") {
        "Challenge"
    } else if type_iri.contains("ModeratedScore") {
        "Moderated"
    } else {
        "default"
    }
}

fn group_for_type(node_type: &str) -> u32 {
    match node_type {
        "Document" | "Section" | "Claim" | "Evidence" => 0,
        "Framework" | "Criterion" | "RubricLevel" => 1,
        "Agent" | "Need" | "Trust" => 2,
        "Score" => 3,
        "Challenge" => 4,
        "Moderated" => 5,
        _ => 6,
    }
}

fn size_for_type(node_type: &str) -> f64 {
    match node_type {
        "Document" => 24.0,
        "Framework" => 20.0,
        "Agent" => 16.0,
        "Criterion" => 14.0,
        "Section" => 12.0,
        "Score" => 12.0,
        "Moderated" => 14.0,
        "Challenge" => 10.0,
        "Claim" | "Evidence" => 8.0,
        "RubricLevel" => 6.0,
        "Need" => 6.0,
        "Trust" => 5.0,
        _ => 8.0,
    }
}

/// Extract a short, human-readable label from an IRI.
///
/// Handles patterns like:
/// - `http://brain-in-the-fish.dev/agent/ab109dfc_Score_value_for_money_R1` → `Score: value for money (R1)`
/// - `http://brain-in-the-fish.dev/doc/ab109dfc-uuid-here` → `doc:ab10...` (truncated UUID)
/// - `http://brain-in-the-fish.dev/eval/Section` → `Section`
fn label_from_iri(iri: &str) -> String {
    let s = iri.trim_start_matches('<').trim_end_matches('>');
    // Take fragment or last path segment
    let raw = if let Some(frag) = s.rsplit_once('#') {
        frag.1.to_string()
    } else if let Some(seg) = s.rsplit_once('/') {
        seg.1.to_string()
    } else {
        s.to_string()
    };

    // If it looks like a UUID-prefixed compound label (e.g. uuid_Score_value_R1),
    // strip the UUID prefix and humanise the rest
    humanise_label(&raw)
}

/// Convert a raw IRI local name into a human-readable label.
///
/// Handles: `ab109dfc_7740_411a_af92_2b32c3fbeaec_Score_value_for_money_R1`
/// → `Score: value for money (R1)`
fn humanise_label(raw: &str) -> String {
    // Detect UUID prefix (8-4-4-4-12 hex with underscores instead of hyphens)
    // Pattern: 8 hex chars, then underscore-separated groups
    let parts: Vec<&str> = raw.splitn(6, '_').collect();
    if parts.len() >= 6
        && parts[0].len() == 8
        && parts[0].chars().all(|c| c.is_ascii_hexdigit())
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 4
        && parts[4].len() == 12
    {
        // Strip UUID prefix — the meaningful part is parts[5]
        let meaningful = parts[5];
        return format_meaningful_label(meaningful);
    }

    // Also handle hyphenated UUIDs: ab109dfc-7740-411a-af92-2b32c3fbeaec_Score_...
    if raw.len() > 36 {
        let maybe_uuid = &raw[..36];
        if maybe_uuid.chars().filter(|c| *c == '-').count() == 4
            && maybe_uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            if raw.len() > 37 {
                let meaningful = &raw[37..]; // skip UUID + separator
                return format_meaningful_label(meaningful);
            }
            // Just a UUID, truncate
            return format!("{}...", &raw[..8]);
        }
    }

    // No UUID — just replace underscores with spaces
    raw.replace('_', " ")
}

/// Format the meaningful (post-UUID) part of a label.
///
/// `Score_value_for_money_R1` → `Score: value for money (R1)`
fn format_meaningful_label(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').collect();
    if parts.is_empty() {
        return s.to_string();
    }

    // Check for round suffix like R1, R2
    let last = *parts.last().unwrap();
    let has_round = last.starts_with('R') && last[1..].parse::<u32>().is_ok();

    let (label_parts, round) = if has_round {
        (&parts[..parts.len() - 1], Some(last))
    } else {
        (&parts[..], None)
    };

    if label_parts.is_empty() {
        return round.unwrap_or("").to_string();
    }

    // First part is the type (Score, Need, Trust, etc.), rest is the name
    let type_name = label_parts[0];
    let rest = if label_parts.len() > 1 {
        label_parts[1..].join(" ")
    } else {
        String::new()
    };

    let mut label = if rest.is_empty() {
        type_name.to_string()
    } else {
        format!("{}: {}", type_name, rest)
    };

    if let Some(r) = round {
        label.push_str(&format!(" ({})", r));
    }

    label
}

/// Strip Oxigraph literal wrapping: `"value"^^<type>` → `value`.
fn strip_literal(raw: &str) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix('"') {
        // Find the closing quote (handles ^^<type> and @lang suffixes)
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    s.to_string()
}

/// Clean an IRI by removing angle brackets.
fn clean_iri(raw: &str) -> String {
    raw.trim_start_matches('<').trim_end_matches('>').to_string()
}

// ============================================================================
// Parse SPARQL JSON results (same format as scoring.rs)
// ============================================================================

fn parse_sparql_results(
    json_str: &str,
) -> Vec<HashMap<String, String>> {
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let results = match parsed.get("results").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    let mut rows = Vec::new();
    for row in results {
        if let Some(obj) = row.as_object() {
            let mut map = HashMap::new();
            for (key, val) in obj {
                if let Some(s) = val.as_str() {
                    map.insert(key.clone(), s.to_string());
                }
            }
            rows.push(map);
        }
    }
    rows
}

// ============================================================================
// Extract from Oxigraph triple store
// ============================================================================

/// Extract graph data from the actual Oxigraph triple store.
///
/// Queries ALL triples and builds a proper knowledge graph visualization.
/// Each unique subject/object IRI becomes a node, each triple becomes an edge.
/// Nodes are classified by their `rdf:type`.
pub fn extract_graph_from_store(graph: &GraphStore) -> GraphData {
    // 1. Query all type assertions to classify nodes
    let type_json = graph
        .sparql_select("SELECT ?s ?type WHERE { ?s a ?type }")
        .unwrap_or_default();
    let type_rows = parse_sparql_results(&type_json);

    let mut type_map: HashMap<String, String> = HashMap::new();
    for row in &type_rows {
        if let (Some(s), Some(t)) = (row.get("s"), row.get("type")) {
            let node_type = classify_rdf_type(t);
            // Prefer the most specific type (first wins for important types)
            let s_clean = clean_iri(s);
            type_map
                .entry(s_clean)
                .or_insert_with(|| node_type.to_string());
        }
    }

    // 2. Query ALL triples
    let all_json = graph
        .sparql_select("SELECT ?s ?p ?o WHERE { ?s ?p ?o }")
        .unwrap_or_default();
    let all_rows = parse_sparql_results(&all_json);

    let mut node_ids: HashSet<String> = HashSet::new();
    let mut node_properties: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut connected_map: HashMap<String, Vec<String>> = HashMap::new();

    for row in &all_rows {
        let (s_raw, p_raw, o_raw) = match (row.get("s"), row.get("p"), row.get("o")) {
            (Some(s), Some(p), Some(o)) => (s.clone(), p.clone(), o.clone()),
            _ => continue,
        };

        let s = clean_iri(&s_raw);
        let p = clean_iri(&p_raw);
        let p_label = label_from_iri(&p);

        // Skip rdf:type triples — already handled
        if p.contains("22-rdf-syntax-ns#type") || p.ends_with("#type") || p_label == "type" {
            continue;
        }

        node_ids.insert(s.clone());

        // Check if object is an IRI (node reference) or a literal (property)
        let o_trimmed = o_raw.trim().to_string();
        let is_iri = o_trimmed.starts_with('<') || o_trimmed.starts_with("http");

        if is_iri {
            let o = clean_iri(&o_trimmed);
            node_ids.insert(o.clone());

            // Determine edge type for styling
            let edge_type = if p_label.contains("contains") || p_label.contains("partOf") {
                "structural"
            } else if p_label.contains("trust") {
                "trust"
            } else if p_label.contains("challeng") {
                "challenge"
            } else if p_label.contains("score") || p_label.contains("scored") {
                "scores"
            } else if p_label.contains("align") {
                "evaluates"
            } else {
                "relation"
            };

            edges.push(GraphEdge {
                source: s.clone(),
                target: o.clone(),
                label: p_label.clone(),
                edge_type: edge_type.to_string(),
            });

            connected_map.entry(s.clone()).or_default().push(o.clone());
            connected_map.entry(o.clone()).or_default().push(s.clone());
        } else {
            // It's a literal — store as property
            let value = strip_literal(&o_trimmed);
            node_properties
                .entry(s.clone())
                .or_default()
                .push((p_label.clone(), value));
        }
    }

    // 3. Build nodes
    let mut nodes: Vec<GraphNode> = Vec::new();
    for id in &node_ids {
        let node_type = type_map
            .get(id)
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let group = group_for_type(&node_type);
        let size = size_for_type(&node_type);

        let properties = node_properties.get(id).cloned().unwrap_or_default();
        let connected = connected_map.get(id).cloned().unwrap_or_default();

        // Build label from properties or IRI
        let label = properties
            .iter()
            .find(|(k, _)| k == "title" || k == "name" || k == "label")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| label_from_iri(id));

        // Build details string
        let details = properties
            .iter()
            .map(|(k, v)| {
                let display_v = if v.len() > 80 {
                    format!("{}...", &v[..80])
                } else {
                    v.clone()
                };
                format!("{}: {}", k, display_v)
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Tag = last IRI segment (raw, not humanised)
        let tag = {
            let s = id.trim_start_matches('<').trim_end_matches('>');
            if let Some(frag) = s.rsplit_once('#') {
                frag.1.to_string()
            } else if let Some(seg) = s.rsplit_once('/') {
                seg.1.to_string()
            } else {
                s.to_string()
            }
        };

        nodes.push(GraphNode {
            id: id.clone(),
            label,
            tag,
            node_type,
            group,
            size,
            details,
            properties,
            connected,
        });
    }

    GraphData { nodes, edges }
}

// ============================================================================
// Extract from EvaluationSession (fallback — no store needed)
// ============================================================================

/// Extract a rich graph from an [`EvaluationSession`] struct.
///
/// This is the fallback path when no GraphStore is available. It builds nodes
/// for every entity in the session: sections, claims, evidence, criteria,
/// rubric levels, agents, Maslow needs, trust relations, scores, challenges,
/// and moderated scores.
pub fn extract_graph_data(session: &EvaluationSession) -> GraphData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Build lookup maps for human-readable labels
    let agent_names: HashMap<String, String> = session
        .agents
        .iter()
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();
    let criterion_titles: HashMap<String, String> = session
        .framework
        .criteria
        .iter()
        .map(|c| (c.id.clone(), c.title.clone()))
        .collect();

    // --- Document ---
    let doc = &session.document;
    nodes.push(GraphNode {
        id: doc.id.clone(),
        label: if doc.title.is_empty() {
            "Document".into()
        } else {
            doc.title.clone()
        },
        tag: doc.id.clone(),
        node_type: "Document".into(),
        group: 0,
        size: 24.0,
        details: format!(
            "type: {}\nwords: {}\npages: {}",
            doc.doc_type,
            doc.total_word_count.unwrap_or(0),
            doc.total_pages.map(|p| p.to_string()).unwrap_or_else(|| "—".into())
        ),
        properties: vec![
            ("type".into(), doc.doc_type.clone()),
            (
                "total_word_count".into(),
                doc.total_word_count.unwrap_or(0).to_string(),
            ),
            (
                "total_pages".into(),
                doc.total_pages.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
            ),
        ],
        connected: doc.sections.iter().map(|s| s.id.clone()).collect(),
    });

    // --- Sections, Claims, Evidence ---
    for section in &doc.sections {
        add_section_nodes(&mut nodes, &mut edges, &doc.id, section);
    }

    // --- Framework ---
    let fw = &session.framework;
    let fw_id = format!("fw_{}", fw.id);
    // Connect framework to document (ONE graph, not two clusters)
    edges.push(GraphEdge {
        source: fw_id.clone(),
        target: doc.id.clone(),
        label: "evaluates".into(),
        edge_type: "evaluates".into(),
    });

    nodes.push(GraphNode {
        id: fw_id.clone(),
        label: fw.name.clone(),
        tag: fw.id.clone(),
        node_type: "Framework".into(),
        group: 1,
        size: 20.0,
        details: format!(
            "criteria: {}\ntotal_weight: {:.1}\npass_mark: {}",
            fw.criteria.len(),
            fw.total_weight,
            fw.pass_mark.map(|p| format!("{:.1}", p)).unwrap_or_else(|| "—".into())
        ),
        properties: vec![
            ("criteria_count".into(), fw.criteria.len().to_string()),
            ("evaluators".into(), session.agents.len().to_string()),
            ("total_weight".into(), format!("{:.1}", fw.total_weight)),
            (
                "pass_mark".into(),
                fw.pass_mark.map(|p| format!("{:.1}", p)).unwrap_or_else(|| "—".into()),
            ),
        ],
        connected: fw.criteria.iter().map(|c| c.id.clone()).collect(),
    });

    // --- Criteria + Rubric Levels ---
    for criterion in &fw.criteria {
        add_criterion_nodes(&mut nodes, &mut edges, &fw_id, criterion);
    }

    // --- Agents + Maslow Needs + Trust ---
    for agent in &session.agents {
        let mut agent_connected: Vec<String> = Vec::new();

        // Maslow needs as nodes
        for (i, need) in agent.needs.iter().enumerate() {
            let need_id = format!("need_{}_{}", agent.id, i);
            let need_label = format!("{:?}", need.need_type);
            nodes.push(GraphNode {
                id: need_id.clone(),
                label: need_label.clone(),
                tag: need_id.clone(),
                node_type: "Need".into(),
                group: 2,
                size: 6.0,
                details: format!(
                    "expression: {}\nsalience: {:.2}\nsatisfied: {}",
                    need.expression, need.salience, need.satisfied
                ),
                properties: vec![
                    ("need_type".into(), need_label),
                    ("expression".into(), need.expression.clone()),
                    ("salience".into(), format!("{:.2}", need.salience)),
                    ("satisfied".into(), need.satisfied.to_string()),
                ],
                connected: vec![agent.id.clone()],
            });
            edges.push(GraphEdge {
                source: agent.id.clone(),
                target: need_id.clone(),
                label: "hasNeed".into(),
                edge_type: "structural".into(),
            });
            agent_connected.push(need_id);
        }

        // Trust relations as directed edges with weight labels
        for trust in &agent.trust_weights {
            edges.push(GraphEdge {
                source: agent.id.clone(),
                target: trust.target_agent_id.clone(),
                label: format!("trust({:.2}, {})", trust.trust_level, trust.domain),
                edge_type: "trust".into(),
            });
            agent_connected.push(trust.target_agent_id.clone());
        }

        nodes.push(GraphNode {
            id: agent.id.clone(),
            label: agent.name.clone(),
            tag: agent.id.clone(),
            node_type: "Agent".into(),
            group: 2,
            size: 16.0,
            details: format!(
                "role: {}\ndomain: {}\nexperience: {} years\nneeds: {}\ntrust_relations: {}",
                agent.role,
                agent.domain,
                agent.years_experience.unwrap_or(0),
                agent.needs.len(),
                agent.trust_weights.len()
            ),
            properties: vec![
                ("role".into(), agent.role.clone()),
                ("domain".into(), agent.domain.clone()),
                (
                    "years_experience".into(),
                    agent.years_experience.unwrap_or(0).to_string(),
                ),
                ("persona".into(), agent.persona_description.clone()),
                ("needs_count".into(), agent.needs.len().to_string()),
            ],
            connected: agent_connected,
        });
    }

    // --- Alignment edges ---
    for alignment in &session.alignments {
        edges.push(GraphEdge {
            source: alignment.section_id.clone(),
            target: alignment.criterion_id.clone(),
            label: format!("aligned({:.2})", alignment.confidence),
            edge_type: "evaluates".into(),
        });
    }

    // --- Gaps ---
    for gap in &session.gaps {
        let gap_id = format!("gap_{}", gap.criterion_id);
        nodes.push(GraphNode {
            id: gap_id.clone(),
            label: format!("GAP: {}", gap.criterion_title),
            tag: gap_id.clone(),
            node_type: "Challenge".into(),
            group: 4,
            size: 10.0,
            details: format!(
                "No document content maps to criterion: {}",
                gap.criterion_title
            ),
            properties: vec![
                ("criterion_id".into(), gap.criterion_id.clone()),
                ("criterion_title".into(), gap.criterion_title.clone()),
            ],
            connected: vec![gap.criterion_id.clone()],
        });
        edges.push(GraphEdge {
            source: gap.criterion_id.clone(),
            target: gap_id,
            label: "gap".into(),
            edge_type: "challenge".into(),
        });
    }

    // --- Debate Rounds: Scores + Challenges ---
    for round in &session.rounds {
        for score in &round.scores {
            let agent_name = agent_names.get(&score.agent_id)
                .cloned()
                .unwrap_or_else(|| score.agent_id.clone());
            let crit_title = criterion_titles.get(&score.criterion_id)
                .cloned()
                .unwrap_or_else(|| score.criterion_id.clone());

            let score_id = format!("score_r{}_{}_{}",
                score.round, score.agent_id, score.criterion_id);

            // Human label: "Prof. Harding → Knowledge: 7/8"
            let label = format!("{} → {}: {:.0}/{:.0}",
                agent_name, crit_title, score.score, score.max_score);

            nodes.push(GraphNode {
                id: score_id.clone(),
                label,
                tag: format!("R{}", score.round),
                node_type: "Score".into(),
                group: 3,
                size: 12.0,
                details: format!(
                    "score: {:.1}/{:.0}\nround: {}\njustification: {}\nevidence_used: {}\ngaps: {}",
                    score.score, score.max_score, score.round,
                    score.justification,
                    score.evidence_used.join(", "),
                    score.gaps_identified.join(", ")
                ),
                properties: vec![
                    ("evaluator".into(), agent_name),
                    ("criterion".into(), crit_title),
                    ("score".into(), format!("{:.1}/{:.0}", score.score, score.max_score)),
                    ("round".into(), score.round.to_string()),
                    ("justification".into(), score.justification.clone()),
                ],
                connected: vec![
                    score.agent_id.clone(),
                    score.criterion_id.clone(),
                ],
            });
            edges.push(GraphEdge {
                source: score.agent_id.clone(),
                target: score_id.clone(),
                label: "feedback".into(),
                edge_type: "scores".into(),
            });
            edges.push(GraphEdge {
                source: score_id,
                target: score.criterion_id.clone(),
                label: "on".into(),
                edge_type: "scores".into(),
            });
        }

        for challenge in &round.challenges {
            let challenger_name = agent_names.get(&challenge.challenger_id)
                .cloned()
                .unwrap_or_else(|| challenge.challenger_id.clone());
            let target_name = agent_names.get(&challenge.target_agent_id)
                .cloned()
                .unwrap_or_else(|| challenge.target_agent_id.clone());

            let ch_id = format!("ch_r{}_{}_{}",
                challenge.round, challenge.challenger_id, challenge.target_agent_id);
            let score_change_str = challenge
                .score_change
                .map(|(from, to)| format!("{:.1} → {:.1}", from, to))
                .unwrap_or_else(|| "maintained".into());

            // Human label: "Dr. Chen challenges Prof. Harding"
            let label = format!("{} challenges {}", challenger_name, target_name);

            nodes.push(GraphNode {
                id: ch_id.clone(),
                label,
                tag: format!("R{}", challenge.round),
                node_type: "Challenge".into(),
                group: 4,
                size: 10.0,
                details: format!(
                    "argument: {}\nresponse: {}\nscore_change: {}",
                    challenge.argument,
                    challenge.response.as_deref().unwrap_or("—"),
                    score_change_str
                ),
                properties: vec![
                    ("challenger".into(), challenger_name),
                    ("target".into(), target_name),
                    ("argument".into(), challenge.argument.clone()),
                    (
                        "response".into(),
                        challenge.response.clone().unwrap_or_else(|| "—".into()),
                    ),
                    ("outcome".into(), score_change_str),
                    ("round".into(), challenge.round.to_string()),
                ],
                connected: vec![
                    challenge.challenger_id.clone(),
                    challenge.target_agent_id.clone(),
                ],
            });
            edges.push(GraphEdge {
                source: challenge.challenger_id.clone(),
                target: ch_id.clone(),
                label: "challenges".into(),
                edge_type: "challenge".into(),
            });
            edges.push(GraphEdge {
                source: ch_id,
                target: challenge.target_agent_id.clone(),
                label: "targets".into(),
                edge_type: "challenge".into(),
            });
        }
    }

    // --- Final Moderated Scores ---
    for ms in &session.final_scores {
        let crit_title = criterion_titles.get(&ms.criterion_id)
            .cloned()
            .unwrap_or_else(|| ms.criterion_id.clone());
        let ms_id = format!("mod_{}", ms.criterion_id);
        let mut ms_connected = vec![ms.criterion_id.clone()];
        let mut ms_props = vec![
            ("criterion".into(), crit_title.clone()),
            ("consensus_score".into(), format!("{:.1}", ms.consensus_score)),
            ("panel_mean".into(), format!("{:.1}", ms.panel_mean)),
            ("panel_std_dev".into(), format!("{:.2}", ms.panel_std_dev)),
            ("dissent_count".into(), ms.dissents.len().to_string()),
        ];

        for dissent in &ms.dissents {
            let dissenter_name = agent_names.get(&dissent.agent_id)
                .cloned()
                .unwrap_or_else(|| dissent.agent_id.clone());
            ms_props.push((
                format!("dissent: {}", dissenter_name),
                format!("{:.1} — {}", dissent.score, dissent.reason),
            ));
            ms_connected.push(dissent.agent_id.clone());
        }

        // Human label: "Consensus: Knowledge → 7.5"
        nodes.push(GraphNode {
            id: ms_id.clone(),
            label: format!("Consensus: {} → {:.1}", crit_title, ms.consensus_score),
            tag: ms_id.clone(),
            node_type: "Moderated".into(),
            group: 5,
            size: 14.0,
            details: format!(
                "consensus: {:.1}\nmean: {:.1}\nstd_dev: {:.2}\ndissents: {}",
                ms.consensus_score,
                ms.panel_mean,
                ms.panel_std_dev,
                ms.dissents.len()
            ),
            properties: ms_props,
            connected: ms_connected,
        });
        edges.push(GraphEdge {
            source: ms.criterion_id.clone(),
            target: ms_id,
            label: "moderatedScore".into(),
            edge_type: "scores".into(),
        });
    }

    GraphData { nodes, edges }
}

/// Recursively add section, claim, and evidence nodes.
fn add_section_nodes(
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
    parent_id: &str,
    section: &Section,
) {
    let mut section_connected: Vec<String> = Vec::new();
    section_connected.push(parent_id.to_string());

    // Claims
    for claim in &section.claims {
        let label = if claim.text.len() > 40 {
            format!("{}...", &claim.text[..40])
        } else {
            claim.text.clone()
        };
        nodes.push(GraphNode {
            id: claim.id.clone(),
            label,
            tag: claim.id.clone(),
            node_type: "Claim".into(),
            group: 0,
            size: 8.0,
            details: format!(
                "specificity: {:.2}\nverifiable: {}",
                claim.specificity, claim.verifiable
            ),
            properties: vec![
                ("text".into(), claim.text.clone()),
                ("specificity".into(), format!("{:.2}", claim.specificity)),
                ("verifiable".into(), claim.verifiable.to_string()),
            ],
            connected: vec![section.id.clone()],
        });
        edges.push(GraphEdge {
            source: section.id.clone(),
            target: claim.id.clone(),
            label: "hasClaim".into(),
            edge_type: "structural".into(),
        });
        section_connected.push(claim.id.clone());
    }

    // Evidence
    for ev in &section.evidence {
        let label = if ev.source.len() > 30 {
            format!("{}...", &ev.source[..30])
        } else {
            ev.source.clone()
        };
        nodes.push(GraphNode {
            id: ev.id.clone(),
            label,
            tag: ev.id.clone(),
            node_type: "Evidence".into(),
            group: 0,
            size: 8.0,
            details: format!(
                "type: {}\nsource: {}\nquantified: {}",
                ev.evidence_type, ev.source, ev.has_quantified_outcome
            ),
            properties: vec![
                ("evidence_type".into(), ev.evidence_type.clone()),
                ("source".into(), ev.source.clone()),
                ("text".into(), ev.text.clone()),
                (
                    "has_quantified_outcome".into(),
                    ev.has_quantified_outcome.to_string(),
                ),
            ],
            connected: vec![section.id.clone()],
        });
        edges.push(GraphEdge {
            source: section.id.clone(),
            target: ev.id.clone(),
            label: "hasEvidence".into(),
            edge_type: "structural".into(),
        });
        section_connected.push(ev.id.clone());
    }

    nodes.push(GraphNode {
        id: section.id.clone(),
        label: section.title.clone(),
        tag: section.id.clone(),
        node_type: "Section".into(),
        group: 0,
        size: 12.0,
        details: format!(
            "words: {}\nclaims: {}\nevidence: {}",
            section.word_count,
            section.claims.len(),
            section.evidence.len()
        ),
        properties: vec![
            ("title".into(), section.title.clone()),
            ("word_count".into(), section.word_count.to_string()),
            ("claims".into(), section.claims.len().to_string()),
            ("evidence".into(), section.evidence.len().to_string()),
        ],
        connected: section_connected,
    });

    edges.push(GraphEdge {
        source: parent_id.to_string(),
        target: section.id.clone(),
        label: "contains".into(),
        edge_type: "structural".into(),
    });

    // Recursion for subsections
    for sub in &section.subsections {
        add_section_nodes(nodes, edges, &section.id, sub);
    }
}

/// Recursively add criterion and rubric level nodes.
fn add_criterion_nodes(
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
    parent_id: &str,
    criterion: &EvaluationCriterion,
) {
    let mut crit_connected: Vec<String> = vec![parent_id.to_string()];

    // Rubric levels
    for (i, rubric) in criterion.rubric_levels.iter().enumerate() {
        let rubric_id = format!("rubric_{}_{}", criterion.id, i);
        nodes.push(GraphNode {
            id: rubric_id.clone(),
            label: format!("{} ({})", rubric.level, rubric.score_range),
            tag: rubric_id.clone(),
            node_type: "RubricLevel".into(),
            group: 1,
            size: 6.0,
            details: format!(
                "level: {}\nrange: {}\ndescriptor: {}",
                rubric.level, rubric.score_range, rubric.descriptor
            ),
            properties: vec![
                ("level".into(), rubric.level.clone()),
                ("score_range".into(), rubric.score_range.clone()),
                ("descriptor".into(), rubric.descriptor.clone()),
            ],
            connected: vec![criterion.id.clone()],
        });
        edges.push(GraphEdge {
            source: criterion.id.clone(),
            target: rubric_id.clone(),
            label: "hasRubricLevel".into(),
            edge_type: "structural".into(),
        });
        crit_connected.push(rubric_id);
    }

    nodes.push(GraphNode {
        id: criterion.id.clone(),
        label: criterion.title.clone(),
        tag: criterion.id.clone(),
        node_type: "Criterion".into(),
        group: 1,
        size: 14.0,
        details: format!(
            "max_score: {:.0}\nweight: {:.2}\nrubric_levels: {}\ndescription: {}",
            criterion.max_score,
            criterion.weight,
            criterion.rubric_levels.len(),
            criterion.description.as_deref().unwrap_or("—")
        ),
        properties: vec![
            ("title".into(), criterion.title.clone()),
            ("max_score".into(), format!("{:.0}", criterion.max_score)),
            ("weight".into(), format!("{:.2}", criterion.weight)),
            (
                "description".into(),
                criterion.description.clone().unwrap_or_else(|| "—".into()),
            ),
            (
                "rubric_levels".into(),
                criterion.rubric_levels.len().to_string(),
            ),
        ],
        connected: crit_connected,
    });

    edges.push(GraphEdge {
        source: parent_id.to_string(),
        target: criterion.id.clone(),
        label: "hasCriterion".into(),
        edge_type: "structural".into(),
    });

    // Sub-criteria recursion
    for sub in &criterion.sub_criteria {
        add_criterion_nodes(nodes, edges, &criterion.id, sub);
    }
}

// ============================================================================
// Lineage trail builder
// ============================================================================

/// Build lineage trail from an evaluation session.
pub fn build_lineage(session: &EvaluationSession) -> Vec<LineageEvent> {
    let mut events = Vec::new();

    // Ingest
    let section_count = session.document.sections.len();
    let claim_count: usize = session.document.sections.iter().map(|s| s.claims.len()).sum();
    let evidence_count: usize = session.document.sections.iter().map(|s| s.evidence.len()).sum();
    events.push(LineageEvent {
        stage: "ingest".into(),
        description: "Document ingested".into(),
        timestamp: session.created_at.clone(),
        details: format!(
            "{} sections, {} claims, {} evidence items",
            section_count, claim_count, evidence_count
        ),
        icon: "\u{1F4C4}".into(), // page emoji
    });

    // Criteria
    events.push(LineageEvent {
        stage: "criteria".into(),
        description: "Evaluation framework loaded".into(),
        timestamp: session.created_at.clone(),
        details: format!(
            "{} criteria in \"{}\"",
            session.framework.criteria.len(),
            session.framework.name
        ),
        icon: "\u{1F4CF}".into(), // ruler emoji
    });

    // Spawn
    events.push(LineageEvent {
        stage: "spawn".into(),
        description: "Evaluator panel spawned".into(),
        timestamp: session.created_at.clone(),
        details: format!("{} agents spawned", session.agents.len()),
        icon: "\u{1F9E0}".into(), // brain emoji
    });

    // Align
    if !session.alignments.is_empty() || !session.gaps.is_empty() {
        events.push(LineageEvent {
            stage: "align".into(),
            description: "Section-criterion alignment".into(),
            timestamp: session.created_at.clone(),
            details: format!(
                "{} alignments, {} gaps",
                session.alignments.len(),
                session.gaps.len()
            ),
            icon: "\u{1F517}".into(), // link emoji
        });
    }

    // Score + Debate rounds
    for round in &session.rounds {
        events.push(LineageEvent {
            stage: "score".into(),
            description: format!("Round {} scoring", round.round_number),
            timestamp: session.created_at.clone(),
            details: format!(
                "{} scores, {} challenges, converged: {}",
                round.scores.len(),
                round.challenges.len(),
                round.converged
            ),
            icon: "\u{1F3AF}".into(), // target emoji
        });

        if !round.challenges.is_empty() {
            events.push(LineageEvent {
                stage: "debate".into(),
                description: format!("Round {} debate", round.round_number),
                timestamp: session.created_at.clone(),
                details: format!(
                    "{} challenges, drift: {}",
                    round.challenges.len(),
                    round.drift_velocity
                        .map(|d| format!("{:.3}", d))
                        .unwrap_or_else(|| "—".into())
                ),
                icon: "\u{2694}\u{FE0F}".into(), // swords emoji
            });
        }
    }

    // Moderate
    if !session.final_scores.is_empty() {
        let total_dissents: usize = session
            .final_scores
            .iter()
            .map(|s| s.dissents.len())
            .sum();
        events.push(LineageEvent {
            stage: "moderate".into(),
            description: "Consensus moderation".into(),
            timestamp: session.created_at.clone(),
            details: format!(
                "{} moderated scores, {} dissents",
                session.final_scores.len(),
                total_dissents
            ),
            icon: "\u{2696}\u{FE0F}".into(), // scales emoji
        });
    }

    // Report
    events.push(LineageEvent {
        stage: "report".into(),
        description: "Report generated".into(),
        timestamp: session.created_at.clone(),
        details: "Evaluation complete".into(),
        icon: "\u{1F4CA}".into(), // chart emoji
    });

    events
}

// ============================================================================
// HTML generation
// ============================================================================

/// Generate a self-contained HTML page with D3.js interactive graph.
///
/// `title` — document title or fallback.
/// `intent` — evaluation intent string.
pub fn generate_graph_html(
    data: &GraphData,
    lineage: &[LineageEvent],
    title: &str,
    intent: &str,
) -> String {
    let graph_json = serde_json::to_string(data).unwrap_or_else(|_| "{}".into());
    let lineage_json = serde_json::to_string(lineage).unwrap_or_else(|_| "[]".into());
    let node_count = data.nodes.len();
    let edge_count = data.edges.len();

    GRAPH_TEMPLATE
        .replace("{{GRAPH_DATA}}", &graph_json)
        .replace("{{LINEAGE_DATA}}", &lineage_json)
        .replace("{{TITLE}}", title)
        .replace("{{INTENT}}", intent)
        .replace("{{NODE_COUNT}}", &node_count.to_string())
        .replace("{{EDGE_COUNT}}", &edge_count.to_string())
}

// ============================================================================
// HTML Template — Catppuccin Mocha dark theme, D3 v7
// ============================================================================

const GRAPH_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Brain in the Fish — {{TITLE}}</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
:root {
    --bg-primary: #1e1e2e;
    --bg-secondary: #252536;
    --bg-panel: #2a2a3d;
    --bg-hover: #313244;
    --text-primary: #cdd6f4;
    --text-secondary: #a6adc8;
    --text-dim: #6c7086;
    --accent: #89b4fa;
    --border: #45475a;
    --border-light: #585b70;
    --surface0: #313244;
    --surface1: #45475a;
    --overlay0: #6c7086;
}

* { margin: 0; padding: 0; box-sizing: border-box; }

body {
    background: var(--bg-primary);
    color: var(--text-primary);
    font-family: 'JetBrains Mono', 'SF Mono', 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
    overflow: hidden;
    height: 100vh;
    width: 100vw;
}

/* ---- Layout ---- */
#app {
    display: grid;
    grid-template-columns: 280px 1fr 360px;
    grid-template-rows: 56px 1fr 48px;
    height: 100vh;
    width: 100vw;
}

/* ---- Header ---- */
#header {
    grid-column: 1 / -1;
    grid-row: 1;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 20px;
    gap: 24px;
    z-index: 20;
}

#header h1 {
    font-size: 16px;
    font-weight: 600;
    color: var(--accent);
    white-space: nowrap;
}

#header .subtitle {
    font-size: 12px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
}

#header .stats {
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
    display: flex;
    gap: 12px;
}

#header .stat-badge {
    background: var(--surface0);
    padding: 2px 8px;
    border-radius: 4px;
    border: 1px solid var(--border);
}

/* ---- Lineage Panel (left) ---- */
#lineage-panel {
    grid-column: 1;
    grid-row: 2;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border);
    overflow-y: auto;
    padding: 16px;
    z-index: 10;
}

#lineage-panel h2 {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1.5px;
    color: var(--text-dim);
    margin-bottom: 16px;
}

.lineage-event {
    display: flex;
    gap: 12px;
    padding: 10px 0;
    position: relative;
}

.lineage-event:not(:last-child)::after {
    content: '';
    position: absolute;
    left: 15px;
    top: 36px;
    bottom: -10px;
    width: 1px;
    background: var(--border);
}

.lineage-icon {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    flex-shrink: 0;
    z-index: 1;
}

.lineage-content {
    flex: 1;
    min-width: 0;
}

.lineage-stage {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 2px;
}

.lineage-details {
    font-size: 10px;
    color: var(--text-dim);
    line-height: 1.4;
}

.stage-ingest .lineage-icon { background: rgba(137,180,250,0.15); }
.stage-criteria .lineage-icon { background: rgba(166,227,161,0.15); }
.stage-spawn .lineage-icon { background: rgba(250,179,135,0.15); }
.stage-align .lineage-icon { background: rgba(148,226,213,0.15); }
.stage-score .lineage-icon { background: rgba(243,139,168,0.15); }
.stage-debate .lineage-icon { background: rgba(249,226,175,0.15); }
.stage-moderate .lineage-icon { background: rgba(203,166,247,0.15); }
.stage-report .lineage-icon { background: rgba(166,173,200,0.15); }

/* ---- Graph Canvas ---- */
#graph-container {
    grid-column: 2;
    grid-row: 2;
    position: relative;
    overflow: hidden;
}

#graph-container svg {
    width: 100%;
    height: 100%;
    display: block;
}

.node-label {
    font-size: 11px;
    fill: var(--text-primary);
    pointer-events: none;
    text-anchor: middle;
    dominant-baseline: central;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    font-weight: 500;
}

.edge-label {
    font-size: 8px;
    fill: var(--text-dim);
    pointer-events: none;
    font-family: 'JetBrains Mono', 'SF Mono', monospace;
    opacity: 0;
    transition: opacity 0.2s;
}

.link-group:hover .edge-label {
    opacity: 1;
}

/* ---- Detail Panel (right) ---- */
#detail-panel {
    grid-column: 3;
    grid-row: 2;
    background: var(--bg-secondary);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    padding: 20px;
    z-index: 10;
}

#detail-panel.empty {
    display: flex;
    align-items: center;
    justify-content: center;
}

#detail-panel .empty-msg {
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.6;
}

#detail-panel .empty-msg .key {
    display: inline-block;
    background: var(--surface0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 10px;
    margin: 0 2px;
}

#detail-header {
    margin-bottom: 16px;
}

#detail-label {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 6px;
    word-break: break-word;
}

#detail-type-badge {
    display: inline-block;
    padding: 2px 10px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-bottom: 8px;
}

#detail-uri {
    font-size: 9px;
    color: var(--text-dim);
    word-break: break-all;
    margin-bottom: 12px;
}

.detail-section-title {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--text-dim);
    margin-top: 16px;
    margin-bottom: 8px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
}

.prop-row {
    display: flex;
    gap: 8px;
    padding: 4px 0;
    font-size: 11px;
    border-bottom: 1px solid rgba(69,71,90,0.3);
}

.prop-key {
    color: var(--text-dim);
    flex-shrink: 0;
    min-width: 100px;
}

.prop-value {
    color: var(--text-primary);
    word-break: break-word;
    flex: 1;
}

.connected-node {
    display: inline-block;
    background: var(--surface0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 10px;
    margin: 2px 4px 2px 0;
    cursor: pointer;
    color: var(--accent);
    transition: background 0.15s;
}

.connected-node:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
}

/* ---- Legend Bar (bottom) ---- */
#legend-bar {
    grid-column: 1 / -1;
    grid-row: 3;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 20px;
    gap: 4px;
    overflow-x: auto;
    z-index: 20;
}

.legend-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 4px;
    font-size: 10px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.15s;
    font-family: inherit;
}

.legend-btn:hover {
    background: var(--surface0);
}

.legend-btn.hidden {
    opacity: 0.35;
    text-decoration: line-through;
}

.legend-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
}

.legend-spacer {
    flex: 1;
}

.legend-shortcut {
    font-size: 10px;
    color: var(--text-dim);
    display: flex;
    gap: 12px;
}

.legend-shortcut span {
    display: flex;
    align-items: center;
    gap: 4px;
}

.legend-shortcut .key {
    display: inline-block;
    background: var(--surface0);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 5px;
    font-size: 9px;
    min-width: 18px;
    text-align: center;
}

/* ---- Scrollbar styling ---- */
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--surface1); border-radius: 3px; }
::-webkit-scrollbar-thumb:hover { background: var(--overlay0); }
</style>
</head>
<body>
<div id="app">

<!-- Header -->
<div id="header">
    <h1>Brain in the Fish</h1>
    <div class="subtitle">{{TITLE}} — {{INTENT}}</div>
    <div class="stats">
        <span class="stat-badge">{{NODE_COUNT}} nodes</span>
        <span class="stat-badge">{{EDGE_COUNT}} edges</span>
    </div>
</div>

<!-- Lineage Panel -->
<div id="lineage-panel">
    <h2>Pipeline Lineage</h2>
    <div id="lineage-events"></div>
</div>

<!-- Graph Canvas -->
<div id="graph-container"></div>

<!-- Detail Panel -->
<div id="detail-panel" class="empty">
    <div class="empty-msg">
        Click a node to inspect<br>
        <span class="key">Esc</span> deselect
        <span class="key">F</span> fit
        <span class="key">R</span> reset
    </div>
</div>

<!-- Legend Bar -->
<div id="legend-bar">
    <div id="legend-buttons"></div>
    <div class="legend-spacer"></div>
    <div class="legend-shortcut">
        <span><span class="key">Esc</span> deselect</span>
        <span><span class="key">F</span> fit</span>
        <span><span class="key">R</span> reset</span>
    </div>
</div>

</div><!-- /app -->

<script>
// ---- Data ----
const graphData = {{GRAPH_DATA}};
const lineageData = {{LINEAGE_DATA}};

// ---- Color & Size Maps ----
const nodeColors = {
    'Document':   '#89b4fa',
    'Section':    '#74c7ec',
    'Claim':      '#94e2d5',
    'Evidence':   '#a6e3a1',
    'Framework':  '#a6e3a1',
    'Criterion':  '#a6e3a1',
    'RubricLevel':'#a6e3a1',
    'Agent':      '#fab387',
    'Need':       '#f9e2af',
    'Trust':      '#f9e2af',
    'Score':      '#f38ba8',
    'Challenge':  '#eba0ac',
    'Moderated':  '#cba6f7',
    'default':    '#a6adc8',
};

const nodeSizes = {
    'Document': 24, 'Framework': 20, 'Agent': 16, 'Criterion': 14,
    'Section': 12, 'Score': 12, 'Moderated': 14, 'Challenge': 10,
    'Claim': 8, 'Evidence': 8, 'RubricLevel': 6, 'Need': 6, 'Trust': 5,
    'default': 8,
};

const edgeStyleMap = {
    'structural': { dash: 'none',   width: 1.2 },
    'trust':      { dash: '6,3',    width: 1.0 },
    'challenge':  { dash: '2,3',    width: 1.5 },
    'scores':     { dash: 'none',   width: 1.5 },
    'evaluates':  { dash: '8,4',    width: 1.2 },
    'relation':   { dash: 'none',   width: 1.0 },
};

function getColor(type) { return nodeColors[type] || nodeColors['default']; }
function getSize(type) { return nodeSizes[type] || nodeSizes['default']; }

// ---- Lineage Panel ----
const lineageContainer = document.getElementById('lineage-events');
lineageData.forEach(ev => {
    const div = document.createElement('div');
    div.className = `lineage-event stage-${ev.stage}`;
    div.innerHTML = `
        <div class="lineage-icon">${ev.icon}</div>
        <div class="lineage-content">
            <div class="lineage-stage">${ev.description}</div>
            <div class="lineage-details">${ev.details}</div>
        </div>`;
    lineageContainer.appendChild(div);
});

// ---- Legend Buttons ----
const nodeTypes = [...new Set(graphData.nodes.map(n => n.node_type))];
const hiddenTypes = new Set();
const legendContainer = document.getElementById('legend-buttons');

nodeTypes.forEach(type => {
    const btn = document.createElement('button');
    btn.className = 'legend-btn';
    btn.dataset.type = type;
    btn.innerHTML = `<span class="legend-dot" style="background:${getColor(type)}"></span>${type}`;
    btn.addEventListener('click', () => toggleType(type, btn));
    legendContainer.appendChild(btn);
});

function toggleType(type, btn) {
    if (hiddenTypes.has(type)) {
        hiddenTypes.delete(type);
        btn.classList.remove('hidden');
    } else {
        hiddenTypes.add(type);
        btn.classList.add('hidden');
    }
    updateVisibility();
}

function updateVisibility() {
    nodeGroup.style('display', d => hiddenTypes.has(d.node_type) ? 'none' : null);
    labelGroup.style('display', d => hiddenTypes.has(d.node_type) ? 'none' : null);
    linkGroups.style('display', d => {
        const sType = typeof d.source === 'object' ? d.source.node_type : null;
        const tType = typeof d.target === 'object' ? d.target.node_type : null;
        if (sType && hiddenTypes.has(sType)) return 'none';
        if (tType && hiddenTypes.has(tType)) return 'none';
        return null;
    });
}

// ---- D3 Graph ----
const container = document.getElementById('graph-container');
const rect = container.getBoundingClientRect();
const width = rect.width;
const height = rect.height;

const svg = d3.select('#graph-container')
    .append('svg')
    .attr('width', width)
    .attr('height', height);

const g = svg.append('g');

// Zoom
const zoom = d3.zoom()
    .scaleExtent([0.05, 6])
    .on('zoom', (event) => g.attr('transform', event.transform));
svg.call(zoom);

// Arrow markers for each edge type
const defs = svg.append('defs');
Object.keys(edgeStyleMap).forEach(etype => {
    defs.append('marker')
        .attr('id', `arrow-${etype}`)
        .attr('viewBox', '0 -5 10 10')
        .attr('refX', 20)
        .attr('refY', 0)
        .attr('markerWidth', 5)
        .attr('markerHeight', 5)
        .attr('orient', 'auto')
        .append('path')
        .attr('d', 'M0,-4L10,0L0,4')
        .attr('fill', '#585b70');
});

// Build index for multi-edge offset
const edgePairCount = {};
graphData.edges.forEach(e => {
    const key = [e.source, e.target].sort().join('|');
    edgePairCount[key] = (edgePairCount[key] || 0) + 1;
    e._pairIndex = edgePairCount[key] - 1;
    e._pairKey = key;
});
// Store total counts
graphData.edges.forEach(e => {
    e._pairTotal = edgePairCount[e._pairKey];
});

// Force simulation
const simulation = d3.forceSimulation(graphData.nodes)
    .force('link', d3.forceLink(graphData.edges).id(d => d.id).distance(80).strength(0.7))
    .force('charge', d3.forceManyBody().strength(-300))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(d => getSize(d.node_type) + 8))
    .force('x', d3.forceX(width / 2).strength(0.03))
    .force('y', d3.forceY(height / 2).strength(0.03));

// Edges (as groups with path + label)
const linkGroups = g.append('g').attr('class', 'links')
    .selectAll('g')
    .data(graphData.edges)
    .enter().append('g')
    .attr('class', 'link-group');

const linkPaths = linkGroups.append('path')
    .attr('fill', 'none')
    .attr('stroke', d => {
        // Color from source node type
        const srcNode = graphData.nodes.find(n => n.id === (typeof d.source === 'object' ? d.source.id : d.source));
        return srcNode ? getColor(srcNode.node_type) : '#585b70';
    })
    .attr('stroke-opacity', 0.3)
    .attr('stroke-width', d => (edgeStyleMap[d.edge_type] || edgeStyleMap['relation']).width)
    .attr('stroke-dasharray', d => (edgeStyleMap[d.edge_type] || edgeStyleMap['relation']).dash)
    .attr('marker-end', d => `url(#arrow-${d.edge_type})`);

const edgeLabels = linkGroups.append('text')
    .attr('class', 'edge-label')
    .text(d => d.label);

// Nodes
const nodeGroup = g.append('g').attr('class', 'nodes')
    .selectAll('circle')
    .data(graphData.nodes)
    .enter().append('circle')
    .attr('r', d => getSize(d.node_type))
    .attr('fill', d => getColor(d.node_type))
    .attr('stroke', 'var(--bg-primary)')
    .attr('stroke-width', 2)
    .style('cursor', 'pointer')
    .style('filter', 'drop-shadow(0 0 4px rgba(0,0,0,0.4))')
    .on('click', (event, d) => { event.stopPropagation(); selectNode(d); })
    .on('mouseover', function(event, d) {
        d3.select(this).attr('stroke', '#cdd6f4').attr('stroke-width', 3);
        // Highlight connected edges
        linkPaths.attr('stroke-opacity', e => {
            const sid = typeof e.source === 'object' ? e.source.id : e.source;
            const tid = typeof e.target === 'object' ? e.target.id : e.target;
            return (sid === d.id || tid === d.id) ? 0.8 : 0.15;
        });
    })
    .on('mouseout', function() {
        d3.select(this).attr('stroke', 'var(--bg-primary)').attr('stroke-width', 2);
        linkPaths.attr('stroke-opacity', 0.3);
    })
    .call(d3.drag()
        .on('start', dragstarted)
        .on('drag', dragged)
        .on('end', dragended));

// Labels
const labelGroup = g.append('g').attr('class', 'labels')
    .selectAll('text')
    .data(graphData.nodes)
    .enter().append('text')
    .attr('class', 'node-label')
    .attr('dy', d => getSize(d.node_type) + 12)
    .text(d => d.label.length > 16 ? d.label.substring(0, 16) + '...' : d.label);

// Tick
simulation.on('tick', () => {
    linkPaths.attr('d', d => {
        const sx = d.source.x, sy = d.source.y;
        const tx = d.target.x, ty = d.target.y;
        if (d._pairTotal <= 1) {
            return `M${sx},${sy}L${tx},${ty}`;
        }
        // Curved for multi-edges
        const dx = tx - sx, dy = ty - sy;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const offset = (d._pairIndex - (d._pairTotal - 1) / 2) * 20;
        const mx = (sx + tx) / 2 + (-dy / dist) * offset;
        const my = (sy + ty) / 2 + (dx / dist) * offset;
        return `M${sx},${sy}Q${mx},${my},${tx},${ty}`;
    });

    edgeLabels
        .attr('x', d => {
            const sx = d.source.x, sy = d.source.y;
            const tx = d.target.x, ty = d.target.y;
            if (d._pairTotal <= 1) return (sx + tx) / 2;
            const dx = tx - sx, dy = ty - sy;
            const dist = Math.sqrt(dx * dx + dy * dy) || 1;
            const offset = (d._pairIndex - (d._pairTotal - 1) / 2) * 20;
            return (sx + tx) / 2 + (-dy / dist) * offset;
        })
        .attr('y', d => {
            const sx = d.source.x, sy = d.source.y;
            const tx = d.target.x, ty = d.target.y;
            if (d._pairTotal <= 1) return (sy + ty) / 2;
            const dx = tx - sx, dy = ty - sy;
            const dist = Math.sqrt(dx * dx + dy * dy) || 1;
            const offset = (d._pairIndex - (d._pairTotal - 1) / 2) * 20;
            return (sy + ty) / 2 + (dx / dist) * offset;
        });

    nodeGroup.attr('cx', d => d.x).attr('cy', d => d.y);
    labelGroup.attr('x', d => d.x).attr('y', d => d.y);
});

// ---- Drag ----
function dragstarted(event) {
    if (!event.active) simulation.alphaTarget(0.3).restart();
    event.subject.fx = event.subject.x;
    event.subject.fy = event.subject.y;
}
function dragged(event) {
    event.subject.fx = event.x;
    event.subject.fy = event.y;
}
function dragended(event) {
    if (!event.active) simulation.alphaTarget(0);
    event.subject.fx = null;
    event.subject.fy = null;
}

// ---- Detail Panel ----
let selectedNode = null;
const detailPanel = document.getElementById('detail-panel');
const nodeMap = {};
graphData.nodes.forEach(n => nodeMap[n.id] = n);

function selectNode(d) {
    selectedNode = d;
    detailPanel.classList.remove('empty');
    detailPanel.innerHTML = buildDetailHTML(d);

    // Highlight selected node
    nodeGroup
        .attr('stroke', n => n.id === d.id ? '#cdd6f4' : 'var(--bg-primary)')
        .attr('stroke-width', n => n.id === d.id ? 3 : 2);
}

function deselectNode() {
    selectedNode = null;
    detailPanel.classList.add('empty');
    detailPanel.innerHTML = `<div class="empty-msg">
        Click a node to inspect<br>
        <span class="key">Esc</span> deselect
        <span class="key">F</span> fit
        <span class="key">R</span> reset
    </div>`;
    nodeGroup.attr('stroke', 'var(--bg-primary)').attr('stroke-width', 2);
}

function buildDetailHTML(d) {
    const color = getColor(d.node_type);
    let html = `<div id="detail-header">
        <div id="detail-label" style="font-size:18px;font-weight:700;margin-bottom:8px">${escapeHtml(d.label)}</div>
        <span id="detail-type-badge" style="background:${color}22;color:${color};border:1px solid ${color}55;font-size:11px;padding:3px 12px;border-radius:6px;font-weight:600;text-transform:uppercase;letter-spacing:1px">${d.node_type}</span>
    </div>`;

    // Reasoning narrative — human-readable description of what this node IS
    html += buildReasoningNarrative(d);

    // Properties as labeled cards (not raw table)
    if (d.properties && d.properties.length > 0) {
        html += `<div class="detail-section-title" style="margin-top:20px">Properties</div>`;
        d.properties.forEach(([k, v]) => {
            const isLong = v.length > 60;
            html += `<div style="margin-bottom:8px;padding:8px 12px;background:var(--bg-primary);border-radius:6px;border-left:3px solid ${color}44">
                <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.5px;color:var(--text-dim);margin-bottom:3px">${escapeHtml(k)}</div>
                <div style="font-size:${isLong ? '12' : '13'}px;color:var(--text-primary);line-height:1.5;word-break:break-word">${escapeHtml(v)}</div>
            </div>`;
        });
    }

    // Tag (technical ID) — collapsible at the bottom
    if (d.tag) {
        html += `<div style="margin-top:16px;padding:6px 10px;background:var(--surface0);border-radius:4px;font-size:10px;color:var(--text-dim);word-break:break-all;font-family:monospace">
            <span style="opacity:0.5">ID:</span> ${escapeHtml(d.tag)}
        </div>`;
    }

    // Connected nodes as clickable pills
    if (d.connected && d.connected.length > 0) {
        html += `<div class="detail-section-title" style="margin-top:20px">Connections</div><div style="display:flex;flex-wrap:wrap;gap:6px">`;
        d.connected.forEach(cid => {
            const cn = nodeMap[cid];
            const lbl = cn ? cn.label : cid;
            const clr = cn ? getColor(cn.node_type) : '#a6adc8';
            const typeBadge = cn ? cn.node_type : '';
            html += `<span class="connected-node" onclick="navigateToNode('${escapeHtml(cid)}')" style="border-color:${clr}55;border-left:3px solid ${clr};padding:4px 10px;font-size:11px">
                ${escapeHtml(lbl)}${typeBadge ? '<br><span style=\"font-size:9px;opacity:0.5\">' + typeBadge + '</span>' : ''}
            </span>`;
        });
        html += `</div>`;
    }

    return html;
}

function buildReasoningNarrative(d) {
    const color = getColor(d.node_type);
    const props = {};
    if (d.properties) d.properties.forEach(([k,v]) => props[k] = v);
    let sections = [];

    switch(d.node_type) {
        case 'Document':
            sections.push({title: 'What', text: `This is the root node of the Document Ontology — the document being evaluated. It represents the complete submission as an eval:Document individual in the OWL knowledge graph.`});
            sections.push({title: 'Structure', text: `Contains ${d.connected.length} top-level sections with a total of ${props.total_word_count || '?'} words. Each section is connected via eval:contains relationships and may have nested subsections, claims, and evidence nodes.`});
            sections.push({title: 'Why it exists', text: `The Document Ontology is one of three ontologies that coexist in the shared Oxigraph triple store. It provides the structured evidence base that agents evaluate against criteria. Every claim and piece of evidence in this document is a queryable RDF node that agents reference in their justifications.`});
            break;
        case 'Section':
            sections.push({title: 'What', text: `A section of the document: "${d.label}". This is an eval:Section individual containing ${props.word_count || '?'} words, with ${props.claims || '0'} extracted claims and ${props.evidence || '0'} evidence references.`});
            sections.push({title: 'Role in evaluation', text: `Sections are the primary units that get mapped to evaluation criteria via onto_align. When an agent scores a criterion, they examine the sections that align to it and assess the quality of claims and evidence within.`});
            sections.push({title: 'Ontology context', text: `Connected to the parent document via eval:contains. Claims and evidence within this section are linked via eval:hasClaim and eval:hasEvidence predicates. Subsections (paragraphs) provide finer-grained structure for detailed assessment.`});
            break;
        case 'Claim':
            sections.push({title: 'What', text: `An assertion made in the document: "${props.text || d.label}"`});
            sections.push({title: 'Assessment', text: `Specificity: ${props.specificity || '?'}/1.0 — ${parseFloat(props.specificity||0) >= 0.7 ? 'this is a specific, concrete claim' : parseFloat(props.specificity||0) >= 0.4 ? 'moderately specific' : 'vague or generic'}. ${props.verifiable === 'true' ? 'This claim is verifiable against external evidence.' : 'This claim cannot be directly verified from the document.'}`});
            sections.push({title: 'Why it matters', text: `Claims are eval:Claim individuals in the Document Ontology. Agents assess whether claims are supported by evidence (eval:Evidence nodes in the same section), whether they are specific enough to demonstrate knowledge, and whether they are relevant to the evaluation criteria. High-specificity verifiable claims score better.`});
            break;
        case 'Evidence':
            sections.push({title: 'What', text: `Evidence cited in the document from: "${props.source || '?'}". Type: ${props.evidence_type || '?'}.`});
            sections.push({title: 'Quality', text: `${props.has_quantified_outcome === 'true' ? 'This evidence includes quantified outcomes (numbers, percentages, monetary values), which strengthens scoring.' : 'No quantified outcomes — this is a qualitative reference.'} Evidence type "${props.evidence_type || '?'}" ${props.evidence_type === 'statistical' ? 'carries strong weight for empirical claims' : props.evidence_type === 'citation' ? 'supports theoretical arguments' : 'provides supporting context'}.`});
            sections.push({title: 'Ontology role', text: `Evidence nodes (eval:Evidence) are linked to their parent section via eval:hasEvidence. When agents score criteria, they reference specific evidence nodes in their justifications. Evidence with quantified outcomes (eval:hasQuantifiedOutcome = true) is weighted more heavily by most agent personas.`});
            break;
        case 'Framework':
            sections.push({title: 'What', text: `The evaluation framework "${d.label}" — an eval:EvaluationFramework individual that defines the complete set of criteria, weights, and rubric levels used to assess the document.`});
            sections.push({title: 'Configuration', text: `Contains ${props.criteria_count || '?'} evaluation criteria, assessed by ${props.evaluators || '?'} evaluator agents. ${props.pass_mark ? 'Pass mark: ' + props.pass_mark + '%. ' : ''}Total weight: ${props.total_weight || '1.0'}.`});
            sections.push({title: 'Ontology role', text: `The Criteria Ontology is the second of three ontologies in the triple store. It connects to the Document Ontology via onto_align mappings (which sections address which criteria) and to the Agent Ontology via scoring relationships. The framework defines WHAT is evaluated; agents determine HOW WELL.`});
            break;
        case 'Criterion':
            sections.push({title: 'What', text: `Evaluation criterion: "${d.label}". This is an eval:EvaluationCriterion individual that agents score the document against.`});
            sections.push({title: 'Scoring', text: `Maximum score: ${props.max_score || '?'}. Weight: ${props.weight ? (parseFloat(props.weight)*100).toFixed(0) + '% of total' : '?'}. ${props.description || 'No additional description.'}`});
            sections.push({title: 'How agents use it', text: `Each evaluator agent independently scores this criterion in Round 1. Their scores become eval:Score nodes connecting the agent to this criterion. If scores diverge by more than the disagreement threshold, agents challenge each other's justifications in debate rounds. The final consensus score (eval:ModeratedScore) represents the trust-weighted panel agreement.`});
            if (props.rubric_levels && props.rubric_levels !== '0') {
                sections.push({title: 'Rubric', text: `Has ${props.rubric_levels} defined rubric levels. Each level specifies a score range and descriptor that agents reference when justifying their scores.`});
            }
            break;
        case 'RubricLevel':
            sections.push({title: 'What', text: `Rubric level "${props.level || d.label}" — an eval:RubricLevel individual that defines what a specific score range means for its parent criterion.`});
            sections.push({title: 'Descriptor', text: `Score range: ${props.score_range || '?'}. ${props.descriptor || 'No descriptor.'}`});
            sections.push({title: 'Purpose', text: `Rubric levels provide the objective grounding for agent scores. When an agent gives a score of 7/8 and the rubric says Level 4 (7-8) means "accurate and thorough knowledge with precise use of terminology", the agent must demonstrate that the document meets this standard. This prevents arbitrary scoring and ensures consistency across the panel.`});
            break;
        case 'Agent':
            sections.push({title: 'Who', text: `${d.label} — a ${props.role || 'evaluator'} with expertise in ${props.domain || 'this domain'}${props.years_experience ? ' and ' + props.years_experience + ' years of experience' : ''}.`});
            sections.push({title: 'Persona', text: props.persona_description || 'No detailed persona description.'});
            sections.push({title: 'Cognitive model', text: `This agent's mental state is modelled as OWL individuals in the Agent Ontology — the third of three ontologies in the triple store. Their Maslow needs (cog:Need subclasses) determine evaluation priorities. Their trust weights (cog:TrustRelation) influence how they respond to challenges from other agents. During debate, trust weights update based on the quality of challenges: successful challenges increase trust, rejected challenges decrease it.`});
            sections.push({title: 'Evaluation role', text: `This agent independently scores each criterion, producing eval:Score nodes that connect them to the Criteria Ontology. Their scores include justifications grounded in specific document content (eval:Evidence and eval:Claim references). Disagreements with other agents trigger structured debate.`});
            break;
        case 'Need':
            sections.push({title: 'What', text: `A cognitive need from Maslow's hierarchy: "${props.expression || d.label}". This is a ${props.need_type || 'cognitive'} need (cog:${props.need_type || ''}Need) with salience ${props.salience || '?'}/1.0.`});
            sections.push({title: 'Status', text: `${props.satisfied === 'true' ? 'This need is currently SATISFIED — the agent has found sufficient evidence in the document to address this concern.' : 'This need is NOT YET SATISFIED — the agent is still looking for evidence that addresses this concern, which may lead to lower scores on related criteria.'}`});
            sections.push({title: 'Why Maslow in evaluation', text: `The AgentSociety cognitive model maps Maslow's hierarchy to evaluation priorities. Safety needs drive compliance checking ("does this meet minimum requirements?"). Esteem needs drive excellence assessment ("does this demonstrate outstanding quality?"). Self-actualisation needs drive innovation assessment ("does this go beyond what was asked?"). Each agent's need profile shapes their scoring priorities, creating diverse perspectives that emerge through debate.`});
            break;
        case 'Trust':
            sections.push({title: 'What', text: `A trust relationship (cog:TrustRelation) between two evaluator agents in the domain of "${props.domain || '?'}". Current trust level: ${props.trust_level || '?'}/1.0.`});
            sections.push({title: 'How trust works', text: `Trust is directional — Agent A may trust Agent B differently than B trusts A. Trust influences two things: (1) how an agent responds to challenges from the trusted agent, and (2) the weight given to that agent's scores in the final consensus calculation. Higher trust means more influence on the moderated result.`});
            sections.push({title: 'Trust evolution', text: `Trust weights update during debate. When Agent A challenges Agent B and B adjusts their score (acknowledging A's argument), B's trust in A increases by 0.1 (capped at 1.0). When B maintains their score despite the challenge, B's trust in A decreases by 0.05 (floored at 0.0). This creates a self-calibrating system where agents who make better arguments gain more influence over time.`});
            break;
        case 'Score':
            sections.push({title: 'Feedback', text: props.justification || 'No justification provided.'});
            sections.push({title: 'Assessment', text: `${props.evaluator || '?'} scored "${props.criterion || '?'}" at ${props.score || '?'} in Round ${props.round || '?'}. This score is an eval:Score individual connecting the Agent Ontology to the Criteria Ontology — it is the primary bridge that makes the knowledge graph ONE connected structure.`});
            sections.push({title: 'Ontology significance', text: `Score nodes are the most important connectors in the graph. Each score links an agent (who scored), a criterion (what was scored), and references to document evidence (why). The justification text explains the reasoning. When scores from different agents on the same criterion disagree, the debate orchestrator generates structured challenges. The full scoring trail is versioned via onto_version, making every round diffable and replayable.`});
            break;
        case 'Moderated':
            sections.push({title: 'Consensus', text: `Final moderated score for "${props.criterion || '?'}": ${props.consensus_score || '?'}. This is a trust-weighted consensus, not a simple average — agents with higher trust contribute more to the final number.`});
            sections.push({title: 'Panel statistics', text: `Panel mean: ${props.panel_mean || '?'}. Standard deviation: ${props.panel_std_dev || '?'}. ${props.dissent_count === '0' || !props.dissent_count ? 'No dissenting opinions — the panel converged.' : props.dissent_count + ' dissenting opinion(s) recorded. Dissents are preserved as formal disagreements — they do not affect the consensus score but are flagged in the report as areas where expert opinion diverged.'}`});
            sections.push({title: 'How moderation works', text: `After debate rounds converge (drift velocity drops below threshold), the moderator agent calculates trust-weighted consensus scores. Outlier scores (>2 standard deviations from the mean) are flagged for review. The moderator can interview outlier agents to determine whether their dissent should be recorded or their score adjusted. The final eval:ModeratedScore connects to the criterion and records both the consensus and any dissent.`});
            break;
        case 'Challenge':
            sections.push({title: 'What happened', text: `${props.challenger || '?'} challenged ${props.target || '?'} in Round ${props.round || '?'}.`});
            sections.push({title: 'Argument', text: props.argument || 'No argument recorded.'});
            sections.push({title: 'Response', text: `${props.response || 'No response recorded.'} Outcome: ${props.outcome || 'unknown'}.`});
            sections.push({title: 'How debate works', text: `When agents disagree on a criterion score by more than the threshold, the debate orchestrator generates structured challenges. The challenger must ground their argument in specific document evidence (eval:Evidence and eval:Claim nodes from the Document Ontology). The target must either defend their score with counter-evidence or adjust it. Score changes update the graph state, and trust weights between the agents adjust accordingly. This creates a self-improving system where the best-argued positions prevail.`});
            break;
        default:
            sections.push({title: 'Details', text: d.details || 'No additional information available.'});
    }

    if (sections.length === 0) return '';

    let html = '<div style="margin:16px 0">';
    sections.forEach(s => {
        html += `<div style="margin-bottom:12px;padding:10px 14px;background:var(--bg-primary);border-radius:8px;border:1px solid var(--border)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:1px;color:${color};font-weight:600;margin-bottom:6px">${s.title}</div>
            <div style="font-size:13px;line-height:1.7;color:var(--text-secondary)">${escapeHtml(s.text)}</div>
        </div>`;
    });
    html += '</div>';
    return html;
}

function navigateToNode(id) {
    const n = nodeMap[id];
    if (n) selectNode(n);
}

function escapeHtml(str) {
    if (!str) return '';
    return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// Click background to deselect
svg.on('click', () => deselectNode());

// ---- Keyboard Shortcuts ----
document.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
        deselectNode();
    } else if (e.key === 'f' || e.key === 'F') {
        fitGraph();
    } else if (e.key === 'r' || e.key === 'R') {
        svg.transition().duration(500).call(zoom.transform, d3.zoomIdentity);
    }
});

function fitGraph() {
    const nodes = graphData.nodes;
    if (nodes.length === 0) return;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    nodes.forEach(n => {
        if (n.x < minX) minX = n.x;
        if (n.y < minY) minY = n.y;
        if (n.x > maxX) maxX = n.x;
        if (n.y > maxY) maxY = n.y;
    });
    const pad = 60;
    const bw = (maxX - minX) + pad * 2;
    const bh = (maxY - minY) + pad * 2;
    const scale = Math.min(width / bw, height / bh, 2);
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    const transform = d3.zoomIdentity
        .translate(width / 2, height / 2)
        .scale(scale)
        .translate(-cx, -cy);
    svg.transition().duration(500).call(zoom.transform, transform);
}

// Initial fit after simulation settles
setTimeout(fitGraph, 2000);
</script>
</body>
</html>"##;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> EvaluationSession {
        EvaluationSession {
            id: "test".into(),
            document: EvalDocument {
                id: "doc1".into(),
                title: "Test Doc".into(),
                doc_type: "essay".into(),
                total_pages: Some(5),
                total_word_count: Some(1000),
                sections: vec![Section {
                    id: "sec1".into(),
                    title: "Intro".into(),
                    text: "Content".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![Claim {
                        id: "c1".into(),
                        text: "A claim".into(),
                        specificity: 0.8,
                        verifiable: true,
                    }],
                    evidence: vec![Evidence {
                        id: "e1".into(),
                        source: "Source".into(),
                        evidence_type: "citation".into(),
                        text: "ev text".into(),
                        has_quantified_outcome: true,
                    }],
                    subsections: vec![],
                }],
            },
            framework: EvaluationFramework {
                id: "fw1".into(),
                name: "Test Framework".into(),
                total_weight: 1.0,
                pass_mark: Some(50.0),
                criteria: vec![EvaluationCriterion {
                    id: "crit1".into(),
                    title: "Quality".into(),
                    description: Some("Test".into()),
                    max_score: 10.0,
                    weight: 1.0,
                    rubric_levels: vec![],
                    sub_criteria: vec![],
                }],
            },
            agents: vec![EvaluatorAgent {
                id: "a1".into(),
                name: "Agent One".into(),
                role: "Expert".into(),
                domain: "Test".into(),
                years_experience: Some(10),
                persona_description: "Test agent".into(),
                needs: vec![],
                trust_weights: vec![],
            }],
            alignments: vec![],
            gaps: vec![],
            rounds: vec![],
            final_scores: vec![ModeratedScore {
                criterion_id: "crit1".into(),
                consensus_score: 7.5,
                panel_mean: 7.0,
                panel_std_dev: 0.5,
                dissents: vec![],
            }],
            created_at: "2026-03-23".into(),
        }
    }

    #[test]
    fn test_extract_graph_data() {
        let session = test_session();
        let data = extract_graph_data(&session);
        assert!(!data.nodes.is_empty());
        assert!(!data.edges.is_empty());
        // doc + section + claim + evidence + framework + criterion + agent + moderated = 8
        assert!(
            data.nodes.len() >= 8,
            "Expected at least 8 nodes, got {}",
            data.nodes.len()
        );
    }

    #[test]
    fn test_graph_node_types() {
        let session = test_session();
        let data = extract_graph_data(&session);
        let types: Vec<&str> = data.nodes.iter().map(|n| n.node_type.as_str()).collect();
        assert!(types.contains(&"Document"));
        assert!(types.contains(&"Section"));
        assert!(types.contains(&"Criterion"));
        assert!(types.contains(&"Agent"));
        assert!(types.contains(&"Moderated"));
    }

    #[test]
    fn test_generate_graph_html() {
        let session = test_session();
        let data = extract_graph_data(&session);
        let lineage = build_lineage(&session);
        let html = generate_graph_html(&data, &lineage, "Test Doc", "test intent");
        assert!(html.contains("d3.v7.min.js"));
        assert!(html.contains("Brain in the Fish"));
        assert!(html.contains("forceSimulation"));
        assert!(html.contains("Test Doc"));
        assert!(html.contains("Agent One"));
    }

    #[test]
    fn test_graph_edges_reference_valid_nodes() {
        let session = test_session();
        let data = extract_graph_data(&session);
        let node_ids: Vec<&str> = data.nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &data.edges {
            assert!(
                node_ids.contains(&edge.source.as_str()),
                "Edge source {} not found in nodes",
                edge.source
            );
            assert!(
                node_ids.contains(&edge.target.as_str()),
                "Edge target {} not found in nodes",
                edge.target
            );
        }
    }

    #[test]
    fn test_lineage_events() {
        let session = test_session();
        let events = build_lineage(&session);
        assert!(!events.is_empty());
        let stages: Vec<&str> = events.iter().map(|e| e.stage.as_str()).collect();
        assert!(stages.contains(&"ingest"));
        assert!(stages.contains(&"criteria"));
        assert!(stages.contains(&"spawn"));
        assert!(stages.contains(&"report"));
    }

    #[test]
    fn test_classify_rdf_type() {
        assert_eq!(classify_rdf_type("http://example.org/eval/Document"), "Document");
        assert_eq!(classify_rdf_type("http://example.org/eval/Section"), "Section");
        assert_eq!(classify_rdf_type("http://example.org/eval/EvaluatorAgent"), "Agent");
        assert_eq!(classify_rdf_type("http://example.org/eval/Score"), "Score");
        assert_eq!(classify_rdf_type("http://example.org/eval/ModeratedScore"), "Moderated");
        assert_eq!(classify_rdf_type("http://example.org/unknown"), "default");
    }

    #[test]
    fn test_label_from_iri() {
        assert_eq!(label_from_iri("http://example.org/eval#Document"), "Document");
        assert_eq!(label_from_iri("http://example.org/eval/Section"), "Section");
        assert_eq!(label_from_iri("<http://example.org/eval#Foo>"), "Foo");
    }

    #[test]
    fn test_strip_literal() {
        assert_eq!(strip_literal(r#""hello"^^<http://www.w3.org/2001/XMLSchema#string>"#), "hello");
        assert_eq!(strip_literal(r#""42"^^<http://www.w3.org/2001/XMLSchema#integer>"#), "42");
        assert_eq!(strip_literal("plain"), "plain");
    }

    #[test]
    fn test_node_properties_populated() {
        let session = test_session();
        let data = extract_graph_data(&session);
        let doc_node = data.nodes.iter().find(|n| n.node_type == "Document").unwrap();
        assert!(!doc_node.properties.is_empty(), "Document node should have properties");
        let agent_node = data.nodes.iter().find(|n| n.node_type == "Agent").unwrap();
        assert!(!agent_node.properties.is_empty(), "Agent node should have properties");
    }

    #[test]
    fn test_connected_nodes_populated() {
        let session = test_session();
        let data = extract_graph_data(&session);
        let doc_node = data.nodes.iter().find(|n| n.node_type == "Document").unwrap();
        assert!(!doc_node.connected.is_empty(), "Document should have connected nodes (sections)");
    }
}
