#!/usr/bin/env python3
import csv
import math
from collections import defaultdict
from pathlib import Path

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


def svg_header(width, height, title=None):
    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}">',
    ]
    if title:
        lines.append(f"<title>{title}</title>")
    return lines


def svg_footer():
    return ["</svg>"]


def format_number(value):
    if value >= 1000:
        return f"{value/1000:.1f}k"
    return f"{value:.0f}"


def render_line_chart(series, x_values, output_path, title, x_label, y_label):
    width, height = 720, 480
    margin = 60
    max_y = max(max(values) for values in series.values()) * 1.1
    min_y = min(min(values) for values in series.values())
    if math.isclose(max_y, min_y):
        max_y += 1.0
    lines = svg_header(width, height, title)
    plot_width = width - 2 * margin
    plot_height = height - 2 * margin

    def scale_x(x):
        return margin + (x - x_values[0]) / (x_values[-1] - x_values[0]) * plot_width

    def scale_y(y):
        return margin + plot_height - (y - min_y) / (max_y - min_y) * plot_height

    # Axes
    lines.append(f'<line x1="{margin}" y1="{margin + plot_height}" x2="{margin + plot_width}" '
                 f'y2="{margin + plot_height}" stroke="#333" stroke-width="1.5"/>')
    lines.append(f'<line x1="{margin}" y1="{margin}" x2="{margin}" '
                 f'y2="{margin + plot_height}" stroke="#333" stroke-width="1.5"/>')

    # Gridlines and labels
    y_ticks = 5
    for i in range(y_ticks + 1):
        value = min_y + i * (max_y - min_y) / y_ticks
        y = scale_y(value)
        lines.append(
            f'<line x1="{margin}" y1="{y}" x2="{margin + plot_width}" y2="{y}" '
            f'stroke="#ccc" stroke-width="0.5" />'
        )
        lines.append(
            f'<text x="{margin - 10}" y="{y + 4}" text-anchor="end" '
            f'font-size="12" fill="#333">{value:.3f}</text>'
        )

    for x in x_values:
        px = scale_x(x)
        lines.append(
            f'<line x1="{px}" y1="{margin}" x2="{px}" y2="{margin + plot_height}" '
            f'stroke="#eee" stroke-width="0.5" />'
        )
        lines.append(
            f'<text x="{px}" y="{margin + plot_height + 20}" text-anchor="middle" '
            f'font-size="12" fill="#333">{x}</text>'
        )

    lines.append(
        f'<text x="{width/2}" y="{height - 10}" text-anchor="middle" font-size="14" '
        f'fill="#111">{x_label}</text>'
    )
    lines.append(
        f'<text x="15" y="{height/2}" transform="rotate(-90 15 {height/2})" '
        f'text-anchor="middle" font-size="14" fill="#111">{y_label}</text>'
    )
    lines.append(
        f'<text x="{width/2}" y="30" text-anchor="middle" font-size="16" '
        f'fill="#111" font-weight="bold">{title}</text>'
    )

    # Data series
    for idx, (label, values) in enumerate(series.items()):
        color = COLORS[idx % len(COLORS)]
        points = " ".join(
            f"{scale_x(x):.2f},{scale_y(y):.2f}" for x, y in zip(x_values, values)
        )
        lines.append(
            f'<polyline fill="none" stroke="{color}" stroke-width="2.5" '
            f'points="{points}"/>'
        )
        for x, y in zip(x_values, values):
            lines.append(
                f'<circle cx="{scale_x(x):.2f}" cy="{scale_y(y):.2f}" r="4" '
                f'fill="{color}"/>'
            )

    # Legend
    legend_x = margin + plot_width + 10

    for idx, label in enumerate(series.keys()):
        color = COLORS[idx % len(COLORS)]
        y = margin + idx * 20
        lines.append(
            f'<rect x="{legend_x}" y="{y}" width="12" height="12" fill="{color}"/>'
        )
        lines.append(
            f'<text x="{legend_x + 18}" y="{y + 11}" font-size="12" fill="#333">{label}</text>'
        )

    lines.extend(svg_footer())
    output_path.write_text("\n".join(lines))


