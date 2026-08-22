//! A native-Rust ANN vector index built on
//! [turbovec](https://github.com/RyanCodrai/turbovec)'s `IdMapIndex` —
//! its stable, revocable-by-id design (`add_with_ids`/`remove`/
//! `search_with_allowlist`) maps directly onto this app's per-agent
//! File Access grant model: an embedding's external id can be the
//! granted file's row id, a grant revocation is one `remove(id)`, and
//! `search`'s `allowlist` parameter is exactly "restrict results to
//! this agent's currently-granted files" with no need to re-filter
//! results after the fact.
//!
//! **Not wired into `invoke`/`semantic_search` yet.** Today,
//! `ml/_engine.py`'s `semantic_search` capability computes cosine
//! similarity in Python (numpy) over whatever embeddings it holds
//! in-process per call — see the module doc at the top of this file's
//! parent (`ml_engine::mod`) and `ML Engine Design.md` in the vault.
//! Replacing that with this index for real would mean deciding where a
//! per-agent `.tvim` file persists, when it's rebuilt after a file
//! grant is revoked, and how the Python bridge's embedding step feeds
//! vectors into a Rust-owned index across the JSON-RPC boundary —
//! real design decisions this module doesn't make on its own. This is
//! a tested, ready-to-integrate building block for when that design is
//! decided, not a claim that semantic search already uses it.

// Staged building block, not wired into any caller yet (see module docs)
// — allow dead_code rather than deleting real, unit-tested logic just
// because nothing calls it across the crate boundary yet.
#![allow(dead_code)]

use turbovec::IdMapIndex;

#[derive(Debug, PartialEq)]
pub enum VectorIndexError {
    /// `dim` wasn't a positive multiple of 8, or `bit_width` wasn't in
    /// `2..=4` — see `turbovec::ConstructError`.
    InvalidConfig(String),
    /// `vector.len() != dim`, or `id` was already present.
    InvalidInput(String),
}

impl std::fmt::Display for VectorIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VectorIndexError::InvalidConfig(msg) => write!(f, "invalid vector index config: {msg}"),
            VectorIndexError::InvalidInput(msg) => write!(f, "invalid vector index input: {msg}"),
        }
    }
}

/// A single search hit: the external id passed to `add`, and its
/// distance score (turbovec's convention: lower is closer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchHit {
    pub id: u64,
    pub score: f32,
}

#[derive(Debug)]
pub struct VectorIndex {
    inner: IdMapIndex,
    dim: usize,
}

impl VectorIndex {
    /// `bit_width` is turbovec's quantization width (2–4 bits/dimension
    /// — see the crate's README for the recall/memory tradeoff); this
    /// app has no opinion on which to default to yet, so callers choose
    /// explicitly rather than this wrapper picking one silently.
    pub fn new(dim: usize, bit_width: usize) -> Result<Self, VectorIndexError> {
        let inner = IdMapIndex::new(dim, bit_width).map_err(|e| VectorIndexError::InvalidConfig(e.to_string()))?;
        Ok(Self { inner, dim })
    }

    /// Adds one vector under `id`. Fails if `id` is already present
    /// (use `remove` first to replace) or `vector.len() != dim`.
    pub fn add(&mut self, id: u64, vector: &[f32]) -> Result<(), VectorIndexError> {
        if vector.len() != self.dim {
            return Err(VectorIndexError::InvalidInput(format!(
                "vector has {} dimensions, index expects {}",
                vector.len(),
                self.dim
            )));
        }
        self.inner
            .add_with_ids(vector, &[id])
            .map_err(|e| VectorIndexError::InvalidInput(e.to_string()))
    }

