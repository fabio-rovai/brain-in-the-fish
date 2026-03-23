# Changelog

## 0.1.0 (2026-03-23)

### Added
- PDF and plain text document ingestion with section splitting
- Document ontology generation (sections, claims, evidence as OWL/Turtle)
- Evaluation criteria ontology with built-in frameworks (generic, academic, tender)
- Agent cognitive model (Maslow hierarchy + Theory of Planned Behaviour as OWL)
- Domain-specific agent panel spawning (6 domains: academic, tender, policy, survey, legal, generic)
- Independent scoring engine with ReACT loop prompts and SPARQL queries
- Multi-round structured debate with disagreement detection and convergence
- Trust-weighted consensus moderation with outlier/dissent handling
- Evaluation report generation (Markdown scorecard + Turtle export)
- MCP server with 10 eval_* tools via rmcp
- CLI interface: `brain-in-the-fish evaluate` and `brain-in-the-fish serve`
- 95 unit and integration tests
