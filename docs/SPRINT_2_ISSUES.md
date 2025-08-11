# Sprint 2: GitHub Issues Template

Copy and create these issues on GitHub for Sprint 2 tracking:

---

## Issue #1: Create storage crate with WAL implementation

**Title:** `[Sprint 2] Implement WAL storage crate with segmented log`

**Labels:** `enhancement`, `sprint-2`, `storage`

**Description:**
Create a new `storage/` crate implementing a Write-Ahead Log (WAL) with the following features:

### Requirements:
- [ ] Append-only segmented log (binary format)
- [ ] Segment rotation at configurable size (default: 128MB)
- [ ] CRC32 checksums for each entry
- [ ] Atomic writes with fsync
- [ ] Zero-copy reads where possible

### API Design:
```rust
pub trait WalEntry: Serialize + DeserializeOwned {
    fn timestamp(&self) -> Ts;
}

pub struct Wal {
    pub fn new(path: &Path, segment_size: usize) -> Result<Self>
    pub fn append<T: WalEntry>(&mut self, entry: &T) -> Result<()>
    pub fn stream<T: WalEntry>(&self, from_ts: Ts) -> Result<WalIterator<T>>
    pub fn compact(&mut self, before_ts: Ts) -> Result<()>
}
```

### Performance Targets:
- Write latency: < 10 µs (buffered)
- Throughput: > 1M events/sec
- Recovery time: < 1 sec for 1GB WAL

---

## Issue #2: Implement core event types for WAL

**Title:** `[Sprint 2] Define canonical event types for WAL persistence`

**Labels:** `enhancement`, `sprint-2`, `storage`

**Description:**
Define the core event types that will be persisted to WAL:

### Event Types:
```rust
#[derive(Serialize, Deserialize)]
pub enum WalEvent {
    Tick(TickEvent),
    Order(OrderEvent),
    Fill(FillEvent),
    Signal(SignalEvent),
    Risk(RiskEvent),
    System(SystemEvent),
}

pub struct TickEvent {
    ts: Ts,
    venue: String,
    symbol: Symbol,
    bid: Option<Px>,
    ask: Option<Px>,
    last: Option<Px>,
    volume: Option<Qty>,
}
```

### Requirements:
- [ ] All events must have timestamps
- [ ] Efficient serialization with bincode
- [ ] Support for schema evolution
- [ ] Unit tests for round-trip serialization

---

## Issue #3: Build deterministic replay engine

**Title:** `[Sprint 2] Implement deterministic replay from WAL`

**Labels:** `enhancement`, `sprint-2`, `simulation`

**Description:**
Create `sim/replay` module that reads WAL and re-emits events deterministically:

### Requirements:
- [ ] Read events from WAL in timestamp order
- [ ] Publish events to bus at original intervals (time scaling)
- [ ] Support fast-forward mode (no delays)
- [ ] Pause/resume/seek functionality
- [ ] Progress tracking and metrics

### API:
```rust
pub struct Replayer {
    pub fn new(wal: Wal, bus: Bus<WalEvent>) -> Self
    pub fn start(&mut self, from: Ts, to: Ts) -> Result<()>
    pub fn pause(&mut self)
    pub fn resume(&mut self)
    pub fn seek(&mut self, ts: Ts) -> Result<()>
    pub fn speed(&mut self, multiplier: f64)
}
```

### Success Criteria:
- Byte-identical replay of any session
- Support for 1000x speed replay
- Memory usage < 100MB for replay engine

---

## Issue #4: Add WAL benchmarks and stress tests

**Title:** `[Sprint 2] Performance benchmarks for WAL operations`

**Labels:** `testing`, `sprint-2`, `performance`

**Description:**
Create comprehensive benchmarks using criterion:

### Benchmarks:
- [ ] Sequential write throughput
- [ ] Random read latency
- [ ] Concurrent write contention
- [ ] Recovery time vs WAL size
- [ ] Compression ratios (if implemented)

### Stress Tests:
- [ ] Write 10M events, verify all readable
- [ ] Crash during write, verify recovery
- [ ] Fill disk, verify graceful handling
- [ ] Corrupt segment, verify detection

### Targets:
- Establish baseline metrics
- Document in `benches/README.md`
- Add to CI pipeline

---

## Issue #5: WAL monitoring and diagnostics

**Title:** `[Sprint 2] Add observability for WAL operations`

**Labels:** `enhancement`, `sprint-2`, `monitoring`

**Description:**
Add metrics and logging for WAL operations:

### Metrics:
- [ ] Write latency histogram
- [ ] Bytes written/read per second
- [ ] Segment count and sizes
- [ ] Compression ratio
- [ ] Error counts

### CLI Commands:
```bash
shrivenq wal info <path>      # Show WAL statistics
shrivenq wal verify <path>    # Verify checksums
shrivenq wal compact <path>   # Manual compaction
shrivenq wal export <path>    # Export to JSON
```

### Dashboard:
- Add WAL stats to Temple UI
- Real-time write throughput graph
- Disk usage visualization

---

## Sprint 2 Meta Issue

**Title:** `[Sprint 2] WAL & Replay Implementation`

**Labels:** `epic`, `sprint-2`

**Description:**
Parent issue for Sprint 2: Structured WAL that can be replayed deterministically

### Child Issues:
- #1 Storage crate with WAL
- #2 Core event types
- #3 Replay engine
- #4 Benchmarks and tests
- #5 Monitoring

### Definition of Done:
- [ ] All child issues completed
- [ ] 10K fake Tick events → replay → byte-identical
- [ ] Performance benchmarks passing
- [ ] Documentation updated
- [ ] Code passes strict-check.sh
- [ ] CI/CD green

### Timeline:
- Start: TBD
- Target: 1 week
- Review: End of sprint

---

*Copy these issues to GitHub and update issue numbers in cross-references*