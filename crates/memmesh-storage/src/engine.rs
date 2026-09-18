use chrono::{DateTime, Utc};
use memmesh_core::{
    CoreError, HandoffStatus, InterAgentHandoff, MemoryRecord, MemoryTier, Provenance,
    SearchResultItem,
};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct StorageEngine {
    conn: Arc<Mutex<Connection>>,
    vault_path: Option<String>,
}

impl StorageEngine {
    /// Initialize SQLite connection, apply WAL mode and migrations[cite: 2]
    pub fn new<P: AsRef<Path>>(db_path: P, vault_path: Option<String>) -> Result<Self, CoreError> {
        let conn = Connection::open(db_path)
            .map_err(|e| CoreError::Storage(format!("SQLite open error: {}", e)))?;

        // SQLite PRAGMA configuration per specifications[cite: 2]
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            "#,
        )
        .map_err(|e| CoreError::Storage(format!("PRAGMA init error: {}", e)))?;

        let engine = Self {
            conn: Arc::new(Mutex::new(conn)),
            vault_path,
        };

        engine.init_tables()?;
        Ok(engine)
    }

    /// In-memory database initialization for tests[cite: 2]
    pub fn new_in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| CoreError::Storage(format!("In-memory open error: {}", e)))?;
        let engine = Self {
            conn: Arc::new(Mutex::new(conn)),
            vault_path: None,
        };
        engine.init_tables()?;
        Ok(engine)
    }

    fn init_tables(&self) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                tier TEXT NOT NULL,
                scope TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT NOT NULL,
                author_agent TEXT NOT NULL,
                machine_id TEXT NOT NULL,
                session_id TEXT,
                created_at TEXT NOT NULL,
                importance REAL NOT NULL,
                ttl_seconds INTEGER,
                vector_json TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                title,
                content,
                tags,
                tokenize='porter'
            );

            CREATE TABLE IF NOT EXISTS handoffs (
                handoff_id TEXT PRIMARY KEY,
                from_agent TEXT NOT NULL,
                target_agent TEXT NOT NULL,
                project_scope TEXT NOT NULL,
                status TEXT NOT NULL,
                task_summary TEXT NOT NULL,
                next_steps TEXT NOT NULL,
                associated_files TEXT NOT NULL,
                created_at TEXT NOT NULL,
                lease_expires_at TEXT,
                retry_count INTEGER NOT NULL,
                consumed INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|e| CoreError::Storage(format!("Table init error: {}", e)))?;

        Ok(())
    }

    /// Insert or update a memory record[cite: 2]
    pub fn store_memory(&self, record: &MemoryRecord) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();

        let tags_json = serde_json::to_string(&record.tags)?;
        let vector_json = record
            .vector
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()?;
        let tier_str = serde_json::to_string(&record.tier)?.replace('\"', "");

        conn.execute(
            r#"
            INSERT OR REPLACE INTO memories (
                id, tier, scope, title, content, tags, author_agent,
                machine_id, session_id, created_at, importance, ttl_seconds, vector_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                record.id.to_string(),
                tier_str,
                record.scope,
                record.title,
                record.content,
                tags_json,
                record.provenance.author_agent,
                record.provenance.machine_id,
                record.provenance.session_id,
                record.provenance.timestamp.to_rfc3339(),
                record.importance,
                record.ttl_seconds,
                vector_json,
            ],
        )
        .map_err(|e| CoreError::Storage(format!("Insert memory error: {}", e)))?;

        // Synchronize FTS5 Index[cite: 2]
        conn.execute(
            "DELETE FROM memories_fts WHERE id = ?1",
            params![record.id.to_string()],
        )
        .ok();

        conn.execute(
            "INSERT INTO memories_fts (id, title, content, tags) VALUES (?1, ?2, ?3, ?4)",
            params![
                record.id.to_string(),
                record.title,
                record.content,
                tags_json
            ],
        )
        .map_err(|e| CoreError::Storage(format!("Insert FTS error: {}", e)))?;

        // Mirror to markdown vault asynchronously if configured[cite: 2]
        if let Some(vault) = &self.vault_path {
            let vault_dir = std::path::Path::new(vault);
            if !vault_dir.exists() {
                let _ = std::fs::create_dir_all(vault_dir);
            }
            let file_name = format!(
                "{}_{}.md",
                record.title.replace([' ', '/', '\\'], "_"),
                record.id
            );
            let md_path = vault_dir.join(file_name);
            let md_body = format!(
                "---\nid: {}\ntier: {:?}\nscope: {}\ntags: {:?}\nauthor: {}\ndate: {}\n---\n\n# {}\n\n{}",
                record.id, record.tier, record.scope, record.tags,
                record.provenance.author_agent, record.provenance.timestamp,
                record.title, record.content
            );
            let _ = std::fs::write(md_path, md_body);
        }

        Ok(())
    }

    /// Retrieve a single memory by ID[cite: 2]
    pub fn get_memory(&self, id: &Uuid) -> Result<Option<MemoryRecord>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, tier, scope, title, content, tags, author_agent,
                       machine_id, session_id, created_at, importance, ttl_seconds, vector_json
                FROM memories WHERE id = ?1
                "#,
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        if let Some(row) = rows.next().map_err(|e| CoreError::Storage(e.to_string()))? {
            let tags_raw: String = row.get(5).unwrap_or_default();
            let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();
            let created_at_raw: String = row.get(9).unwrap();
            let created_at = DateTime::parse_from_rfc3339(&created_at_raw)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let vector_json: Option<String> = row.get(12).unwrap_or(None);
            let vector: Option<Vec<f32>> = vector_json.and_then(|v| serde_json::from_str(&v).ok());

            let tier_raw: String = row.get(1).unwrap();
            let tier: MemoryTier = match tier_raw.to_lowercase().as_str() {
                "working" => MemoryTier::Working,
                "episodic" => MemoryTier::Episodic,
                "procedural" => MemoryTier::Procedural,
                _ => MemoryTier::Semantic,
            };

            return Ok(Some(MemoryRecord {
                id: *id,
                tier,
                scope: row.get(2).unwrap(),
                title: row.get(3).unwrap(),
                content: row.get(4).unwrap(),
                tags,
                provenance: Provenance {
                    author_agent: row.get(6).unwrap(),
                    machine_id: row.get(7).unwrap(),
                    session_id: row.get(8).unwrap_or(None),
                    timestamp: created_at,
                },
                importance: row.get(10).unwrap(),
                ttl_seconds: row.get(11).unwrap_or(None),
                vector,
            }));
        }

        Ok(None)
    }

    /// Delete memory by ID[cite: 2]
    pub fn delete_memory(&self, id: &Uuid) -> Result<bool, CoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM memories WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let _ = conn.execute(
            "DELETE FROM memories_fts WHERE id = ?1",
            params![id.to_string()],
        );
        Ok(affected > 0)
    }

    /// Hybrid search using Reciprocal Rank Fusion: RRF = 1/(60 + Rank_Vector) + 1/(60 + Rank_BM25)[cite: 2]
    pub fn hybrid_search(
        &self,
        query: &str,
        query_vector: Option<&[f32]>,
        tier: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResultItem>, CoreError> {
        let conn = self.conn.lock().unwrap();

        // 1. BM25 Text Search via FTS5[cite: 2]
        let mut bm25_ranks: HashMap<String, usize> = HashMap::new();
        let sanitized_query = query.replace('"', "").replace('\'', "");
        if !sanitized_query.trim().is_empty() {
            let mut fts_stmt = conn
                .prepare(
                    r#"
                    SELECT id FROM memories_fts
                    WHERE memories_fts MATCH ?1
                    ORDER BY rank LIMIT 50
                    "#,
                )
                .map_err(|e| CoreError::Storage(e.to_string()))?;

            let fts_rows = fts_stmt
                .query_map(params![sanitized_query], |row| row.get::<_, String>(0))
                .map_err(|e| CoreError::Storage(e.to_string()))?;

            for (idx, id_res) in fts_rows.enumerate() {
                if let Ok(id) = id_res {
                    bm25_ranks.insert(id, idx + 1);
                }
            }
        }

        // 2. Dense Vector Cosine Similarity Search[cite: 2]
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        if let Some(target_vec) = query_vector {
            let mut all_vec_stmt = conn
                .prepare("SELECT id, vector_json FROM memories WHERE vector_json IS NOT NULL")
                .map_err(|e| CoreError::Storage(e.to_string()))?;

            let mut scored: Vec<(String, f32)> = Vec::new();
            let rows = all_vec_stmt
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    let v_json: String = row.get(1)?;
                    Ok((id, v_json))
                })
                .map_err(|e| CoreError::Storage(e.to_string()))?;

            for item in rows.flatten() {
                if let Ok(cand_vec) = serde_json::from_str::<Vec<f32>>(&item.1) {
                    let sim = cosine_similarity(target_vec, &cand_vec);
                    scored.push((item.0, sim));
                }
            }

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (idx, (id, _)) in scored.into_iter().take(50).enumerate() {
                vector_ranks.insert(id, idx + 1);
            }
        }

        // 3. Compute RRF Score: 1 / (60 + Rank)[cite: 2]
        let mut all_candidate_ids: Vec<String> = bm25_ranks.keys().cloned().collect();
        for k in vector_ranks.keys() {
            if !all_candidate_ids.contains(k) {
                all_candidate_ids.push(k.clone());
            }
        }

        // If both searches yielded no matches, fallback to recent entries
        if all_candidate_ids.is_empty() {
            let mut recent_stmt = conn
                .prepare("SELECT id FROM memories ORDER BY created_at DESC LIMIT ?1")
                .map_err(|e| CoreError::Storage(e.to_string()))?;
            let recent_rows = recent_stmt
                .query_map(params![limit as i64], |row| row.get::<_, String>(0))
                .map_err(|e| CoreError::Storage(e.to_string()))?;
            for item in recent_rows.flatten() {
                all_candidate_ids.push(item);
            }
        }

        let mut rrf_scored: Vec<(String, f64)> = Vec::new();
        for id in &all_candidate_ids {
            let mut score = 0.0;
            if let Some(r_bm25) = bm25_ranks.get(id) {
                score += 1.0 / (60.0 + *r_bm25 as f64);
            }
            if let Some(r_vec) = vector_ranks.get(id) {
                score += 1.0 / (60.0 + *r_vec as f64);
            }
            if score == 0.0 {
                score = 0.01;
            }
            rrf_scored.push((id.clone(), score));
        }

        rrf_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 4. Hydrate Result Items[cite: 2]
        let mut results = Vec::new();
        for (id_str, score) in rrf_scored.into_iter().take(limit) {
            let uuid_val = match Uuid::parse_str(&id_str) {
                Ok(u) => u,
                Err(_) => continue,
            };

            let mut query_sql = "SELECT tier, title, content, tags, author_agent, machine_id, session_id, created_at FROM memories WHERE id = ?1".to_string();
            if let Some(t) = tier {
                if t != "all" {
                    query_sql.push_str(&format!(" AND tier = '{}'", t));
                }
            }

            let mut stmt = conn
                .prepare(&query_sql)
                .map_err(|e| CoreError::Storage(e.to_string()))?;
            let mut rows = stmt
                .query(params![id_str])
                .map_err(|e| CoreError::Storage(e.to_string()))?;

            if let Some(row) = rows.next().map_err(|e| CoreError::Storage(e.to_string()))? {
                let tier_raw: String = row.get(0).unwrap();
                let tier_val: MemoryTier = match tier_raw.to_lowercase().as_str() {
                    "working" => MemoryTier::Working,
                    "episodic" => MemoryTier::Episodic,
                    "procedural" => MemoryTier::Procedural,
                    _ => MemoryTier::Semantic,
                };
                let tags_raw: String = row.get(3).unwrap_or_default();
                let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();
                let created_at_raw: String = row.get(7).unwrap();
                let created_at = DateTime::parse_from_rfc3339(&created_at_raw)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                results.push(SearchResultItem {
                    id: uuid_val,
                    tier: tier_val,
                    title: row.get(1).unwrap(),
                    content: row.get(2).unwrap(),
                    score,
                    tags,
                    provenance: Provenance {
                        author_agent: row.get(4).unwrap(),
                        machine_id: row.get(5).unwrap(),
                        session_id: row.get(6).unwrap_or(None),
                        timestamp: created_at,
                    },
                });
            }
        }

        Ok(results)
    }

    /// Store a multi-agent handoff ticket[cite: 2]
    pub fn store_handoff(&self, handoff: &InterAgentHandoff) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let steps_json = serde_json::to_string(&handoff.next_steps)?;
        let files_json = serde_json::to_string(&handoff.associated_files)?;
        let status_str = format!("{:?}", handoff.status).to_lowercase();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO handoffs (
                handoff_id, from_agent, target_agent, project_scope, status,
                task_summary, next_steps, associated_files, created_at,
                lease_expires_at, retry_count, consumed
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                handoff.handoff_id.to_string(),
                handoff.from_agent,
                handoff.target_agent,
                handoff.project_scope,
                status_str,
                handoff.task_summary,
                steps_json,
                files_json,
                handoff.created_at.to_rfc3339(),
                handoff.lease_expires_at.map(|d| d.to_rfc3339()),
                handoff.retry_count,
                if handoff.consumed { 1 } else { 0 },
            ],
        )
        .map_err(|e| CoreError::Storage(format!("Handoff store error: {}", e)))?;

        Ok(())
    }

    /// Retrieve pending handoffs for a specific agent[cite: 2]
    pub fn get_pending_handoffs(
        &self,
        target_agent: &str,
    ) -> Result<Vec<InterAgentHandoff>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                r#"
                SELECT handoff_id, from_agent, target_agent, project_scope, status,
                       task_summary, next_steps, associated_files, created_at,
                       lease_expires_at, retry_count, consumed
                FROM handoffs
                WHERE (target_agent = ?1 OR target_agent = 'all') AND consumed = 0
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(params![target_agent], |row| {
                let id_raw: String = row.get(0)?;
                let next_steps_raw: String = row.get(6)?;
                let files_raw: String = row.get(7)?;
                let created_raw: String = row.get(8)?;
                let lease_raw: Option<String> = row.get(9)?;
                let consumed_int: i32 = row.get(11)?;

                Ok(InterAgentHandoff {
                    handoff_id: Uuid::parse_str(&id_raw).unwrap(),
                    from_agent: row.get(1)?,
                    target_agent: row.get(2)?,
                    project_scope: row.get(3)?,
                    status: HandoffStatus::Pending,
                    task_summary: row.get(5)?,
                    next_steps: serde_json::from_str(&next_steps_raw).unwrap_or_default(),
                    associated_files: serde_json::from_str(&files_raw).unwrap_or_default(),
                    created_at: DateTime::parse_from_rfc3339(&created_raw)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    lease_expires_at: lease_raw.and_then(|r| {
                        DateTime::parse_from_rfc3339(&r)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                    retry_count: row.get(10)?,
                    consumed: consumed_int == 1,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        let mut list = Vec::new();
        for item in rows.flatten() {
            list.push(item);
        }
        Ok(list)
    }

    /// Mark handoff consumed by the target agent[cite: 2]
    pub fn consume_handoff(&self, handoff_id: &Uuid) -> Result<bool, CoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE handoffs SET consumed = 1, status = 'completed' WHERE handoff_id = ?1",
                params![handoff_id.to_string()],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        Ok(affected > 0)
    }

    /// System statistics[cite: 2]
    pub fn get_stats(&self) -> Result<(usize, usize), CoreError> {
        let conn = self.conn.lock().unwrap();
        let count_records: i64 = conn
            .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
            .unwrap_or(0);
        let count_vectors: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories WHERE vector_json IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok((count_records as usize, count_vectors as usize))
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}
