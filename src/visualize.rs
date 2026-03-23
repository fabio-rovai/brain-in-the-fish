//! Graph visualization data extraction.
//!
//! Extracts graph data from an [`EvaluationSession`] and generates a
//! self-contained HTML file with a D3.js force-directed interactive graph.

use crate::types::*;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub group: u32,
    pub size: f64,
    pub details: String,
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

/// Extract graph visualization data from an [`EvaluationSession`].
pub fn extract_graph_data(session: &EvaluationSession) -> GraphData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Document node (large, central)
    nodes.push(GraphNode {
        id: session.document.id.clone(),
        label: if session.document.title.is_empty() {
            "Document".into()
        } else {
            session.document.title.clone()
        },
        node_type: "document".into(),
        group: 0,
        size: 20.0,
        details: format!(
            "Type: {}\nWords: {}",
            session.document.doc_type,
            session.document.total_word_count.unwrap_or(0)
        ),
    });

    // Sections
    for section in &session.document.sections {
        nodes.push(GraphNode {
            id: section.id.clone(),
            label: section.title.clone(),
            node_type: "section".into(),
            group: 0,
            size: 12.0,
            details: format!("{} words", section.word_count),
        });
        edges.push(GraphEdge {
            source: session.document.id.clone(),
            target: section.id.clone(),
            label: "contains".into(),
            edge_type: "contains".into(),
        });

        // Claims
        for claim in &section.claims {
            let label = if claim.text.len() > 30 {
                format!("{}...", &claim.text[..30])
            } else {
                claim.text.clone()
            };
            nodes.push(GraphNode {
                id: claim.id.clone(),
                label,
                node_type: "claim".into(),
                group: 0,
                size: 8.0,
                details: format!(
                    "Specificity: {:.1}\nVerifiable: {}",
                    claim.specificity, claim.verifiable
                ),
            });
            edges.push(GraphEdge {
                source: section.id.clone(),
                target: claim.id.clone(),
                label: "claims".into(),
                edge_type: "contains".into(),
            });
        }

        // Evidence
        for ev in &section.evidence {
            let label = if ev.source.len() > 25 {
                format!("{}...", &ev.source[..25])
            } else {
                ev.source.clone()
            };
            nodes.push(GraphNode {
                id: ev.id.clone(),
                label,
                node_type: "evidence".into(),
                group: 0,
                size: 8.0,
                details: format!(
                    "Type: {}\nQuantified: {}",
                    ev.evidence_type, ev.has_quantified_outcome
                ),
            });
            edges.push(GraphEdge {
                source: section.id.clone(),
                target: ev.id.clone(),
                label: "cites".into(),
                edge_type: "contains".into(),
            });
        }
    }

    // Framework node
    let fw_id = format!("fw_{}", session.framework.id);
    nodes.push(GraphNode {
        id: fw_id.clone(),
        label: session.framework.name.clone(),
        node_type: "framework".into(),
        group: 1,
        size: 18.0,
        details: format!(
            "Criteria: {}\nPass mark: {:?}",
            session.framework.criteria.len(),
            session.framework.pass_mark
        ),
    });

    // Criteria
    for criterion in &session.framework.criteria {
        nodes.push(GraphNode {
            id: criterion.id.clone(),
            label: criterion.title.clone(),
            node_type: "criterion".into(),
            group: 1,
            size: 14.0,
            details: format!("Max: {}\nWeight: {}", criterion.max_score, criterion.weight),
        });
        edges.push(GraphEdge {
            source: fw_id.clone(),
            target: criterion.id.clone(),
            label: "criterion".into(),
            edge_type: "contains".into(),
        });
    }

    // Agents
    for agent in &session.agents {
        nodes.push(GraphNode {
            id: agent.id.clone(),
            label: agent.name.clone(),
            node_type: "agent".into(),
            group: 2,
            size: 16.0,
            details: format!("Role: {}\nDomain: {}", agent.role, agent.domain),
        });

        // Trust edges between agents
        for trust in &agent.trust_weights {
            edges.push(GraphEdge {
                source: agent.id.clone(),
                target: trust.target_agent_id.clone(),
                label: format!("trust: {:.1}", trust.trust_level),
                edge_type: "trusts".into(),
            });
        }
    }

    // Final scores — connect agents to criteria
    for score in &session.final_scores {
        let score_id = format!("score_{}", score.criterion_id);
        nodes.push(GraphNode {
            id: score_id.clone(),
            label: format!("{:.1}", score.consensus_score),
            node_type: "score".into(),
            group: 3,
            size: 10.0,
            details: format!(
                "Consensus: {:.1}\nMean: {:.1}\nStd: {:.1}\nDissents: {}",
                score.consensus_score,
                score.panel_mean,
                score.panel_std_dev,
                score.dissents.len()
            ),
        });
        edges.push(GraphEdge {
            source: score.criterion_id.clone(),
            target: score_id.clone(),
            label: "scored".into(),
            edge_type: "scores".into(),
        });
    }

    // Alignment edges — connect sections to criteria
    for alignment in &session.alignments {
        edges.push(GraphEdge {
            source: alignment.section_id.clone(),
            target: alignment.criterion_id.clone(),
            label: format!("align: {:.2}", alignment.confidence),
            edge_type: "evaluates".into(),
        });
    }

    // Debate challenge edges from rounds
    for round in &session.rounds {
        for challenge in &round.challenges {
            edges.push(GraphEdge {
                source: challenge.challenger_id.clone(),
                target: challenge.target_agent_id.clone(),
                label: format!("challenges (R{})", round.round_number),
                edge_type: "challenges".into(),
            });
        }
    }

    GraphData { nodes, edges }
}

