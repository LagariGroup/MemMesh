# MemMesh Protocol & API Reference Manual



This document defines the formal communication specifications for MemMesh across Model Context Protocol (MCP) tools and REST/SSE API endpoints.

---

### 1. Model Context Protocol (MCP) Specification



MemMesh runs as a compliant MCP server supporting both stdio and sse transports.

#### Transport Details



* **Stdio**: Invoked locally via `memmesh mcp-stdio` (useful for local desktop agents like Claude Desktop).


* **SSE (Server-Sent Events)**: Network-accessible via `http://<host>:8740/sse` with bidirectional messaging via `POST http://<host>:8740/messages?sessionId=<uuid>`.



---

#### 1.1 MCP Tool: `memory_recall`

Searches persistent semantic, procedural, and episodic memory using hybrid vector + keyword matching.

##### Parameters (JSON Schema)



```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "Natural language query, error message, or technical concept to search for."
    },
    "tier": {
      "type": "string",
      "enum": ["all", "semantic", "procedural", "episodic"],
      "default": "all",
      "description": "Specific memory tier to restrict search to."
    },
    "scope": {
      "type": "string",
      "description": "Optional scope filter (e.g., 'global', 'project:backend-api'). Defaults to current project + global."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 20,
      "default": 5,
      "description": "Maximum number of relevant memory snippets to return."
    }
  },
  "required": ["query"]
}

```

##### Return Value



```json
{
  "results": [
    {
      "id": "c1f76d49-41fa-4f51-a20c-7b47e5b3127a",
      "tier": "semantic",
      "title": "Fastify Plugin Registration Rule",
      "content": "All Fastify plugins must register using fastify-plugin to prevent route encapsulation from breaking global hooks.",
      "score": 0.892,
      "tags": ["fastify", "backend", "architecture"],
      "provenance": {
        "author_agent": "claude-code",
        "created_at": "2026-09-17T18:14:02Z"
      }
    }
  ]
}

```

---

#### 1.2 MCP Tool: `memory_store`

Records a persistent memory record into the system.

##### Parameters (JSON Schema)



```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The exact fact, convention, architectural rule, or solution to remember."
    },
    "tier": {
      "type": "string",
      "enum": ["semantic", "procedural", "working"],
      "default": "semantic",
      "description": "The memory category."
    },
    "title": {
      "type": "string",
      "description": "Brief descriptive title for the memory item."
    },
    "tags": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Keywords for indexing and filtering."
    },
    "scope": {
      "type": "string",
      "description": "Scope of the memory. Use 'global' for cross-project rules, or 'project:<hash>'."
    },
    "ttl_hours": {
      "type": "integer",
      "description": "Optional time-to-live in hours (primarily used for working/scratchpad entries)."
    }
  },
  "required": ["content"]
}

```

---

#### 1.3 MCP Tool: `memory_handoff`

Writes an active handoff record tagged for a target agent.

* **Inputs**:


* `target_agent` (string, required)


* `task_summary` (string, required)


* `next_steps` (string array, optional)


* `associated_files` (string array, optional)





---

#### 1.4 MCP Tool: `memory_consume_handoff`

Retrieves and acknowledges pending handoff tickets directed to the invoking agent.

##### Parameters (JSON Schema)



```json
{
  "type": "object",
  "properties": {
    "agent_name": {
      "type": "string",
      "description": "Identifier of the agent checking for pending handoffs."
    }
  },
  "required": ["agent_name"]
}

```

---

### 2. REST & Streaming API Specification



All REST endpoints require the HTTP header: `Authorization: Bearer <MEMMESH_AUTH_TOKEN>`

#### 2.1 System Health



* **`GET /health`**

* **Response (`200 OK`)**:





```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 34820,
  "storage": {
    "sqlite_wal_bytes": 4194304,
    "total_records": 1420,
    "vector_dimensions": 384
  }
}

```

#### 2.2 Memory Storage & Retrieval



* **`POST /v1/memories`**: Creates a memory record and initiates async vector indexing and Markdown mirror generation.


* **`POST /v1/memories/search`**: Performs hybrid query search.


* **Request Body**:





```json
{
  "query": "how to build android release",
  "limit": 5,
  "tier": "procedural"
}

```

* **`GET /v1/memories/:id`**: Returns full metadata, content, and provenance of a specific record.


* **`DELETE /v1/memories/:id`**: Deletes record from SQLite, vector index, and Markdown vault.



#### 2.3 Inter-Agent Handoff Queue



* **`POST /v1/handoffs`**: Creates a pending task handoff ticket.


* **`GET /v1/handoffs/pending?agent=<agent_name>`** (or `GET /v1/handoffs/:agent`): Returns list of unconsumed handoff tickets for the specified agent.


* **`POST /v1/handoffs/:id/consume`**: Marks the handoff ticket as consumed by the recipient agent.



#### 2.4 Server-Sent Events (SSE) Stream



* **`GET /v1/events/sse`**: Emits real-time event notifications:


* `event: memory_created`

* `event: handoff_created`

* `event: handoff_consumed`

* `event: consolidation_finished`