    /// Removes `id` if present. Returns whether it was actually there —
    /// mirrors `IdMapIndex::remove`, which is `O(1)` (swap-remove, not a
    /// linear scan), so callers can call this per revoked grant without
    /// worrying about index size.
    pub fn remove(&mut self, id: u64) -> bool {
        self.inner.remove(id)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    /// Top-`k` nearest ids to `query`. When `allowlist` is `Some`,
    /// results are restricted to those ids — this is the direct match
    /// for "search only this agent's currently-granted files," since
    /// turbovec applies the allowlist inside the search kernel rather
    /// than filtering after the fact (see the crate README's "Filter at
    /// search time" section).
    pub fn search(&self, query: &[f32], k: usize, allowlist: Option<&[u64]>) -> Result<Vec<SearchHit>, VectorIndexError> {
        if query.len() != self.dim {
            return Err(VectorIndexError::InvalidInput(format!(
                "query has {} dimensions, index expects {}",
                query.len(),
                self.dim
            )));
        }
        let (scores, ids) = self
            .inner
            .search_with_allowlist(query, k, allowlist)
            .map_err(|e| VectorIndexError::InvalidInput(e.to_string()))?;
        Ok(scores.into_iter().zip(ids).map(|(score, id)| SearchHit { id, score }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // dim must be a positive multiple of 8 (turbovec::ConstructError::DimNotPositiveMultipleOf8).
    const DIM: usize = 8;

    fn e(i: usize) -> Vec<f32> {
        // A one-hot-ish basis vector, `i`-th coordinate set — trivially
        // distinguishable nearest neighbors without needing real
        // embeddings.
        let mut v = vec![0.0; DIM];
        v[i % DIM] = 1.0;
        v
    }

    #[test]
    fn rejects_an_invalid_bit_width() {
        let err = VectorIndex::new(DIM, 9).unwrap_err();
        assert!(matches!(err, VectorIndexError::InvalidConfig(_)));
    }

    #[test]
    fn rejects_a_dim_that_is_not_a_multiple_of_eight() {
        let err = VectorIndex::new(7, 4).unwrap_err();
        assert!(matches!(err, VectorIndexError::InvalidConfig(_)));
    }

    #[test]
    fn add_then_search_finds_the_nearest_real_neighbor() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        index.add(1, &e(0)).unwrap();
        index.add(2, &e(1)).unwrap();
        index.add(3, &e(2)).unwrap();

        let hits = index.search(&e(1), 1, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 2);
    }

    #[test]
    fn search_respects_an_allowlist_even_when_a_closer_id_is_excluded() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        index.add(1, &e(0)).unwrap();
        index.add(2, &e(1)).unwrap(); // the true nearest neighbor to e(1)
        index.add(3, &e(2)).unwrap();

        // Query for e(1) but only allow ids 1 and 3 — id 2 (the exact
        // match) must not appear, proving the allowlist actually
        // constrains the search rather than being ignored.
        let hits = index.search(&e(1), 3, Some(&[1, 3])).unwrap();
        let ids: Vec<u64> = hits.iter().map(|h| h.id).collect();
        assert!(!ids.contains(&2), "allowlist should have excluded id 2, got {ids:?}");
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn remove_makes_a_vector_unfindable() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        index.add(1, &e(0)).unwrap();
        index.add(2, &e(1)).unwrap();
        assert!(index.remove(1));
        assert_eq!(index.len(), 1);

        let hits = index.search(&e(0), 2, None).unwrap();
        assert!(!hits.iter().any(|h| h.id == 1));
    }

    #[test]
    fn remove_of_an_absent_id_returns_false_without_panicking() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        index.add(1, &e(0)).unwrap();
        assert!(!index.remove(999));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn add_rejects_a_vector_of_the_wrong_dimension() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        let err = index.add(1, &[0.0, 1.0]).unwrap_err();
        assert!(matches!(err, VectorIndexError::InvalidInput(_)));
    }

    #[test]
    fn add_rejects_a_duplicate_id() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        index.add(1, &e(0)).unwrap();
        let err = index.add(1, &e(1)).unwrap_err();
        assert!(matches!(err, VectorIndexError::InvalidInput(_)));
    }

    #[test]
    fn search_rejects_a_query_of_the_wrong_dimension() {
        let index = VectorIndex::new(DIM, 4).unwrap();
        let err = index.search(&[0.0, 1.0], 1, None).unwrap_err();
        assert!(matches!(err, VectorIndexError::InvalidInput(_)));
    }

    #[test]
    fn k_is_clamped_to_the_index_size() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        index.add(1, &e(0)).unwrap();
        let hits = index.search(&e(0), 50, None).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn is_empty_reflects_index_state() {
        let mut index = VectorIndex::new(DIM, 4).unwrap();
        assert!(index.is_empty());
        index.add(1, &e(0)).unwrap();
        assert!(!index.is_empty());
    }
}
