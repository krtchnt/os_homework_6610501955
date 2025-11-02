#!/usr/bin/env python3
import csv
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter, MaxNLocator

try:
    plt.style.use("seaborn-v0_8-darkgrid")
except OSError:
    plt.style.use("seaborn-darkgrid")

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "data"
GRAPH_DIR = ROOT / "graphs"

COLORS = [
    "#1f77b4",
    "#ff7f0e",
    "#2ca02c",
    "#d62728",
    "#9467bd",
    "#8c564b",
]


def load_parallel_results():
    results = defaultdict(lambda: defaultdict(list))
    path = DATA_DIR / "parallel_results.csv"
    if not path.exists():
        raise FileNotFoundError(path)
    with path.open() as csvfile:
        reader = csv.DictReader(csvfile)
        for row in reader:
            number = int(row["number"])
            threads = int(row["threads"])
            time_ms = float(row["time_ms"])
            results[number][threads].append(time_ms)
    return results


def summarise_parallel(results):
    summary_rows = []
    for number, thread_map in results.items():
        baseline = sum(thread_map[1]) / len(thread_map[1])
        for threads, samples in sorted(thread_map.items()):
            avg_time = sum(samples) / len(samples)
            speedup = baseline / avg_time if avg_time > 0 else 0.0
            parallel_fraction = None
            if threads > 1 and speedup > 1.0:
                numerator = 1 - 1 / speedup
                denominator = 1 - 1 / threads
                if denominator != 0:
                    parallel_fraction = max(0.0, min(1.0, numerator / denominator))
            summary_rows.append(
                {
                    "number": number,
                    "threads": threads,
                    "avg_time_ms": avg_time,
                    "speedup": speedup,
                    "parallel_fraction": parallel_fraction if parallel_fraction else 0.0,
                }
            )
    summary_path = DATA_DIR / "parallel_summary.csv"
    with summary_path.open("w", newline="") as csvfile:
        fieldnames = ["number", "threads", "avg_time_ms", "speedup", "parallel_fraction"]
        writer = csv.DictWriter(csvfile, fieldnames=fieldnames)
        writer.writeheader()
        for row in summary_rows:
            writer.writerow(row)
    return summary_rows


def save_figure(fig, output_path: Path):
    output_path.parent.mkdir(parents=True, exist_ok=True)
    format_hint = output_path.suffix.lower().lstrip(".")
    if not format_hint:
        format_hint = "png"
    fig.savefig(output_path, format=format_hint, bbox_inches="tight", dpi=150)
    plt.close(fig)


def plot_parallel_metric(
    threads, numbers, by_number, metric_key, title, y_label, output_path
):
    fig, ax = plt.subplots(figsize=(10, 6))
    for idx, number in enumerate(numbers):
        values = [by_number[number][thread][metric_key] for thread in threads]
        color = COLORS[idx % len(COLORS)]
        ax.plot(
            threads,
            values,
            marker="o",
            linewidth=2,
            markersize=6,
            label=str(number),
            color=color,
        )
    ax.set_title(title)
    ax.set_xlabel("Threads")
    ax.set_ylabel(y_label)
    ax.xaxis.set_major_locator(MaxNLocator(integer=True))
    ax.set_xticks(list(threads))
    ax.grid(True, axis="both", linestyle="--", linewidth=0.8, alpha=0.5)
    ax.set_ylim(bottom=0)
    ax.legend(title="Number", frameon=False)
    if metric_key == "speedup":
        ax.axhline(1.0, color="#555555", linestyle=":", linewidth=1.2, alpha=0.8)
    fig.tight_layout()
    save_figure(fig, output_path)


def generate_parallel_plots(summary_rows):
    by_number = defaultdict(dict)
    for row in summary_rows:
        by_number[row["number"]][row["threads"]] = row

    numbers = sorted(by_number.keys())
    threads = sorted({row["threads"] for row in summary_rows})

    time_path = GRAPH_DIR / "parallel_time.svg"
    plot_parallel_metric(
        threads,
        numbers,
        by_number,
        "avg_time_ms",
        "Factorisation Time vs Threads",
        "Average time (ms)",
        time_path,
    )

    speedup_path = GRAPH_DIR / "parallel_speedup.svg"
    plot_parallel_metric(
        threads,
        numbers,
        by_number,
        "speedup",
        "Measured Speedup",
        "Speedup (T1/Tn)",
        speedup_path,
    )


