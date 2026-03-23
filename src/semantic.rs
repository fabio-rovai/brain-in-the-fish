//! Semantic search and embedding integration.
//!
//! Uses open-ontologies' TextEmbedder and VecStore to enable
//! semantic matching between document content and evaluation criteria.

use open_ontologies::graph::GraphStore;
use std::path::Path;
use std::sync::Arc;

/// Check if embedding models are available.
pub fn models_available() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    let model_dir = home.join(".open-ontologies").join("models");
    model_dir.join("bge-small-en-v1.5.onnx").exists()
        && model_dir.join("tokenizer.json").exists()
}

/// Paths to embedding model and tokenizer.
fn model_paths() -> (std::path::PathBuf, std::path::PathBuf) {
    let home = dirs::home_dir().unwrap_or_default();
    let model_dir = home.join(".open-ontologies").join("models");
    (
        model_dir.join("bge-small-en-v1.5.onnx"),
        model_dir.join("tokenizer.json"),
    )
}

/// Open (or create) the VecStore backed by a SQLite database in output_dir.
fn open_vecstore(output_dir: &Path) -> anyhow::Result<open_ontologies::vecstore::VecStore> {
    let db_path = output_dir.join(".embeddings.db");
    let db = open_ontologies::state::StateDb::open(&db_path)?;
    let mut store = open_ontologies::vecstore::VecStore::new(db);
    store.load_from_db()?;
    Ok(store)
}

/// Generate embeddings for all labelled entities in the graph store.
/// Returns the number of entities embedded.
pub fn embed_graph(
    graph: &Arc<GraphStore>,
    output_dir: &Path,
) -> anyhow::Result<usize> {
    if !models_available() {
        anyhow::bail!("Embedding models not found. Run 'open-ontologies init' to download.");
    }

    let (model_path, tokenizer_path) = model_paths();
    let embedder = open_ontologies::embed::TextEmbedder::load(&model_path, &tokenizer_path)?;
    let mut vecstore = open_vecstore(output_dir)?;

    // Query all entities with labels from the graph
    let query = r#"
        PREFIX eval: <http://brain-in-the-fish.dev/eval/>
        PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?s ?title ?text WHERE {
            { ?s eval:title ?title }
            UNION
            { ?s eval:text ?text }
            UNION
            { ?s eval:name ?title }
        }
    "#;

    let result_json = graph.sparql_select(query)?;
    let parsed: serde_json::Value = serde_json::from_str(&result_json)?;
    let results = parsed
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let struct_dim = 32;
    let zero_struct = vec![0.0f32; struct_dim];
    let mut count = 0;

    for row in &results {
        let obj = match row.as_object() {
            Some(o) => o,
            None => continue,
        };

        let iri = obj.get("s").and_then(|v| v.as_str()).unwrap_or("");
        let text = obj
            .get("title")
            .or(obj.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if iri.is_empty() || text.is_empty() {
            continue;
        }

        let clean_iri = iri.trim_start_matches('<').trim_end_matches('>');
        let clean_text = text.trim_start_matches('"').split('"').next().unwrap_or(text);

        match embedder.embed(clean_text) {
            Ok(vec) => {
                vecstore.upsert(clean_iri, &vec, &zero_struct);
                count += 1;
            }
            Err(_) => continue,
        }
    }

    vecstore.persist()?;
    Ok(count)
}

/// Search for entities semantically similar to a query.
pub fn semantic_search(
    query: &str,
    output_dir: &Path,
    top_k: usize,
) -> anyhow::Result<Vec<(String, f32)>> {
    if !models_available() {
        return Ok(Vec::new());
    }

    let (model_path, tokenizer_path) = model_paths();
    let embedder = open_ontologies::embed::TextEmbedder::load(&model_path, &tokenizer_path)?;
    let vecstore = open_vecstore(output_dir)?;

    let query_vec = embedder.embed(query)?;
    Ok(vecstore.search_cosine(&query_vec, top_k))
}

/// Compute cosine similarity between two entities by IRI.
/// Returns 0.0 if either IRI is not found.
pub fn semantic_similarity(
    iri_a: &str,
    iri_b: &str,
    output_dir: &Path,
) -> anyhow::Result<f64> {
    let vecstore = open_vecstore(output_dir)?;
    let vec_a = vecstore.get_text_vec(iri_a);
    let vec_b = vecstore.get_text_vec(iri_b);
    match (vec_a, vec_b) {
        (Some(a), Some(b)) => {
            Ok(open_ontologies::poincare::cosine_similarity(a, b) as f64)
        }
        _ => Ok(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_models_available_check() {
        // Just verifies the function doesn't panic
        let _ = models_available();
    }
}
