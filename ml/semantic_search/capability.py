"""Semantic search capability: builds an in-memory sentence-embedding
index over documents handed to it (by `build_semantic_index`, which
sources them from `file_access::list_text_files_in_grants` — i.e. only
files inside folders the calling agent, or its current Group Chat
session, has actually been granted) and answers similarity queries
against that index.

Per ADR 0003 in the vault ("Bundle sentence-transformers for a Separate
ML Engine Process"), `sentence-transformers` + PyTorch are a required,
bundled dependency of this app's `ml_engine` process — not optional the
way a plain Skill's dependencies would be. This capability does not
attempt to run without them; a missing install is a packaging bug, not
a normal runtime condition to degrade gracefully from.

Indexes live only in this process's memory (the `ml_engine` subprocess's
lifetime = the app's lifetime) — there is no on-disk persistence in v1
(see `ML Engine Design.md`).
"""

import math

_indexes = {}
_model = None
_model_name = "all-MiniLM-L6-v2"


def _get_model():
    global _model
    if _model is not None:
        return _model
    from sentence_transformers import SentenceTransformer

    _model = SentenceTransformer(_model_name)
    return _model


def _cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    norm_a = math.sqrt(sum(x * x for x in a))
    norm_b = math.sqrt(sum(x * x for x in b))
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


def _action_status(_payload):
    model = _get_model()
    return {"available": True, "model": _model_name, "indexes": list(_indexes.keys())}


def _action_index(payload):
    index_name = payload["indexName"]
    documents = payload["documents"]
    if not documents:
        raise ValueError("no documents to index")
    model = _get_model()
    texts = [d["text"] for d in documents]
    vectors = model.encode(texts).tolist()
    _indexes[index_name] = {"docs": documents, "vectors": vectors}
    return {"indexName": index_name, "indexed": len(documents)}


def _action_search(payload):
    index_name = payload["indexName"]
    query = payload["query"]
    top_k = int(payload.get("topK", 5))
    index = _indexes.get(index_name)
    if index is None:
        raise RuntimeError(f"no index named '{index_name}' — call action 'index' first")
    model = _get_model()
    query_vector = model.encode([query]).tolist()[0]
    scored = [
        {"path": doc["path"], "score": _cosine(query_vector, vector), "excerpt": doc["text"][:400]}
        for doc, vector in zip(index["docs"], index["vectors"])
    ]
    scored.sort(key=lambda item: item["score"], reverse=True)
    return {"query": query, "results": scored[:top_k]}


def run(payload):
    action = payload.get("action")
    if action == "status":
        return _action_status(payload)
    if action == "index":
        return _action_index(payload)
    if action == "search":
        return _action_search(payload)
    raise ValueError(f"unknown action '{action}' — expected 'status', 'index', or 'search'")