def load_cow_results():
    path = DATA_DIR / "cow_results.csv"
    if not path.exists():
        raise FileNotFoundError(path)
    summary = []
    with path.open() as csvfile:
        reader = csv.DictReader(csvfile)
        for row in reader:
            summary.append(
                {
                    "size_mb": int(row["size_mb"]),
                    "parent_rss_kb": int(row["parent_rss_kb"]),
                    "child_post_fork_rss_kb": int(row["child_post_fork_rss_kb"]),
                    "child_post_write_rss_kb": int(row["child_post_write_rss_kb"]),
                    "child_post_write_private_dirty_kb": int(
                        row["child_post_write_private_dirty_kb"]
                    ),
                    "touch_ms": float(row["touch_ms"]),
                }
            )
    summary_path = DATA_DIR / "cow_summary.csv"
    with summary_path.open("w", newline="") as csvfile:
        fieldnames = [
            "size_mb",
            "parent_rss_kb",
            "child_post_fork_rss_kb",
            "child_post_write_rss_kb",
            "child_post_write_private_dirty_kb",
            "touch_ms",
            "rss_delta_kb",
        ]
        writer = csv.DictWriter(csvfile, fieldnames=fieldnames)
        writer.writeheader()
        for row in summary:
            delta = row["child_post_write_rss_kb"] - row["child_post_fork_rss_kb"]
            writer.writerow({**row, "rss_delta_kb": delta})
    return summary


def generate_cow_plot(summary):
    labels = [f'{entry["size_mb"]} MB' for entry in summary]
    stages = [
        ("Parent RSS", "parent_rss_kb"),
        ("Child after fork", "child_post_fork_rss_kb"),
        ("Child after writes", "child_post_write_rss_kb"),
    ]
    positions = list(range(len(labels)))
    bar_width = 0.22
    max_value = max(max(entry[key] for entry in summary) for _, key in stages)
    text_offset = max(10, max_value * 0.012)

    fig, ax = plt.subplots(figsize=(10, 6))

    for idx, (stage_label, key) in enumerate(stages):
        offsets = [
            pos + (idx - (len(stages) - 1) / 2) * bar_width for pos in positions
        ]
        values = [entry[key] for entry in summary]
        color = COLORS[idx % len(COLORS)]
        bars = ax.bar(
            offsets,
            values,
            width=bar_width,
            label=stage_label,
            color=color,
            edgecolor="#222222",
            linewidth=0.7,
        )
        for bar, value in zip(bars, values):
            ax.text(
                bar.get_x() + bar.get_width() / 2,
                value + text_offset,
                f"{value:.0f}",
                ha="center",
                va="bottom",
                fontsize=10,
            )

    ax.set_xticks(positions)
    ax.set_xticklabels(labels)
    ax.set_ylabel("RSS (kB)")
    ax.set_title("Copy-on-Write RSS Observations")
    ax.set_ylim(bottom=0)
    ax.yaxis.set_major_formatter(FuncFormatter(lambda value, _: f"{int(value):,}"))
    ax.legend(frameon=False, title="Measurement")
    ax.grid(True, axis="y", linestyle="--", linewidth=0.8, alpha=0.5)

    fig.tight_layout()
    rss_path = GRAPH_DIR / "cow_rss.svg"
    save_figure(fig, rss_path)


def main():
    GRAPH_DIR.mkdir(parents=True, exist_ok=True)
    DATA_DIR.mkdir(parents=True, exist_ok=True)

    parallel_results = load_parallel_results()
    parallel_summary = summarise_parallel(parallel_results)
    generate_parallel_plots(parallel_summary)

    cow_summary = load_cow_results()
    generate_cow_plot(cow_summary)
    print("Generated plots in", GRAPH_DIR)


if __name__ == "__main__":
    main()
