use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Event record for Continuum journal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuumEvent {
    pub event_id: String,
    pub kind: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectral_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    /// Merkle hash of this event (SHA256)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_hash: Option<String>,
    /// Parent event hash for chain validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_hash: Option<String>,
}

impl ContinuumEvent {
    /// Compute Merkle hash for this event
    pub fn compute_hash(&mut self) {
        let mut hasher = Sha256::new();

        // Hash all fields except merkle_hash itself
        hasher.update(self.event_id.as_bytes());
        hasher.update(self.kind.as_bytes());
        hasher.update(self.timestamp.as_bytes());

        if let Some(persona) = &self.persona {
            hasher.update(persona.as_bytes());
        }

        // Hash payload JSON
        if let Ok(payload_json) = serde_json::to_string(&self.payload) {
            hasher.update(payload_json.as_bytes());
        }

        if let Some(tag) = &self.spectral_tag {
            hasher.update(tag.as_bytes());
        }

        if let Some(parent) = &self.parent_hash {
            hasher.update(parent.as_bytes());
        }

        let result = hasher.finalize();
        self.merkle_hash = Some(hex::encode(result));
    }

    /// Verify this event's hash matches its content
    pub fn verify_hash(&self) -> bool {
        let mut temp = self.clone();
        let original_hash = temp.merkle_hash.clone();
        temp.merkle_hash = None;
        temp.compute_hash();
        temp.merkle_hash == original_hash
    }
}

/// Snapshot of Continuum state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuumSnapshot {
    pub snapshot_id: String,
    pub timestamp: String,
    /// Event count at snapshot time
    pub event_count: usize,
    /// Merkle root hash of all events up to this point
    pub merkle_root: String,
    /// Last event ID included in snapshot
    pub last_event_id: String,
    /// Compressed state data (zstd)
    #[serde(with = "serde_bytes")]
    pub compressed_data: Vec<u8>,
    /// Metadata
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl ContinuumSnapshot {
    /// Create snapshot from events
    pub fn from_events(
        events: &[ContinuumEvent],
        snapshot_id: String,
        timestamp: String,
    ) -> Result<Self> {
        if events.is_empty() {
            anyhow::bail!("cannot create snapshot from zero events");
        }

        // Compute Merkle root
        let merkle_root = Self::compute_merkle_root(events);

        let last_event_id = events
            .last()
            .ok_or_else(|| anyhow::anyhow!("no last event"))?
            .event_id
            .clone();

        // Serialize events to JSON
        let json_data = serde_json::to_vec(events)
            .context("serializing events to JSON")?;

        // Compress with zstd
        let compressed_data = zstd::encode_all(&json_data[..], 3)
            .context("compressing snapshot with zstd")?;

        let compression_ratio = json_data.len() as f64 / compressed_data.len() as f64;

        info!(
            "Created snapshot {} with {} events, {:.1}x compression ({} → {} bytes)",
            snapshot_id,
            events.len(),
            compression_ratio,
            json_data.len(),
            compressed_data.len()
        );

        Ok(Self {
            snapshot_id,
            timestamp,
            event_count: events.len(),
            merkle_root,
            last_event_id,
            compressed_data,
            metadata: BTreeMap::new(),
        })
    }

    /// Restore events from snapshot
    pub fn restore_events(&self) -> Result<Vec<ContinuumEvent>> {
        // Decompress
        let decompressed = zstd::decode_all(&self.compressed_data[..])
            .context("decompressing snapshot")?;

        // Deserialize
        let events: Vec<ContinuumEvent> = serde_json::from_slice(&decompressed)
            .context("deserializing events from snapshot")?;

        // Verify event count matches
        if events.len() != self.event_count {
            anyhow::bail!(
                "snapshot event count mismatch: expected {}, got {}",
                self.event_count,
                events.len()
            );
        }

        // Verify Merkle root
        let computed_root = Self::compute_merkle_root(&events);
        if computed_root != self.merkle_root {
            anyhow::bail!(
                "snapshot Merkle root mismatch: expected {}, computed {}",
                self.merkle_root,
                computed_root
            );
        }

        Ok(events)
    }

    /// Compute Merkle root hash from event list
    fn compute_merkle_root(events: &[ContinuumEvent]) -> String {
        if events.is_empty() {
            return String::new();
        }

        // Simple Merkle tree: hash all event hashes together
        let mut hasher = Sha256::new();

        for event in events {
            if let Some(hash) = &event.merkle_hash {
                hasher.update(hash.as_bytes());
            } else {
                // Event doesn't have hash yet - compute it
                let mut temp = event.clone();
                temp.compute_hash();
                if let Some(hash) = temp.merkle_hash {
                    hasher.update(hash.as_bytes());
                }
            }
        }

        hex::encode(hasher.finalize())
    }

    /// Save snapshot to disk
    pub fn save(&self, snapshot_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(snapshot_dir)
            .context("creating snapshot directory")?;

        let filename = format!("{}.snapshot.json.zst", self.snapshot_id);
        let path = snapshot_dir.join(filename);

        let json = serde_json::to_vec(self)
            .context("serializing snapshot metadata")?;

        // Double-compress: snapshot already contains compressed data
        let mut file = std::fs::File::create(&path)
            .with_context(|| format!("creating snapshot file {}", path.display()))?;

        file.write_all(&json)
            .context("writing snapshot to disk")?;

        file.sync_all()
            .context("syncing snapshot to disk")?;

        info!("Saved snapshot {} to {}", self.snapshot_id, path.display());
        Ok(path)
    }

    /// Load snapshot from disk
    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read(path)
            .with_context(|| format!("reading snapshot from {}", path.display()))?;

        let snapshot: Self = serde_json::from_slice(&json)
            .context("deserializing snapshot metadata")?;

        debug!("Loaded snapshot {} from {}", snapshot.snapshot_id, path.display());
        Ok(snapshot)
    }
}