/// Generate a self-contained HTML file with D3.js force-directed graph.
pub fn generate_graph_html(data: &GraphData) -> String {
    let json = serde_json::to_string(data).unwrap_or_default();
    GRAPH_TEMPLATE.replace("{{GRAPH_DATA}}", &json)
}

const GRAPH_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Brain in the Fish — Evaluation Graph</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    background: #0a0a0f;
    color: #e0e0e0;
    font-family: 'JetBrains Mono', 'SF Mono', 'Fira Code', monospace;
    overflow: hidden;
}
#graph { width: 100vw; height: 100vh; }
svg { width: 100%; height: 100%; }

.node-label {
    font-size: 11px;
    fill: #ccc;
    pointer-events: none;
    text-anchor: middle;
    dominant-baseline: central;
}

.edge-label {
    font-size: 9px;
    fill: #666;
    pointer-events: none;
}

.link {
    stroke-opacity: 0.4;
}

#detail-panel {
    position: fixed;
    top: 20px;
    right: 20px;
    width: 320px;
    background: rgba(15, 15, 25, 0.95);
    border: 1px solid #333;
    border-radius: 12px;
    padding: 20px;
    display: none;
    backdrop-filter: blur(10px);
    box-shadow: 0 8px 32px rgba(0,0,0,0.5);
}

#detail-panel h3 {
    color: #fff;
    font-size: 16px;
    margin-bottom: 8px;
    border-bottom: 1px solid #333;
    padding-bottom: 8px;
}

#detail-panel .type-badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    margin-bottom: 12px;
    text-transform: uppercase;
    letter-spacing: 1px;
}

#detail-panel .details {
    font-size: 13px;
    line-height: 1.6;
    white-space: pre-wrap;
    color: #aaa;
}

#header {
    position: fixed;
    top: 20px;
    left: 20px;
    z-index: 10;
}

#header h1 {
    font-size: 20px;
    color: #fff;
    margin-bottom: 4px;
}

#header .subtitle {
    font-size: 12px;
    color: #666;
}

#legend {
    position: fixed;
    bottom: 20px;
    left: 20px;
    display: flex;
    gap: 16px;
    font-size: 12px;
}

.legend-item {
    display: flex;
    align-items: center;
    gap: 6px;
}

.legend-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
}
</style>
</head>
<body>
<div id="header">
    <h1>Brain in the Fish</h1>
    <div class="subtitle">Evaluation Knowledge Graph</div>
</div>

<div id="legend">
    <div class="legend-item"><div class="legend-dot" style="background: #4fc3f7;"></div> Document</div>
    <div class="legend-item"><div class="legend-dot" style="background: #66bb6a;"></div> Criteria</div>
    <div class="legend-item"><div class="legend-dot" style="background: #ffa726;"></div> Agents</div>
    <div class="legend-item"><div class="legend-dot" style="background: #ef5350;"></div> Scores</div>
</div>

<div id="detail-panel">
    <h3 id="detail-title"></h3>
    <div class="type-badge" id="detail-type"></div>
    <div class="details" id="detail-text"></div>
</div>

<div id="graph"></div>

<script>
const data = {{GRAPH_DATA}};

const colors = {
    0: '#4fc3f7',
    1: '#66bb6a',
    2: '#ffa726',
    3: '#ef5350',
};

const edgeColors = {
    'contains': '#333',
    'evaluates': '#4fc3f7',
    'scores': '#ef5350',
    'challenges': '#ff7043',
    'trusts': '#ffa726',
};

