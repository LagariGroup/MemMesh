use memmesh_core::MemoryRecord;
use memmesh_server::mcp::{handle_mcp_request, JsonRpcRequest};
use memmesh_storage::StorageEngine;
use std::sync::Arc;

#[tokio::test]
async fn test_mcp_protocol_compliance() {
    let storage = Arc::new(StorageEngine::new_in_memory().unwrap());

    // 1. Verify tools/list responds per MCP standard[cite: 2]
    let list_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "tools/list".to_string(),
        params: None,
    };
    let list_resp = handle_mcp_request(list_req, storage.clone());
    assert!(list_resp.result.is_some());
    assert!(list_resp.error.is_none());

    // 2. Store a memory via tool call[cite: 2]
    let store_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(2)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "memory_store",
            "arguments": {
                "title": "Fastify Architecture Rule",
                "content": "Plugins must register using fastify-plugin[cite: 2].",
                "tier": "semantic",
                "tags": ["fastify", "architecture"]
            }
        })),
    };
    let store_resp = handle_mcp_request(store_req, storage.clone());
    assert!(store_resp.result.is_some());

    // 3. Search for the stored item[cite: 2]
    let recall_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(3)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "memory_recall",
            "arguments": {
                "query": "fastify plugin rule",
                "tier": "all"
            }
        })),
    };
    let recall_resp = handle_mcp_request(recall_req, storage.clone());
    assert!(recall_resp.result.is_some());
    let res_val = recall_resp.result.unwrap();
    let hits = res_val.get("results").unwrap().as_array().unwrap();
    assert!(!hits.is_empty());
}