/// Continuum event store manager
pub struct ContinuumStore {
    journal_path: PathBuf,
    snapshot_dir: PathBuf,
    /// Events loaded in memory (for fast snapshot creation)
    events: Vec<ContinuumEvent>,
    /// Snapshot every N events
    snapshot_interval: usize,
}

impl ContinuumStore {
    pub fn new(journal_path: PathBuf, snapshot_dir: PathBuf) -> Self {
        Self {
            journal_path,
            snapshot_dir,
            events: Vec::new(),
            snapshot_interval: 100, // Snapshot every 100 events
        }
    }

    /// Load events from journal
    pub fn load_journal(&mut self) -> Result<usize> {
        if !self.journal_path.exists() {
            info!("Journal file does not exist yet: {}", self.journal_path.display());
            return Ok(0);
        }

        let contents = std::fs::read_to_string(&self.journal_path)
            .with_context(|| format!("reading journal {}", self.journal_path.display()))?;

        let mut loaded = 0;
        let mut last_hash: Option<String> = None;

        for (line_num, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let mut event: ContinuumEvent = serde_json::from_str(line)
                .with_context(|| format!("parsing journal line {}", line_num + 1))?;

            // Verify hash chain
            if let Some(prev_hash) = &last_hash {
                if event.parent_hash.as_ref() != Some(prev_hash) {
                    warn!(
                        "Hash chain broken at event {}: expected parent {}, got {:?}",
                        event.event_id, prev_hash, event.parent_hash
                    );
                }
            }

            // Verify event hash
            if !event.verify_hash() {
                warn!("Event {} has invalid hash, recomputing", event.event_id);
                event.compute_hash();
            }

            last_hash = event.merkle_hash.clone();
            self.events.push(event);
            loaded += 1;
        }

        info!("Loaded {} events from journal", loaded);
        Ok(loaded)
    }

    /// Create snapshot if interval reached
    pub fn maybe_snapshot(&mut self) -> Result<Option<PathBuf>> {
        if self.events.len() < self.snapshot_interval {
            return Ok(None);
        }

        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let snapshot = ContinuumSnapshot::from_events(
            &self.events,
            snapshot_id,
            timestamp,
        )?;

        let path = snapshot.save(&self.snapshot_dir)?;

        // Clear events after snapshot (they're persisted)
        self.events.clear();

        Ok(Some(path))
    }

