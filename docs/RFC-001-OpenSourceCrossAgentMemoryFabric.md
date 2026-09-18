RFC-001: Open-Source Cross-Agent Memory Fabric ("MemMesh")1. Executive Summary & Design TenetsMemMesh is an open-source, local-first, cross-machine agent memory system designed to provide shared state, long-term semantic recall, and inter-agent handoffs across disparate agent harnesses (Claude Code, Claude Desktop, Antigravity, OpenCode, Codex, Hermes, Paperclip, Pi).Core TenetsZero Lock-In & Vendor Neutral: Agents communicate using standardized open protocols—primarily Model Context Protocol (MCP) and OpenAPI/REST.Local-First & Private: Out-of-the-box local storage and embedded vector indexing; no mandatory third-party cloud dependency.Cross-Machine Mesh Native: Designed to operate smoothly over WireGuard mesh overlays (NetBird, Tailscale) connecting laptops, desktop workstations, and homelab servers.Transparent & Inspectable: Dual-representation storage where high-level semantic memories mirror into plain Markdown files for human inspection (e.g., Obsidian vaults or Git repositories).Single-Binary Daemon: Minimal resource footprint suitable for low-power hardware (such as a Raspberry Pi 5 or mini PC) as well as workstation nodes.
--------------------------------------------------------------------------------
2. System Architecture & Component Topology┌────────────────────────────────────────────────────────────────────────┐
│                        AGENT ECOSYSTEM LAYER                           │
│ Claude Code │ Antigravity │ OpenCode │ Hermes │ Codex │ Paperclip │ Pi │
└──────┬─────────────┬───────────┬─────────┬────────┬───────┬─────────┬──┘
       │             │           │         │        │       │         │
       ▼ (MCP/Stdio) ▼ (MCP/SSE) ▼ (MCP)   ▼ (MCP)  ▼(CLI)  ▼ (REST)  ▼(HTTP)
┌────────────────────────────────────────────────────────────────────────┐
│                       GATEWAY & ADAPTER LAYER                          │
│         - MCP Server (Stdio & SSE)     - REST / OpenAPI Daemon         │
│         - CLI Bridge (`mem`)           - Authentication / Scoping      │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
┌────────────────────────────────────▼───────────────────────────────────┐
│                          MEMMESH CORE ENGINE                           │
│  - Query Router & Hybrid Search (RRF: Dense Vector + BM25 FTS5)        │
│  - Working Memory Router (Active Scratchpad & Inter-Agent Handoffs)    │
│  - Consolidation & Pruning Engine (Background Worker / LLM Extractor)  │
│  - Markdown Mirror Sync (Obsidian / Git Vault Exporter)                │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
┌────────────────────────────────────▼───────────────────────────────────┐
│                             STORAGE TIER                               │
│  - SQLite (WAL mode) with `sqlite-vec` + `FTS5` (or LanceDB / DuckDB)  │
│  - Local ONNX Runtime Embedding Engine (`bge-small-en-v1.5` / `nomic`) │
│  - Plaintext Markdown Storage (`.memmesh/vault/`)                      │
└────────────────────────────────────────────────────────────────────────┘