def render_grouped_bar_chart(labels, series, output_path, title, y_label):
    width, height = 720, 480
    margin = 70
    plot_width = width - 2 * margin
    plot_height = height - 2 * margin
    categories = list(series.keys())
    stages = list(next(iter(series.values())).keys())
    max_value = max(max(values.values()) for values in series.values()) * 1.1

    lines = svg_header(width, height, title)
    lines.append(
        f'<text x="{width/2}" y="30" text-anchor="middle" font-size="16" '
        f'fill="#111" font-weight="bold">{title}</text>'
    )
    lines.append(
        f'<text x="20" y="{height/2}" transform="rotate(-90 20 {height/2})" '
        f'text-anchor="middle" font-size="14" fill="#111">{y_label}</text>'
    )

    # Axes
    lines.append(
        f'<line x1="{margin}" y1="{margin}" x2="{margin}" y2="{margin + plot_height}" '
        f'stroke="#333" stroke-width="1.5"/>'
    )
    lines.append(
        f'<line x1="{margin}" y1="{margin + plot_height}" '
        f'x2="{margin + plot_width}" y2="{margin + plot_height}" '
        f'stroke="#333" stroke-width="1.5"/>'
    )

    def scale_y(value):
        return margin + plot_height - (value / max_value) * plot_height

    # Y ticks
    y_ticks = 5
    for i in range(y_ticks + 1):
        value = max_value * i / y_ticks
        y = scale_y(value)
        lines.append(
            f'<line x1="{margin}" y1="{y}" x2="{margin + plot_width}" y2="{y}" '
            f'stroke="#ccc" stroke-width="0.5"/>'
        )
        lines.append(
            f'<text x="{margin - 10}" y="{y + 4}" text-anchor="end" font-size="12" '
            f'fill="#333">{int(value)}</text>'
        )

    group_width = plot_width / len(categories)
    bar_width = group_width / (len(stages) + 1)

    for idx, category in enumerate(categories):
        group_start = margin + idx * group_width
        lines.append(
            f'<text x="{group_start + group_width/2}" y="{margin + plot_height + 25}" '
            f'text-anchor="middle" font-size="12" fill="#333">{category}</text>'
        )
        for s_idx, stage in enumerate(stages):
            value = series[category][stage]
            x = group_start + bar_width * (s_idx + 0.5)
            y = scale_y(value)
            height_bar = margin + plot_height - y
            color = COLORS[s_idx % len(COLORS)]
            lines.append(
                f'<rect x="{x}" y="{y}" width="{bar_width * 0.9}" height="{height_bar}" '
                f'fill="{color}" />'
            )
            lines.append(
                f'<text x="{x + bar_width*0.45}" y="{y - 5}" text-anchor="middle" '
                f'font-size="11" fill="#333">{int(value)}</text>'
            )

    # Legend
    legend_x = margin + plot_width + 10
    for idx, stage in enumerate(stages):
        color = COLORS[idx % len(COLORS)]
        y = margin + idx * 20
        lines.append(
            f'<rect x="{legend_x}" y="{y}" width="12" height="12" fill="{color}"/>'
        )
        lines.append(
            f'<text x="{legend_x + 18}" y="{y + 11}" font-size="12" fill="#333">{stage}</text>'
        )

    lines.extend(svg_footer())
    output_path.write_text("\n".join(lines))


def generate_parallel_plots(summary_rows):
    grouped = defaultdict(lambda: {"time": [], "speedup": [], "threads": []})
    for row in summary_rows:
        grouped[row["number"]]["threads"].append(row["threads"])
        grouped[row["number"]]["time"].append(row["avg_time_ms"])
        grouped[row["number"]]["speedup"].append(row["speedup"])

    time_series = {str(number): data["time"] for number, data in grouped.items()}
    threads = sorted({row["threads"] for row in summary_rows})
    time_path = GRAPH_DIR / "parallel_time.svg"
    render_line_chart(
        time_series,
        threads,
        time_path,
        "Factorisation Time vs Threads",
        "Threads",
        "Average time (ms)",
    )

    speedup_series = {str(number): data["speedup"] for number, data in grouped.items()}
    speedup_path = GRAPH_DIR / "parallel_speedup.svg"
    render_line_chart(
        speedup_series,
        threads,
        speedup_path,
        "Measured Speedup",
        "Threads",
        "Speedup (T1/Tn)",
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
    rss_series = {}
    for entry in summary:
        label = f'{entry["size_mb"]} MB'
        rss_series[label] = {
            "Parent RSS": entry["parent_rss_kb"],
            "Child after fork": entry["child_post_fork_rss_kb"],
            "Child after writes": entry["child_post_write_rss_kb"],
        }
    rss_path = GRAPH_DIR / "cow_rss.svg"
    render_grouped_bar_chart(
        list(rss_series.keys()),
        rss_series,
        rss_path,
        "Copy-on-Write RSS Observations",
        "RSS (kB)",
    )


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
