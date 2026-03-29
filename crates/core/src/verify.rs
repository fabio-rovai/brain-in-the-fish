//! Web verification — checks factual claims against the internet.
//!
//! Each claim node from the ontology gets a search query, a web result,
//! and a verification status: verified, unverifiable, or contradicted.

use serde::{Deserialize, Serialize};

/// Verification result for a single claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimVerification {
    /// The claim text (from the ontology node)
    pub claim: String,
    /// The search query generated for this claim
    pub search_query: String,
    /// What was found (or not)
    pub search_result: String,
    /// Verification status
    pub status: VerificationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VerificationStatus {
    /// Found corroborating evidence online
    Verified,
    /// No evidence found — claim cannot be confirmed
    Unverifiable,
    /// Found evidence that contradicts the claim
    Contradicted,
    /// Not yet checked
    Pending,
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationStatus::Verified => write!(f, "VERIFIED"),
            VerificationStatus::Unverifiable => write!(f, "UNVERIFIABLE"),
            VerificationStatus::Contradicted => write!(f, "CONTRADICTED"),
            VerificationStatus::Pending => write!(f, "PENDING"),
        }
    }
}

/// Full verification report for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub total_claims: usize,
    pub verified: usize,
    pub unverifiable: usize,
    pub contradicted: usize,
    pub claims: Vec<ClaimVerification>,
}

impl std::fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Verification: {}/{} verified, {} unverifiable, {} contradicted",
            self.verified, self.total_claims, self.unverifiable, self.contradicted)
    }
}

/// Extract verifiable claims from ontology nodes.
/// Returns (claim_text, search_query) pairs.
pub fn extract_verifiable_claims(nodes: &[crate::argument_graph::ArgumentNode]) -> Vec<(String, String)> {
    use crate::argument_graph::NodeType;

    let mut claims = Vec::new();

    for node in nodes {
        let query = match node.node_type {
            // Citations: search for the exact reference
            NodeType::Citation => {
                let text = node.source_text.as_deref().unwrap_or(&node.text);
                extract_citation_query(text)
            }
            // Quantified evidence: search for the statistic
            NodeType::QuantifiedEvidence => {
                let text = node.source_text.as_deref().unwrap_or(&node.text);
                extract_stat_query(text)
            }
            // Evidence: search if it contains named entities
            NodeType::Evidence => {
                let text = node.source_text.as_deref().unwrap_or(&node.text);
                if has_named_entity(text) {
                    Some(extract_entity_query(text))
                } else {
                    None
                }
            }
            // Thesis/claims: generally not web-verifiable
            _ => None,
        };

        if let Some(q) = query {
            let claim_text = node.source_text.as_deref().unwrap_or(&node.text).to_string();
            claims.push((claim_text, q));
        }
    }

    claims
}