--------------------------------------------------------------------------------
3. Data Model & Schema Definitions3.1 Memory Record Schema{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "MemoryRecord",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "tier": { 
      "type": "string", 
      "enum": ["working", "episodic", "semantic", "procedural"] 
    },
    "scope": { 
      "type": "string", 
      "description": "global | project:<git-remote-hash> | agent:<agent-name>" 
    },
    "title": { "type": "string" },
    "content": { "type": "string" },
    "tags": { 
      "type": "array", 
      "items": { "type": "string" } 
    },
    "provenance": {
      "type": "object",
      "properties": {
        "author_agent": { "type": "string" },
        "machine_id": { "type": "string" },
        "session_id": { "type": "string" },
        "timestamp": { "type": "string", "format": "date-time" }
      },
      "required": ["author_agent", "machine_id", "timestamp"]
    },
    "importance": { 
      "type": "number", 
      "minimum": 0.0, 
      "maximum": 1.0, 
      "default": 0.5 
    },
    "ttl_seconds": { 
      "type": "integer", 
      "nullable": true, 
      "description": "Null for persistent semantic/procedural records" 
    },
    "vector": { 
      "type": "array", 
      "items": { "type": "number" },
      "description": "384-d or 768-d dense embedding vector"
    }
  },
  "required": ["id", "tier", "scope", "content", "provenance"]
}
3.2 Inter-Agent Handoff SchemaUsed when passing active operational state between agents (e.g., Claude Code -> Antigravity-IDE):{
  "handoff_id": "uuid",
  "from_agent": "claude-code",
  "target_agent": "antigravity-ide",
  "project_scope": "project:repo-xyz",
  "task_summary": "Finished implementing Fastify schema; unit tests in tests/routes/auth.test.ts are failing on 401 response",
  "next_steps": [
    "Inspect mock JWT secret in test runner",
    "Verify token expiration handling in auth plugin"
  ],
  "associated_files": ["src/plugins/auth.ts", "tests/routes/auth.test.ts"],
  "created_at": "2026-09-17T20:30:00Z",
  "consumed": false
}

--------------------------------------------------------------------------------
4. Universal Protocol Specifications4.1 MCP (Model Context Protocol) ToolsMemMesh implements the standard Model Context Protocol:memory_recall:Inputs:query (string, required): Search query or semantic prompt.tier (enum: all, semantic, episodic, procedural, optional).scope (string, optional): Default to current project and global scopes.limit (integer, default: 5).Behavior: Performs Reciprocal Rank Fusion (RRF) between BM25 text match and cosine vector similarity.memory_store:Inputs:content (string, required).tier (enum: semantic, procedural, working).tags (string array, optional).scope (string, optional).Behavior: Generates embedding locally, inserts into SQLite, and writes Markdown file to vault mirror.memory_handoff:Inputs:target_agent (string, required).task_summary (string, required).next_steps (string array, optional).associated_files (string array, optional).Behavior: Writes an active handoff record tagged for the target agent.memory_consume_handoff:Inputs:agent_name (string, required).Behavior: Retrieves and marks active handoff tickets pending for this agent.4.2 REST / SSE APIGET /health -> Daemon status, database statistics, vector count.POST /v1/memories -> Store a new memory item.POST /v1/memories/search -> Perform hybrid query.GET /v1/handoffs/:agent -> Retrieve pending handoff tasks.POST /v1/handoffs -> Create handoff task.GET /v1/events/sse -> Server-Sent Events stream for real-time memory synchronization and handoffs across clients.
--------------------------------------------------------------------------------
5. Storage & Embedding Engine Details5.1 Zero-Dependency Embedded DatabaseSQLite Engine:wal=true, busy_timeout=5000, synchronous=NORMAL.Virtual Tables:memories_fts using FTS5(content, title, tags, tokenize='porter').memories_vec using sqlite-vec virtual table (float[384]).Hybrid Retrieval via Reciprocal Rank Fusion (RRF): $$\text{RRF Score}(d) = \sum_{m \in \{\text{vector}, \text{fts}\}} \frac{1}{60 + \text{rank}_m(d)}$$ This balances exact code/variable identifier matching with fuzzy semantic concepts.5.2 Local Embedding EngineIn-Process ONNX Runtime:Model: bge-small-en-v1.5 (384-dimensional, 33MB ONNX model) or nomic-embed-text-v1.5.Runs in ~4–12ms on modern x86/ARM CPUs without GPU requirements.Zero external cloud latency or billing overhead.
--------------------------------------------------------------------------------
6. Agent Configuration Recipes6.1 Claude CodeIn ~/.claude/config.json or run via CLI:claude mcp add memmesh http://<mesh-ip>:8740/sse
Pre-session injection hook in workspace root (.claude/hooks/pre-prompt.sh):#!/bin/bash
memmesh-cli context --project $(pwd) --format markdown
6.2 Claude DesktopIn ~/.config/Claude/claude_desktop_config.json (Linux) or ~/Library/Application Support/Claude/claude_desktop_config.json (macOS):{
  "mcpServers": {
    "memmesh": {
      "command": "memmesh-cli",
      "args": ["mcp-stdio", "--remote", "http://<mesh-ip>:8740"]
    }
  }
}
6.3 Google Antigravity & Antigravity-IDEIn Antigravity settings (~/.antigravity/settings.json):{
  "mcp.servers": {
    "memmesh": {
      "url": "http://<mesh-ip>:8740/sse",
      "transport": "sse"
    }
  }
}
6.4 OpenCodeIn workspace root opencode.json:{
  "$schema": "https://opencode.dev/schema.json",
  "mcpServers": {
    "memmesh": {
      "url": "http://<mesh-ip>:8740/sse"
    }
  }
}
6.5 Hermes AgentIn Hermes persona configuration (SOUL.md or Hermes tools config):## Tools
- `memmesh`: Access long-term memory and procedural guides via `memory_recall` and `memory_store`.
When switching projects or resuming work, invoke `memory_recall` to pull current architecture guidelines.
6.6 OpenAI Codex CLI / Shell Wrappercodex-mem wrapper:#!/usr/bin/env bash
CONTEXT=$(memmesh-cli context --query "$*" --limit 3)
codex --context "$CONTEXT" "$@"
6.7 Paperclip & PiPaperclip: Import the MemMesh OpenAPI spec into Paperclip tool bindings.Pi: Run a 30-line lightweight Python daemon client querying http://<mesh-ip>:8740/v1/memories/search.
--------------------------------------------------------------------------------
7. Cross-Machine Deployment & NetworkingDocker Compose Stack (docker-compose.yml)services:
  memmesh-daemon:
    image: ghcr.io/memmesh/memmesh:latest
    container_name: memmesh
    restart: unless-stopped
    ports:
      # Bound to internal mesh interface
      - "8740:8740"
    environment:
      - MEMMESH_LISTEN_ADDR=0.0.0.0:8740
      - MEMMESH_DB_PATH=/data/memmesh.db
      - MEMMESH_VAULT_PATH=/data/vault
      - MEMMESH_AUTH_TOKEN=${MEMMESH_AUTH_TOKEN}
      - MEMMESH_EMBEDDING_MODEL=bge-small-en-v1.5
    volumes:
      - ./data:/data
