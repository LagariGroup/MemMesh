# MemMesh Multi-Agent Configuration Manual



Step-by-step instructions for integrating MemMesh into each target agent harness across machines.

---

### 1. Claude Code (Anthropic CLI)



Claude Code supports native MCP servers and custom CLI environment variables.

#### Option A: Registering as an MCP Server (Recommended)



Run inside your terminal on any connected machine:

```bash
claude mcp add memmesh http://<mesh-host-ip>:8740/sse

```

To verify:

```bash
claude mcp list

```

#### Option B: Automatic Context Pre-Injection



Add a wrapper or project hook to inject relevant memories at session start.

Create `.claude/hooks/pre-prompt.sh`:

```bash
#!/usr/bin/env bash
PROJECT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
memmesh context --project "$PROJECT_DIR" --format markdown

```

Claude Code will automatically receive repository conventions and recent handoff notes before processing user prompts.

---

### 2. Claude Desktop & Claude CoWork



#### Claude Desktop



In `~/.config/Claude/claude_desktop_config.json` (Linux) or `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS):

```json
{
  "mcpServers": {
    "memmesh": {
      "command": "memmesh-cli",
      "args": ["mcp-stdio", "--remote", "http://<mesh-ip>:8740"]
    }
  }
}

```

#### Claude CoWork



In your Claude CoWork workspace settings:

1. Navigate to **Workspace Settings** > **Integrations** > **Model Context Protocol (MCP)**.


2. Select **Add Custom Server (SSE)**.


3. Provide URL: `http://<mesh-host-ip>:8740/sse`.


4. Enter Bearer Token under Authorization Headers.



---

### 3. Google Antigravity, Antigravity-CLI & Antigravity-IDE



Antigravity natively implements MCP tools and session environments.

#### Antigravity-IDE



Add to your global or workspace configuration (`~/.antigravity/settings.json`):

```json
{
  "antigravity.mcp.servers": {
    "memmesh": {
      "url": "http://<mesh-host-ip>:8740/sse",
      "transport": "sse",
      "headers": {
        "Authorization": "Bearer <YOUR_AUTH_TOKEN>"
      }
    }
  },
  "antigravity.memory.autoHandoff": true
}

```

#### Antigravity-CLI



When executing commands via terminal:

```bash
# Register persistent MCP configuration
antigravity mcp add --name memmesh --url "http://<mesh-host-ip>:8740/sse"

# Pre-pipe memory context for standalone runs
memmesh inject | antigravity run --prompt "Implement database migration for user preferences"

```

---

### 4. OpenCode



In your repository root or global config (`~/.config/opencode/config.json` or `./opencode.json`):

```json
{
  "$schema": "https://opencode.dev/schema.json",
  "mcpServers": {
    "memmesh": {
      "url": "http://<mesh-host-ip>:8740/sse",
      "headers": {
        "Authorization": "Bearer <YOUR_AUTH_TOKEN>"
      }
    }
  }
}

```

OpenCode will detect `memory_recall`, `memory_store`, and `memory_handoff` as callable tools.

---

### 5. Hermes Agent



Hermes Agent connects directly to custom tool definitions and respects persona guidelines in `SOUL.md`.

#### Step 1: Add Tool Definition to Hermes Config (`tools.yaml`)



```yaml
tools:
  - name: memory_recall
    description: "Search user's cross-machine memory for architecture rules and past session context."
    endpoint: "http://<mesh-host-ip>:8740/v1/memories/search"
    method: "POST"
  - name: memory_store
    description: "Save a permanent insight, bugfix, or operational playbook."
    endpoint: "http://<mesh-host-ip>:8740/v1/memories"
    method: "POST"

```

#### Step 2: System Persona Guidance (`SOUL.md`)



Add the following instruction block:

```markdown
### Long-Term Memory Fabric (MemMesh)
- You have access to a cross-machine, cross-agent memory system.
- Before embarking on unfamiliar architectural changes, invoke `memory_recall` with the project name or error keywords.
- When an operation requires follow-up in an IDE or CLI harness, write a handoff ticket using `memory_handoff`.

```

---

### 6. OpenAI Codex CLI



Create a shell wrapper script (`codex-mem`):

```bash
#!/usr/bin/env bash
CONTEXT=$(memmesh-cli context --query "$*" --limit 3)
codex --context "$CONTEXT" "$@"

```

---

### 7. Paperclip & Pi



* **Paperclip**: Import the MemMesh OpenAPI spec into Paperclip tool bindings.


* **Pi**: Run a lightweight Python daemon client querying `http://<mesh-ip>:8740/v1/memories/search`.