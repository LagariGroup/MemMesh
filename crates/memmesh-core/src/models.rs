use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Memory tier classifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryTier {
    Working,
    Episodic,
    Semantic,
    Procedural,
}

/// Provenance metadata tracking author, machine, and timestamp
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Provenance{
    pub author_agent:String,
    pub machine_id:String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub session_id: Option<String>,
    pub timestamp:DateTime<Utc>,
}

/// Core Memory Record schema definition
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct MemoryRecord{
    pub id:Uuid,
    pub tier:MemoryTier,
    pub scope: String,
    pub title:String,
    pub content:String,
    pub tags:Vec<String>,
    pub provenance:Provenance,
    pub importance:f64,
    #[serde(skip_serializing_if="Option::is_none")]
    pub ttl_seconds:Option<i64>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub vector:Option<Vec<f32>>,
}

/// Task lifecycle status for multi-agent handoffs
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="lowercase")]
pub enum HandoffStatus{
    Pending,
    Claimed,
    Completed,
    Abandoned,
}

/// Inter-Agent Handoff schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterAgentHandoff{
    pub handoff_id: Uuid,
    pub from_agent:String,
    pub target_agent:String,
    pub project_scope:String,
    pub status:HandoffStatus,
    pub task_summary:String,
    pub next_steps: Vec<String>,
    pub associated_files: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub retry_count:u32,
    pub consumed:bool,
}

/// Reciprocal Rank Fusion search result item
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct SearchResultItem {
    pub id: Uuid,
    pub tier:MemoryTier,
    pub title:String,
    pub content:String,
    pub score:f64,
    pub tags:Vec<String>,
    pub provenance:Provenance,
}