    /// List available snapshots
    pub fn list_snapshots(&self) -> Result<Vec<PathBuf>> {
        if !self.snapshot_dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();

        for entry in std::fs::read_dir(&self.snapshot_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("zst") {
                snapshots.push(path);
            }
        }

        snapshots.sort();
        Ok(snapshots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_event(id: &str, parent_hash: Option<String>) -> ContinuumEvent {
        let mut event = ContinuumEvent {
            event_id: id.to_string(),
            kind: "test".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            persona: Some("core".to_string()),
            payload: serde_json::json!({"msg": "test"}),
            spectral_tag: Some("test::event".to_string()),
            bytes: Some(10),
            merkle_hash: None,
            parent_hash,
        };
        event.compute_hash();
        event
    }

    #[test]
    fn event_hash_computation() {
        let mut event = create_test_event("test1", None);
        assert!(event.merkle_hash.is_some());
        assert!(event.verify_hash());

        // Modify event
        event.payload = serde_json::json!({"modified": true});
        assert!(!event.verify_hash());

        // Recompute
        event.compute_hash();
        assert!(event.verify_hash());
    }

    #[test]
    fn hash_chain_linking() {
        let event1 = create_test_event("e1", None);
        let hash1 = event1.merkle_hash.clone().unwrap();

        let event2 = create_test_event("e2", Some(hash1.clone()));
        assert_eq!(event2.parent_hash.as_ref().unwrap(), &hash1);
    }

    #[test]
    fn snapshot_create_restore() {
        let events = vec![
            create_test_event("s1", None),
            create_test_event("s2", None),
            create_test_event("s3", None),
        ];

        let snapshot = ContinuumSnapshot::from_events(
            &events,
            "snap1".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();

        assert_eq!(snapshot.event_count, 3);
        assert!(!snapshot.merkle_root.is_empty());

        let restored = snapshot.restore_events().unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0].event_id, "s1");
    }

    #[test]
    fn snapshot_compression() {
        let mut events = Vec::new();
        for i in 0..100 {
            events.push(create_test_event(&format!("e{}", i), None));
        }

        let snapshot = ContinuumSnapshot::from_events(
            &events,
            "large".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();

        // Compression should be significant
        let json_size = serde_json::to_vec(&events).unwrap().len();
        let compressed_size = snapshot.compressed_data.len();

        assert!(compressed_size < json_size);
        println!(
            "Compression: {} → {} ({:.1}x)",
            json_size,
            compressed_size,
            json_size as f64 / compressed_size as f64
        );
    }

    #[test]
    fn snapshot_save_load() {
        let temp_dir = TempDir::new().unwrap();

        let events = vec![create_test_event("save1", None)];
        let snapshot = ContinuumSnapshot::from_events(
            &events,
            "persist".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();

        let path = snapshot.save(temp_dir.path()).unwrap();
        assert!(path.exists());

        let loaded = ContinuumSnapshot::load(&path).unwrap();
        assert_eq!(loaded.snapshot_id, snapshot.snapshot_id);
        assert_eq!(loaded.event_count, snapshot.event_count);
        assert_eq!(loaded.merkle_root, snapshot.merkle_root);
    }

    #[test]
    fn store_auto_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let journal_path = temp_dir.path().join("journal.log");
        let snapshot_dir = temp_dir.path().join("snapshots");

        let mut store = ContinuumStore::new(journal_path, snapshot_dir);
        store.snapshot_interval = 3;

        // Add 2 events - no snapshot
        store.events.push(create_test_event("a1", None));
        store.events.push(create_test_event("a2", None));
        assert!(store.maybe_snapshot().unwrap().is_none());

        // Add 3rd event - triggers snapshot
        store.events.push(create_test_event("a3", None));
        let snap_path = store.maybe_snapshot().unwrap();
        assert!(snap_path.is_some());
        assert!(snap_path.unwrap().exists());

        // Events cleared after snapshot
        assert_eq!(store.events.len(), 0);
    }
}