/// Search the web for a query using a simple HTTPS request.
/// Returns the page content or error message.
pub async fn web_search(query: &str) -> Result<String, String> {
    // Use DuckDuckGo HTML search (no API key needed)
    let encoded = urlencoding::encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={}", encoded);

    let client = reqwest::Client::builder()
        .user_agent("BITF-Verify/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let text = resp.text()
        .await
        .map_err(|e| format!("Read failed: {}", e))?;

    // Extract result snippets from DuckDuckGo HTML
    let snippets = extract_snippets(&text);
    if snippets.is_empty() {
        Ok("0 results found".to_string())
    } else {
        Ok(snippets.join("\n---\n"))
    }
}

/// Verify a list of claims against the web.
pub async fn verify_claims(claims: &[(String, String)]) -> VerificationReport {
    let mut results = Vec::new();
    let mut verified = 0;
    let mut unverifiable = 0;
    let mut contradicted = 0;

    for (claim, query) in claims {
        let (status, search_result) = match web_search(query).await {
            Ok(content) => {
                if content == "0 results found" || content.len() < 50 {
                    (VerificationStatus::Unverifiable, "No relevant results found".to_string())
                } else {
                    // Check if results corroborate or contradict
                    let claim_lower = claim.to_lowercase();
                    let content_lower = content.to_lowercase();

                    // Simple heuristic: if key terms from claim appear in results
                    let claim_words: Vec<&str> = claim_lower.split_whitespace()
                        .filter(|w| w.len() > 4)
                        .collect();
                    let matches = claim_words.iter()
                        .filter(|w| content_lower.contains(**w))
                        .count();
                    let match_ratio = if claim_words.is_empty() { 0.0 } else {
                        matches as f64 / claim_words.len() as f64
                    };

                    if match_ratio > 0.5 {
                        (VerificationStatus::Verified, format!("Found corroborating results ({}% term match)", (match_ratio * 100.0) as u32))
                    } else {
                        (VerificationStatus::Unverifiable, format!("Results found but no corroboration ({}% term match)", (match_ratio * 100.0) as u32))
                    }
                }
            }
            Err(e) => {
                (VerificationStatus::Pending, format!("Search failed: {}", e))
            }
        };

        match status {
            VerificationStatus::Verified => verified += 1,
            VerificationStatus::Unverifiable => unverifiable += 1,
            VerificationStatus::Contradicted => contradicted += 1,
            VerificationStatus::Pending => {}
        }

        results.push(ClaimVerification {
            claim: if claim.len() > 100 { format!("{}...", &claim[..100]) } else { claim.clone() },
            search_query: query.clone(),
            search_result,
            status,
        });
    }

    VerificationReport {
        total_claims: results.len(),
        verified,
        unverifiable,
        contradicted,
        claims: results,
    }
}

// === Helper functions ===

fn extract_citation_query(text: &str) -> Option<String> {
    // Look for author names, years, publication names
    let mut parts = Vec::new();

    // Find year pattern
    let year_re = regex_lite::Regex::new(r"\b(19|20)\d{2}\b").ok()?;
    if let Some(m) = year_re.find(text) {
        parts.push(m.as_str().to_string());
    }

    // Find capitalized names (likely proper nouns)
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
        if clean.len() > 2 && clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            && !["The", "This", "That", "Our", "We", "In", "For", "And", "But"].contains(&clean)
        {
            parts.push(clean.to_string());
        }
    }

    if parts.len() >= 2 {
        Some(parts.join(" "))
    } else {
        None
    }
}

fn extract_stat_query(text: &str) -> Option<String> {
    // Look for numbers with context
    let num_re = regex_lite::Regex::new(r"\d+[\.\d]*\s*[%£$€MBKk]|\d+[\.\d]*\s*(million|billion|percent|improvement|reduction|increase)").ok()?;
    if let Some(m) = num_re.find(text) {
        // Get surrounding context
        let start = m.start().saturating_sub(30);
        let end = (m.end() + 30).min(text.len());
        let context = &text[start..end];
        // Extract key words for search
        let words: Vec<&str> = context.split_whitespace()
            .filter(|w| w.len() > 3)
            .take(8)
            .collect();
        if words.len() >= 3 {
            return Some(words.join(" "));
        }
    }
    None
}

fn has_named_entity(text: &str) -> bool {
    // Check if text contains proper nouns (capitalized words that aren't sentence starters)
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        if i == 0 { continue; }
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
        if clean.len() > 2 && clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return true;
        }
    }
    false
}

fn extract_entity_query(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let entities: Vec<&str> = words.iter()
        .filter(|w| {
            let clean = w.trim_matches(|c: char| !c.is_alphanumeric());
            clean.len() > 2 && clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                && !["The", "This", "That", "Our", "We", "In", "For", "And", "But", "A"].contains(&clean)
        })
        .copied()
        .take(6)
        .collect();
    entities.join(" ")
}

fn extract_snippets(html: &str) -> Vec<String> {
    let mut snippets = Vec::new();
    // DuckDuckGo HTML wraps results in <a class="result__snippet">
    for part in html.split("result__snippet") {
        if let Some(start) = part.find('>')
            && let Some(end) = part[start..].find('<')
        {
            let snippet = &part[start+1..start+end];
            let clean = snippet.replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("<b>", "")
                .replace("</b>", "");
            if clean.len() > 20 {
                snippets.push(clean.trim().to_string());
            }
        }
    }
    snippets.truncate(5); // Top 5 results
    snippets
}
