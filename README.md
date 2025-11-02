# OS Homework 2025 – 6610501955

Student ID: `6610501955`

This repository contains three implementations plus the accompanying analysis for the Operating Systems report.

## Project Layout

- `1_parallel_6610501955/` – C++20 + OpenMP factorisation benchmark suite.
- `2_cow_6610501955/` – Rust program that demonstrates Copy-on-Write behaviour via RSS sampling.
- `3_deadlock_6610501955/` – Rust deadlock laboratory covering avoidance, detection, and resolution.
- `analysis/` – Helper script for producing aggregate tables and SVG plots from collected data.
- `data/` – CSV outputs from the experiments.
- `graphs/` – Generated SVG visualisations embedded in the Typst report.
- `report_6610501955.typ` – Main report source (compile with Typst to generate PDF).

## Prerequisites

- GNU Make (for the C++ project)
- `g++` (or `clang++` with OpenMP support)
- Rust compiler (`rustc`)
- Python 3.10+
- Typst (optional, for compiling the report)

## Build & Run

### 1. Parallel Factorisation (C++)

```bash
cd 1_parallel_6610501955
make CXX=g++
./parallel --numbers 600851475143,9999999967,899809363 --threads 1-8 --repeats 3 \
  --schedule dynamic --chunk 32 --output ../data/parallel_results.csv
```

Edit the CLI flags to explore different workloads or scheduling choices. The program can emit either human-readable logs or CSV rows (`--verbose`). A Makefile is provided; override `CXX` if required.

### 2. Copy-on-Write Demonstrator (Rust)

```bash
cd 2_cow_6610501955
rustc --edition=2021 -C opt-level=3 cow.rs -o cow
./cow --sizes 64,96,128 --output ../data/cow_results.csv
```

Flags:

- `--sizes` – comma-separated allocation sizes (MB) to probe.
- `--output` – optional CSV destination.

The program forks once per experiment, touches memory pages in the child, and logs both RSS and Private_Dirty metrics taken from `/proc`.

### 3. Deadlock Laboratory (Rust)

```bash
cd 3_deadlock_6610501955
rustc --edition=2021 -C opt-level=3 deadlock.rs -o deadlock
./deadlock --mode avoidance     # Banker's algorithm walkthrough
./deadlock --mode detection     # Simulated deadlock detection
./deadlock --mode resolution    # Deadlock detection + victim termination
```

The simulation uses three resource types and three worker threads. Deadlock avoidance leverages Banker's algorithm, while detection and resolution rely on a monitor thread that searches for cycles in a wait-for graph.

### Analysis Scripts & Plots

```bash
cd /home/fdl/Code/._/0/os_homework_6610501955
python analysis/generate_plots.py
```

The script reads the CSV logs, calculates Amdahl-style parallel fractions, and emits:

- `data/parallel_summary.csv`
- `data/cow_summary.csv`
- `graphs/parallel_time.svg`
- `graphs/parallel_speedup.svg`
- `graphs/cow_rss.svg`

These artefacts are referenced inside the Typst report.

### Report

Compile the report after regenerating data/plots:

```bash
typst compile report_6610501955.typ
```

This produces `report_6610501955.pdf` beside the source file.

## Safety Considerations

- The factorisation program is CPU-bound and runs entirely in user space; it does not modify system-wide settings.
- The Copy-on-Write demonstrator allocates at most 128 MB per run and cleans up on exit.
- The deadlock laboratory simulates resources with in-memory state and never acquires kernel-managed locks.

All experiments were executed and verified on Linux using the provided development environment.
