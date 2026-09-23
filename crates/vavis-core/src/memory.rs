//! Semantic memory: finding the facts that matter to what is being said.
//!
//! Facts used to be reachable only through the `search_memory` tool, by
//! keyword. The model had to think of looking, and then had to guess a word
//! the fact happened to contain -- "kahve" found nothing saved as "espresso
//! içer". In practice the assistant knew things about the user and never
//! used them.
//!
//! Now every turn looks up the facts relevant to the message and hands them
//! to the model up front. Relevance is a blend of two signals:
//!
//! - **meaning**: cosine similarity between embeddings, when an embedding
//!   provider is available. This is what matches "coffee" to "espresso";
//! - **words**: BM25 over the fact text (`crate::search`), which needs no
//!   provider at all and is exact where meaning is fuzzy -- names, numbers.
//!
//! Either alone works; together they cover each other's blind spots. The
//! embeddings themselves are computed elsewhere (the brain layer knows the
//! providers); this module stores them and does the arithmetic.

use crate::error::Result;
use crate::search::{Document, SearchIndex};
use crate::store::{Fact, Store};

/// A fact with its embedding, when it has one for the current model.
#[derive(Debug, Clone, PartialEq)]
pub struct Remembered {
    pub fact: Fact,
    pub embedding: Option<Vec<f32>>,
}

/// Below this, a fact is not relevant enough to put in front of the model.
/// Chosen so an unrelated fact stays out of an unrelated turn: a stray
/// "kullanıcının kedisi var" in a question about CPU usage is noise, and
/// noise in a system prompt costs attention.
pub const MIN_RELEVANCE: f32 = 0.35;

impl Store {
    pub(crate) fn migrate_fact_memory(&self) -> Result<()> {
        let conn = self.connection();
        if !self.has_column("facts", "source")? {
            conn.execute_batch(
                "ALTER TABLE facts ADD COLUMN source TEXT NOT NULL DEFAULT 'user';",
            )?;
        }
        if !self.has_column("facts", "embedding")? {
            conn.execute_batch("ALTER TABLE facts ADD COLUMN embedding BLOB;")?;
        }
        if !self.has_column("facts", "embedding_model")? {
            conn.execute_batch("ALTER TABLE facts ADD COLUMN embedding_model TEXT;")?;
        }
        Ok(())
    }

    /// Stores a fact's embedding, recording which model made it: vectors
    /// from two models are not comparable, so each is only used with the
    /// model it came from.
    pub fn set_fact_embedding(&self, id: i64, model: &str, vector: &[f32]) -> Result<()> {
        self.connection().execute(
            "UPDATE facts SET embedding = ?1, embedding_model = ?2 WHERE id = ?3",
            rusqlite::params![encode(vector), model, id],
        )?;
        Ok(())
    }