Mesh VPN TopologyConnect daemon host and clients over NetBird or Tailscale.Internal IP address (e.g., 100.x.y.z or 10.x.y.z) ensures peer-to-peer encryption with zero port forwarding on home/office routers.
--------------------------------------------------------------------------------
8. Open-Source Project Structure & RoadmapRepository Layoutmemmesh/
├── Cargo.toml / pyproject.toml
├── crates/
│   ├── memmesh-core/       # Storage engine, SQLite-vec, RRF search
│   ├── memmesh-embed/      # In-process ONNX embeddings
│   ├── memmesh-server/     # MCP SSE + REST daemon
│   └── memmesh-cli/        # CLI client (`memmesh-cli`)
├── integrations/
│   ├── claude-code/
│   ├── antigravity/
│   ├── opencode/
│   └── hermes/
├── docs/
│   ├── rfc/
│   └── setup-guides/
└── docker-compose.yml
Community & Contribution StrategyLicense: Apache-2.0 or MIT for permissive enterprise and indie adoption.Phase 1 (MVP): Standalone daemon with SQLite + FTS5 + ONNX embedding + MCP Stdio/SSE.Phase 2 (CLI & Handoffs): memmesh-cli with handoff queue and pre-prompt context dumping.Phase 3 (Vault Sync): Real-time Markdown bidirectional sync compatible with Obsidian.Phase 4 (Ecosystem Integrations): Ready-to-use plugin packages for OpenCode, Antigravity, and Hermes.