# MemMesh: Universal Agent Memory Fabric

## Open-Source, Local-First, Cross-Machine Agent Memory System

Unifying memory across Claude Code, Claude Desktop, Antigravity, OpenCode, Codex, Hermes, Paperclip, and Pi.

---

### 1. Why MemMesh?

Modern AI software development involves juggling multiple AI harnesses:

* **Claude Code** in terminal sessions for rapid edits
* **Antigravity-IDE & Antigravity-CLI** for full-stack engineering and complex refactors
* **OpenCode** for specialized tasks
* **Hermes Agent** running continuous background workflows
* **Codex CLI / scripts** for quick code transformations
* **Claude Desktop / CoWork** for high-level architectural brainstorming
* **Pi / edge agents** for ambient notifications and monitoring

---

### 2. Core Features

* **Zero Configuration**: A single portable `.db` file requires no separate server setup.
* **Hybrid Retrieval in a Single ACID Transaction**: Full-text keyword matching (FTS5) and vector similarity (sqlite-vec) execute in the same database without two-phase commit overhead.
* **Low Memory Footprint**: Uses < 50MB RAM on idle, making it ideal for low-power edge nodes like Raspberry Pi 5.
* **In-Process Local Embeddings**: Packages `bge-small-en-v1.5` via embedded ONNX Runtime. Text normalization, WordPiece tokenization, and matrix inference run in-process in C++/Rust. Optional external embedding providers (Ollama, OpenAI, Voyage API) via `MEMMESH_EMBEDDING_PROVIDER`.

---

### 3. Monorepo Structure

```text
memmesh/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── memmesh-core/           # Core domain models, SQLite schema, RRF query engine
│   ├── memmesh-embed/          # ONNX Runtime embedding engine & tokenizer
│   ├── memmesh-storage/        # SQLite WAL, sqlite-vec, and Markdown mirror writer
│   ├── memmesh-server/         # MCP Server (Stdio/SSE) & Axum REST API
│   └── memmesh-cli/            # Command-line interface (`memmesh`)
├── integrations/               # Agent-specific plugins & configuration templates
│   ├── claude-code/
│   ├── antigravity/
│   ├── opencode/
│   └── hermes/
├── tests/
│   └── integration/            # Cross-agent simulation and MCP compliance tests
├── docs/                       # Architecture diagrams and specifications
└── docker-compose.yml
```

---

### 4. Quick Start & Installation

#### Option A: Pre-Compiled Binary

```bash
# Download and install the CLI / daemon
curl -fsSL https://get.memmesh.dev | sh

# Start the daemon
memmesh daemon start --port 8740
```

#### Option B: Build from Source

##### Prerequisites

* Rust 1.78+ (`rustup default stable`)
* CMake and Clang (for ONNX Runtime bindings)
* SQLite3 development headers (`libsqlite3-dev`)
* Docker & Docker Compose (for integration testing)

##### Build Steps

```bash
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

##### Running MCP Compliance Tests

```bash
cargo test -p memmesh-server --test mcp_compliance
```

#### Option C: Docker Compose

```yaml
services:
  memmesh:
    image: ghcr.io/memmesh/memmesh:latest
    container_name: memmesh-daemon
    restart: unless-stopped
    ports:
      - "8740:8740"
    environment:
      - MEMMESH_LISTEN_ADDR=0.0.0.0:8740
      - MEMMESH_DB_PATH=/data/memmesh.db
      - MEMMESH_VAULT_PATH=/data/vault
      - MEMMESH_AUTH_TOKEN=${MEMMESH_AUTH_TOKEN}
      - MEMMESH_LOG_LEVEL=info
      - MEMMESH_EMBEDDING_MODEL=bge-small-en-v1.5
      - MEMMESH_CONSOLIDATION_CRON=0 2 * * *
    volumes:
      - ./data:/data
      - ./vault:/data/vault
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8740/health"]
      interval: 30s
      timeout: 5s
      retries: 3
```

Verify system health:

```bash
curl -H "Authorization: Bearer $MEMMESH_AUTH_TOKEN" http://localhost:8740/health
# {"status":"ok","version":"0.1.0","vectors":0,"records":0}
```

---

### 5. CLI Cheatsheet (`memmesh-cli`)

The CLI utility connects to your local or remote daemon over your WireGuard mesh network.

---

### 6. Repository Roadmap

* [x] Phase 1: Core Daemon (SQLite WAL + sqlite-vec + FTS5 + ONNX Embeddings)
* [x] Phase 2: MCP Stdio & SSE Transport Layer
* [ ] Phase 3: Bidirectional Obsidian Vault Synchronization
* [ ] Phase 4: Native VS Code / Antigravity IDE Extension
* [ ] Phase 5: Autonomous Daily Consolidation & Reflection Engine

---

### 7. Contributing Guidelines

* **Safety & Concurrency**: SQLite access requires connection pools with serialized writes and parallel readers in WAL mode. Use `tokio::task::spawn_blocking` for ONNX inference to avoid blocking the async runtime.
* **Error Handling**: Use typed error enums (`thiserror`) inside crates. Return actionable error strings over MCP.
* **Commit Messages**: Follow Conventional Commits (`feat:`, `fix:`, `docs:`, `perf:`, `refactor:`).
* **Documentation**: Document all new MCP tools or REST endpoints in `API_AND_MCP_SPEC.md` and OpenAPI tests.
* **Code of Conduct**: Adheres to the Contributor Covenant Code of Conduct.

---

### 8. License

Distributed under the Apache 2.0 License. See `LICENSE` for details.
