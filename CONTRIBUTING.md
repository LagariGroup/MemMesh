# MemMesh Contributor Guide &amp; Internal Architecture

Thank you for contributing to MemMesh! This document details internal design decisions, repository structure, local development environment setup, and coding conventions.

\-------------------------------------------------------------------------------- 

## 1\. Deep Dive into Internal Architecture

### 1.1 Why SQLite + `sqlite-vec` \+ `FTS5`?

Traditional vector databases (Pinecone, Milvus, Qdrant cluster) introduce significant operational friction for individual developers and small teams. MemMesh uses embedded SQLite for several reasons:

* **Zero Configuration**: A single portable `.db` file requires no separate server setup.
* **Hybrid Retrieval in a Single ACID Transaction**: Full-text keyword matching (`FTS5`) and vector similarity (`sqlite-vec`) execute in the same database without two-phase commit overhead.
* **Low Memory Footprint**: Uses &lt; 50MB RAM on idle, making it ideal for low-power edge nodes like Raspberry Pi 5.

### 1.2 In-Process Local Embeddings

* MemMesh packages `bge-small-en-v1.5` via the embedded ONNX Runtime.
* Text normalization, tokenization (WordPiece), and matrix inference occur in-process in C++/Rust.
* Ingestion time is roughly 5–15ms per record on modern multi-core CPUs.
* If preferred, users can configure an external embedding provider (e.g., local Ollama or OpenAI/Voyage API) via `MEMMESH_EMBEDDING_PROVIDER`.

### 1.3 Reciprocal Rank Fusion (RRF) Algorithm

Hybrid search combines text search and vector search results using Reciprocal Rank Fusion:

```
RRF_Score(doc) = (1 / (60 + Rank_Vector(doc))) + (1 / (60 + Rank_BM25(doc)))

```

This ensures that exact matches (such as file paths, method names, or compiler error codes) are not overshadowed by generic semantic proximity, and vice versa.

### 1.4 Consolidation &amp; Pruning Engine

A background thread periodically processes unindexed episodic memory traces:

1. Gathers execution sessions from the past 24 hours.
2. Identifies recurrent patterns (e.g., repeated build failures or debugging solutions).
3. Synthesizes concise semantic facts.
4. Prunes expired working memory and scratchpad items whose `ttl_seconds` has elapsed.

\-------------------------------------------------------------------------------- 

## 2\. Monorepo Structure

```
memmesh/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── memmesh-core/           # Core domain models, SQLite schema, RRF query engine
│   ├── memmesh-embed/          # ONNX Runtime embedding engine &amp; tokenizer
│   ├── memmesh-storage/        # SQLite WAL, sqlite-vec, and Markdown mirror writer
│   ├── memmesh-server/         # MCP Server (Stdio/SSE) &amp; Axum REST API
│   └── memmesh-cli/            # Command-line interface (`memmesh`)
├── integrations/               # Agent-specific plugins &amp; configuration templates
│   ├── claude-code/
│   ├── antigravity/
│   ├── opencode/
│   └── hermes/
├── tests/
│   └── integration/            # Cross-agent simulation and MCP compliance tests
├── docs/                       # Architecture diagrams and specifications
└── docker-compose.yml

```

\-------------------------------------------------------------------------------- 

## 3\. Local Development Setup

### Prerequisites

* Rust 1.78+ (`rustup default stable`)
* CMake and Clang (for ONNX Runtime bindings)
* SQLite3 development headers (`libsqlite3-dev`)
* Docker &amp; Docker Compose (for integration testing)

### Build Steps

```
# 1. Clone the repo
git clone https://github.com/memmesh/memmesh.git
cd memmesh

# 2. Build the workspace
cargo build

# 3. Run unit and integration tests
cargo test

# 4. Start local test daemon
cargo run -p memmesh-cli -- daemon start --port 8740 --dev

```

### Running MCP Compliance Tests

MemMesh includes a suite of automated MCP test vectors to ensure compatibility with Claude, Antigravity, and OpenCode:

```
cargo test -p memmesh-server --test mcp_compliance

```

\-------------------------------------------------------------------------------- 

## 4\. Coding Standards &amp; PR Guidelines

1. **Safety &amp; Concurrency**:
  * SQLite access must use connection pools with serialized writes and parallel readers (`WAL` mode).
  * Never block the async runtime (`tokio`) with heavy vector calculations; always use `tokio::task::spawn_blocking` for ONNX inference.
2. **Error Handling**:
  * Use strongly typed error enums (`thiserror`) inside crates.
  * Return clean, actionable error strings over the MCP protocol.
3. **Commit Messages**:
  * Follow Conventional Commits: `feat:`, `fix:`, `docs:`, `perf:`, `refactor:`.
4. **Documentation**:
  * Any new MCP tool or REST endpoint must be documented in both `API_AND_MCP_SPEC.md` and the OpenAPI generator tests.

\-------------------------------------------------------------------------------- 

## 5\. Community &amp; Code of Conduct

We adhere to the Contributor Covenant Code of Conduct. Please be welcoming, respectful, and collaborative.