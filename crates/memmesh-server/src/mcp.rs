use memmesh_core::{HandoffStatus, InterAgentHandoff, MemoryRecord, MemoryTier, Provenance};
use memmesh_storage::StorageEngine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

pub fn handle_mcp_request(req: JsonRpcRequest, storage: Arc<StorageEngine>) -> JsonRpcResponse {
    match req.method.as_str() {
        "tools/list" => {
            let tools = serde_json::json!({
                "tools": [
                    {
                        "name": "memory_recall",
                        "description": "Searches persistent memory using hybrid vector + keyword matching[cite: 2].",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string" },
                                "tier": { "type": "string", "enum": ["all", "semantic", "procedural", "episodic"] },
                                "limit": { "type": "integer", "default": 5 }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "memory_store",
                        "description": "Stores a persistent memory record into the system[cite: 2].",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "tier": { "type": "string", "enum": ["semantic", "procedural", "working"] },
                                "title": { "type": "string" },
                                "tags": { "type": "array", "items": { "type": "string" } },
                                "scope": { "type": "string" }
                            },
                            "required": ["content"]
                        }
                    },
                    {
                        "name": "memory_handoff",
                        "description": "Writes an active task handoff ticket for a target agent[cite: 2].",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "target_agent": { "type": "string" },
                                "task_summary": { "type": "string" },
                                "next_steps": { "type": "array", "items": { "type": "string" } },
                                "associated_files": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["target_agent", "task_summary"]
                        }
                    },
                    {
                        "name": "memory_consume_handoff",
                        "description": "Consumes and acknowledges pending handoffs for the invoking agent[cite: 2].",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "agent_name": { "type": "string" }
                            },
                            "required": ["agent_name"]
                        }
                    }
                ]
            });
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Some(tools),
                error: None,
            }
        }
        "tools/call" => {
            let params = req.params.clone().unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

            let result = execute_tool(tool_name, arguments, storage);
            match result {
                Ok(val) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(val),
                    error: None,
                },
                Err(err_msg) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: err_msg,
                    }),
                },
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method '{}' not found", req.method),
            }),
        },
    }
}

fn execute_tool(name: &str, args: Value, storage: Arc<StorageEngine>) -> Result<Value, String> {
    match name {
        "memory_recall" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let tier = args.get("tier").and_then(|v| v.as_str()).unwrap_or("all");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            let hits = storage
                .hybrid_search(query, None, Some(tier), limit)
                .map_err(|e| e.to_string())?;

            Ok(serde_json::json!({ "results": hits }))
        }
        "memory_store" => {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled Memory");
            let tier_raw = args
                .get("tier")
                .and_then(|v| v.as_str())
                .unwrap_or("semantic");
            let scope = args
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("global");
            let tags: Vec<String> = args
                .get("tags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let tier = match tier_raw {
                "working" => MemoryTier::Working,
                "procedural" => MemoryTier::Procedural,
                _ => MemoryTier::Semantic,
            };

            let record = MemoryRecord {
                id: Uuid::new_v4(),
                tier,
                scope: scope.to_string(),
                title: title.to_string(),
                content: content.to_string(),
                tags,
                provenance: Provenance {
                    author_agent: "mcp-client".to_string(),
                    machine_id: "local".to_string(),
                    session_id: None,
                    timestamp: chrono::Utc::now(),
                },
                importance: 0.5,
                ttl_seconds: None,
                vector: None,
            };

            storage.store_memory(&record).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "status": "stored", "id": record.id }))
        }
        "memory_handoff" => {
            let target_agent = args
                .get("target_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let task_summary = args
                .get("task_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let next_steps: Vec<String> = args
                .get("next_steps")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let associated_files: Vec<String> = args
                .get("associated_files")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let handoff = InterAgentHandoff {
                handoff_id: Uuid::new_v4(),
                from_agent: "mcp-caller".to_string(),
                target_agent: target_agent.to_string(),
                project_scope: "default".to_string(),
                status: HandoffStatus::Pending,
                task_summary: task_summary.to_string(),
                next_steps,
                associated_files,
                created_at: chrono::Utc::now(),
                lease_expires_at: None,
                retry_count: 0,
                consumed: false,
            };

            storage.store_handoff(&handoff).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "status": "handoff_created", "handoff_id": handoff.handoff_id }))
        }
        "memory_consume_handoff" => {
            let agent_name = args
                .get("agent_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let handoffs = storage
                .get_pending_handoffs(agent_name)
                .map_err(|e| e.to_string())?;

            for h in &handooffs {
                let _ = storage.consume_handoff(&h.handoff_id);
            }

            Ok(serde_json::json!({ "consumed_handoffs": handoffs }))
        }
        _ => Err(format!("Unknown tool: {}", name)),
    }
}
