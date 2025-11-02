# Build & Run

```bash
rustc --edition=2021 -C opt-level=3 deadlock.rs -o deadlock
```

Example executions:

```bash
# Banker's algorithm safe-state walkthrough
./deadlock --mode avoidance

# Deadlock detection (threads become stuck, program halts them)
./deadlock --mode detection

# Deadlock resolution (monitor terminates a victim and allows recovery)
./deadlock --mode resolution
```

The simulation only manipulates in-memory data structures—no real OS resources are consumed.