const badgeColors = {
    'document': '#1565c0', 'section': '#1976d2', 'claim': '#1e88e5', 'evidence': '#2196f3',
    'framework': '#2e7d32', 'criterion': '#388e3c', 'rubric': '#43a047',
    'agent': '#e65100', 'score': '#c62828',
};

const width = window.innerWidth;
const height = window.innerHeight;

const svg = d3.select('#graph')
    .append('svg')
    .attr('width', width)
    .attr('height', height);

const g = svg.append('g');
svg.call(d3.zoom()
    .scaleExtent([0.1, 4])
    .on('zoom', (event) => g.attr('transform', event.transform)));

svg.append('defs').selectAll('marker')
    .data(['contains', 'evaluates', 'scores', 'challenges', 'trusts'])
    .enter().append('marker')
    .attr('id', d => `arrow-${d}`)
    .attr('viewBox', '0 -5 10 10')
    .attr('refX', 20)
    .attr('refY', 0)
    .attr('markerWidth', 6)
    .attr('markerHeight', 6)
    .attr('orient', 'auto')
    .append('path')
    .attr('d', 'M0,-5L10,0L0,5')
    .attr('fill', d => edgeColors[d] || '#444');

const simulation = d3.forceSimulation(data.nodes)
    .force('link', d3.forceLink(data.edges).id(d => d.id).distance(120))
    .force('charge', d3.forceManyBody().strength(-400))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(d => d.size + 10));

const link = g.append('g')
    .selectAll('line')
    .data(data.edges)
    .enter().append('line')
    .attr('class', 'link')
    .attr('stroke', d => edgeColors[d.edge_type] || '#444')
    .attr('stroke-width', d => d.edge_type === 'trusts' ? 1 : 1.5)
    .attr('stroke-dasharray', d => d.edge_type === 'trusts' ? '4,4' : 'none')
    .attr('marker-end', d => `url(#arrow-${d.edge_type})`);

const edgeLabel = g.append('g')
    .selectAll('text')
    .data(data.edges)
    .enter().append('text')
    .attr('class', 'edge-label')
    .text(d => d.label);

const node = g.append('g')
    .selectAll('circle')
    .data(data.nodes)
    .enter().append('circle')
    .attr('r', d => d.size)
    .attr('fill', d => colors[d.group])
    .attr('stroke', '#1a1a2e')
    .attr('stroke-width', 2)
    .style('cursor', 'pointer')
    .on('click', (event, d) => { event.stopPropagation(); showDetail(d); })
    .on('mouseover', function() {
        d3.select(this).attr('stroke', '#fff').attr('stroke-width', 3);
    })
    .on('mouseout', function() {
        d3.select(this).attr('stroke', '#1a1a2e').attr('stroke-width', 2);
    })
    .call(d3.drag()
        .on('start', dragstarted)
        .on('drag', dragged)
        .on('end', dragended));

node.style('filter', 'drop-shadow(0 0 6px rgba(255,255,255,0.15))');

const label = g.append('g')
    .selectAll('text')
    .data(data.nodes)
    .enter().append('text')
    .attr('class', 'node-label')
    .attr('dy', d => d.size + 14)
    .text(d => d.label.length > 20 ? d.label.substring(0, 20) + '...' : d.label);

simulation.on('tick', () => {
    link
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x)
        .attr('y2', d => d.target.y);

    edgeLabel
        .attr('x', d => (d.source.x + d.target.x) / 2)
        .attr('y', d => (d.source.y + d.target.y) / 2);

    node.attr('cx', d => d.x).attr('cy', d => d.y);
    label.attr('x', d => d.x).attr('y', d => d.y);
});

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

function showDetail(d) {
    const panel = document.getElementById('detail-panel');
    panel.style.display = 'block';
    document.getElementById('detail-title').textContent = d.label;
    const badge = document.getElementById('detail-type');
    badge.textContent = d.node_type;
    badge.style.background = badgeColors[d.node_type] || '#555';
    badge.style.color = '#fff';
    document.getElementById('detail-text').textContent = d.details;
}

svg.on('click', () => {
    document.getElementById('detail-panel').style.display = 'none';
});
</script>
</body>
</html>"##;

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
        // doc + section + claim + evidence + framework + criterion + agent + score = 8
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
        assert!(types.contains(&"document"));
        assert!(types.contains(&"section"));
        assert!(types.contains(&"criterion"));
        assert!(types.contains(&"agent"));
        assert!(types.contains(&"score"));
    }

    #[test]
    fn test_generate_graph_html() {
        let session = test_session();
        let data = extract_graph_data(&session);
        let html = generate_graph_html(&data);
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
}