    /// Every fact, with its embedding when it was made by `model`.
    pub fn remembered(&self, model: Option<&str>) -> Result<Vec<Remembered>> {
        let mut stmt = self.connection().prepare(
            "SELECT id, text, created_at, source, embedding, embedding_model
             FROM facts ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            let blob: Option<Vec<u8>> = r.get(4)?;
            let made_by: Option<String> = r.get(5)?;
            Ok((
                Fact {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    created_at: r.get(2)?,
                    source: r.get(3)?,
                },
                blob,
                made_by,
            ))
        })?;
        Ok(rows
            .filter_map(std::result::Result::ok)
            .map(|(fact, blob, made_by)| {
                let embedding = match (model, made_by, blob) {
                    (Some(want), Some(have), Some(blob)) if want == have => decode(&blob),
                    _ => None,
                };
                Remembered { fact, embedding }
            })
            .collect())
    }

    /// Facts with no embedding from `model` yet, oldest first.
    pub fn facts_needing_embedding(&self, model: &str, limit: usize) -> Result<Vec<Fact>> {
        let mut stmt = self.connection().prepare(
            "SELECT id, text, created_at, source FROM facts
             WHERE embedding IS NULL OR embedding_model IS NULL OR embedding_model != ?1
             ORDER BY id LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![model, limit as i64], |r| {
            Ok(Fact {
                id: r.get(0)?,
                text: r.get(1)?,
                created_at: r.get(2)?,
                source: r.get(3)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }
}

/// Little-endian `f32`s. Portable across machines, and a 768-dimension
/// vector is 3 KB -- a few thousand facts fit in a few megabytes.
fn encode(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn decode(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.is_empty() || blob.len() % 4 != 0 {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// Cosine similarity, 0 for mismatched or empty vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// The facts most relevant to `query`, best first, with their scores.
///
/// Scores are in 0..=1. With embeddings, meaning carries most of the
/// weight and words add to it; without, words alone decide. Only facts at
/// or above `min` come back.
pub fn relevant(
    query: &str,
    query_embedding: Option<&[f32]>,
    facts: &[Remembered],
    limit: usize,
    min: f32,
) -> Vec<(Fact, f32)> {
    if facts.is_empty() || query.trim().is_empty() {
        return Vec::new();
    }

    // Words: BM25 scores are unbounded, so they are scaled against the best
    // hit. A best hit that is itself weak still scales to 1, which is why
    // the lexical share is capped below.
    let index = SearchIndex::build(
        facts
            .iter()
            .map(|r| Document {
                id: r.fact.id,
                text: r.fact.text.clone(),
            })
            .collect(),
    );
    let hits = index.search(query, facts.len());
    let best = hits.first().map(|h| h.score).unwrap_or(0.0);
    let lexical = |id: i64| -> f32 {
        if best <= 0.0 {
            return 0.0;
        }
        hits.iter()
            .find(|h| h.id == id)
            .map(|h| (h.score / best) as f32)
            .unwrap_or(0.0)
    };

    let mut scored: Vec<(Fact, f32)> = facts
        .iter()
        .map(|r| {
            let words = lexical(r.fact.id);
            let meaning = match (query_embedding, &r.embedding) {
                (Some(q), Some(e)) => Some(cosine(q, e)),
                _ => None,
            };
            let score = match meaning {
                // Embedding models put unrelated text around 0.3-0.5 and
                // related text above ~0.6, so the raw cosine is stretched
                // onto 0..1 from that floor before it is blended.
                Some(c) => (0.75 * ((c - 0.3) / 0.5).clamp(0.0, 1.0) + 0.25 * words).min(1.0),
                // Words alone: a matching word is strong evidence, but a
                // single common word matching is not certainty.
                None => 0.8 * words,
            };
            (r.fact.clone(), score)
        })
        .filter(|(_, s)| *s >= min)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

/// Whether `candidate` says what an existing fact already says.
///
/// Background extraction sees the same fact come up in conversation after
/// conversation; without this the memory fills with ten phrasings of
/// "kullanıcının adı Ali". Meaning decides when both have embeddings;
/// otherwise a heavy word overlap does.
pub fn is_duplicate(
    candidate: &str,
    candidate_embedding: Option<&[f32]>,
    existing: &[Remembered],
) -> bool {
    let words = |t: &str| -> std::collections::HashSet<String> {
        crate::search::tokenize(t).into_iter().collect()
    };
    let cand = words(candidate);
    existing.iter().any(|r| {
        if let (Some(a), Some(b)) = (candidate_embedding, &r.embedding) {
            if cosine(a, b) >= 0.9 {
                return true;
            }
        }
        let other = words(&r.fact.text);
        if cand.is_empty() || other.is_empty() {
            return false;
        }
        let shared = cand.intersection(&other).count() as f32;
        let union = cand.union(&other).count() as f32;
        shared / union >= 0.7
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(id: i64, text: &str) -> Remembered {
        Remembered {
            fact: Fact {
                id,
                text: text.into(),
                created_at: 0,
                source: "user".into(),
            },
            embedding: None,
        }
    }

    fn with(mut r: Remembered, v: &[f32]) -> Remembered {
        r.embedding = Some(v.to_vec());
        r
    }

    #[test]
    fn vectors_survive_the_database() {
        let v = vec![0.25, -1.5, 3.0];
        assert_eq!(decode(&encode(&v)).unwrap(), v);
        assert!(decode(&[1, 2, 3]).is_none());
    }

    #[test]
    fn cosine_is_one_for_the_same_direction() {
        assert!((cosine(&[1.0, 2.0], &[2.0, 4.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn words_alone_find_a_fact_that_shares_them() {
        let facts = vec![
            fact(1, "Kullanıcının kedisinin adı Pamuk"),
            fact(2, "Kahveyi sade içer"),
        ];
        let hits = relevant("kedimin adı neydi", None, &facts, 5, MIN_RELEVANCE);
        assert_eq!(hits[0].0.id, 1);
        assert!(hits.iter().all(|(f, _)| f.id != 2));
    }

    #[test]
    fn meaning_finds_what_words_miss() {
        // "espresso" shares no word with "kahve"; only the vectors connect them.
        let facts = vec![
            with(fact(1, "Sabahları espresso içer"), &[0.9, 0.1, 0.0]),
            with(fact(2, "Futbol izlemeyi sever"), &[0.0, 0.2, 0.9]),
        ];
        let query = [0.85, 0.15, 0.05];
        let hits = relevant("bana kahve öner", Some(&query), &facts, 5, MIN_RELEVANCE);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.id, 1);
    }

    #[test]
    fn nothing_relevant_means_nothing_returned() {
        let facts = vec![fact(1, "Kullanıcının kedisi var")];
        assert!(relevant("cpu kullanımı kaç", None, &facts, 5, MIN_RELEVANCE).is_empty());
    }

    #[test]
    fn a_rephrased_fact_is_a_duplicate() {
        let existing = vec![fact(1, "Kullanıcının adı Ali")];
        assert!(is_duplicate("kullanıcının adı ali", None, &existing));
        assert!(!is_duplicate(
            "Kullanıcı İstanbul'da yaşıyor",
            None,
            &existing
        ));
    }

    #[test]
    fn close_vectors_are_a_duplicate_even_in_other_words() {
        let existing = vec![with(fact(1, "Adı Ali"), &[1.0, 0.0])];
        assert!(is_duplicate("İsmi Ali", Some(&[0.99, 0.05]), &existing));
    }

    #[test]
    fn embeddings_are_kept_per_model() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_fact("x").unwrap();
        s.set_fact_embedding(id, "model-a", &[1.0, 2.0]).unwrap();

        let a = s.remembered(Some("model-a")).unwrap();
        assert_eq!(a[0].embedding.as_deref(), Some(&[1.0, 2.0][..]));
        let b = s.remembered(Some("model-b")).unwrap();
        assert!(
            b[0].embedding.is_none(),
            "another model's vector is not comparable"
        );

        assert!(s.facts_needing_embedding("model-a", 10).unwrap().is_empty());
        assert_eq!(s.facts_needing_embedding("model-b", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_fact_remembers_its_source() {
        let s = Store::open_in_memory().unwrap();
        s.add_fact_from("sabah erken kalkar", "auto").unwrap();
        assert_eq!(s.all_facts().unwrap()[0].source, "auto");
    }
}
