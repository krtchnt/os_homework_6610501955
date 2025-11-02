# Build Instructions

```
rustc --edition=2021 -C opt-level=3 cow.rs -o cow
```

Run the executable directly:

```
./cow --sizes 64,96,128 --output ../data/cow_results.csv
```

- `--sizes` accepts a comma-separated list of allocation sizes in megabytes (must be ≥ 16).
- `--output` writes a CSV summarising RSS / private-dirty figures captured from `/proc`.
- Omit `--output` to only print the measurements to stdout.

The program demonstrates copy-on-write by measuring RSS before/after forcing the child process to mutate the allocated pages.
