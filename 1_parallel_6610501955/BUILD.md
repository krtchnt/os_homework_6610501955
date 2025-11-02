# Build Instructions

1. Ensure you are inside `1_parallel_6610501955/`.
2. Run `make` to build the `parallel` binary. The default makefile uses `g++` with OpenMP enabled.
   - Override `CXX` if you need a different compiler: `make CXX=clang++`.

# Running Benchmarks

The binary accepts several CLI flags to explore different workloads:

```bash
./parallel \
  --numbers 600851475143,32416190071 \
  --threads 1-8 \
  --repeats 5 \
  --schedule dynamic \
  --chunk 32 \
  --output ../data/parallel_results.csv
```

- `--numbers` accepts a comma-separated list of integers ≥ 2.
- `--threads` runs across a range (inclusive) of thread counts.
- `--repeats` repeats each experiment to smooth variance.
- `--schedule` picks the OpenMP schedule (`static`, `dynamic`, `guided`, or `auto`).
- `--chunk` controls the chunk size used by OpenMP. Omit or set to `0` for library defaults.
- `--output` writes a CSV summary; omit to only log human-readable summaries.
- `--verbose` sends CSV-formatted rows to stdout when no `--output` file is given.

Results are suitable for further analysis with the scripts in `analysis/` (see README for details).
