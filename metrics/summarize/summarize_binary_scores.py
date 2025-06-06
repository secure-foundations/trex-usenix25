import pandas as pd
import sys
from glob import glob
import matplotlib.pyplot as plt
from mplcursors import cursor
from common import args, extensions, extension_map, our_tool, our_tool_ext, scaling
import pathlib
import os


# All the relevant files
files = {ext: glob(f"{sys.argv[1]}/*.{ext}") for ext in extensions}

# Read all the files into a single dataframe per file
dfs = {ext: {f: pd.read_csv(f) for f in files[ext]} for ext in extensions}

# Add the basename of the file as a column, and tool as a column (using
# `extension_map`)
for ext in extensions:
    for f in dfs[ext]:
        dfs[ext][f]["File"] = pathlib.Path(f).stem
        dfs[ext][f]["Tool"] = extension_map[ext]

# Collect the data into a single dataframe
df = pd.concat([dfs[ext][f] for ext in extensions for f in dfs[ext]])

# Collect the data on average score per file and tool
grouped = (
    df.groupby(["File", "Tool"])["Score"]
    .mean()
    .unstack()[[extension_map[ext] for ext in extensions]]
)

# Replace all NaNs with 0s
grouped = grouped.fillna(0.0)

# Print the data
if args.verbose:
    print(grouped)

if args.output_csv:
    grouped.to_csv(args.output_csv)
    if args.verbose:
        print(f"Saved data to {args.output_csv}")

# bar_charts = ["Unsorted"] + extensions + ["Separately Sorted"]
bar_charts = [our_tool_ext]

# Plot the data as a bar chart
if args.output_figure or args.show_figure:
    fig, axes = plt.subplots(
        nrows=1,
        ncols=len(bar_charts),
        sharey=True,
        figsize=(len(bar_charts) * 6 / scaling, 5 / scaling),
    )

    # Sort by scores on each extension and create a separate bar chart for each sorting
    for i, ext in enumerate(bar_charts):
        if ext == "Unsorted":
            sorted_df = grouped
        elif ext == "Separately Sorted":
            # Independently sort each extension
            sorted_df = grouped.copy()
            for x in extensions:
                sorted_df[extension_map[x]] = sorted(sorted_df[extension_map[x]])
        else:
            sorted_df = (
                grouped.sort_values(extension_map[ext])
                if ext in extensions
                else grouped
            )
        ax = axes[i] if len(bar_charts) > 1 else axes
        sorted_df.plot.bar(
            # X-axis label
            xlabel="File"
            if ext != "Separately Sorted"
            else "Individually Sorted Files",
            # Y-axis label
            ylabel="Mean Score",
            # Title
            title=(f"Sorted by {extension_map[ext]}" if ext in extensions else ext) if len(bar_charts) > 1 else None,
            # Legend
            legend=True,
            # Plot on subplot
            ax=ax,
        )
        # Move the legend to top-left
        ax.legend(loc="upper left")
        # Disable x-axis labels
        ax.set_xticklabels(labels=[])
        # Enable hover
        if args.show_figure:
            # 2D dataframe, 1st axis is tool name, 2nd axis is index, value is file name
            key_lookup = pd.DataFrame(
                index=sorted_df.columns,
                data={i: sorted_df.index[i] for i in range(len(sorted_df.index))},
            ).transpose()

            def print_hover(key_lookup):
                # NOTE: Need to do this as a separate function just to prevent
                # the lambda from capturing the wrong value of `key_lookup`
                # (i.e., otherwise it takes the last value of `key_lookup`
                # instead of the value at the time of the lambda's creation)
                key_lookup = key_lookup.copy()

                def f(sel):
                    text = "".join(
                        (
                            str(key_lookup[sel.artist.get_label()][sel.index]),
                            ", ",
                            str(sel.artist.get_label()),
                            ": ",
                            str(sorted_df[sel.artist.get_label()][sel.index]),
                        )
                    )
                    sel.annotation.set_text(text)

                return f

            if ext != "Separately Sorted":
                cursor([ax], hover=True).connect(
                    "add",
                    print_hover(key_lookup),
                )


# Tighten the layout
plt.tight_layout()

# Save the plot
if args.output_figure:
    plt.savefig(args.output_figure)
    if args.verbose:
        print(f"Saved plot to {args.output_figure}")

# Show the plot
if args.show_figure:
    plt.show()

# Close the plot
plt.close()

# Number of times any tool wins over the other
winning = {tool:
           {other:
            (grouped[tool] > grouped[other]).sum()
            for other in extension_map.values()}
           for tool in extension_map.values()}

# Print the LaTeX summary
if args.output_latex_summary:
    benchmark_name = args.benchmark_name
    win = winning[our_tool]["Ghidra"]
    prefix = ''
    if os.getenv("ENABLE_GEN"):
        prefix = 'gen'
    with open(args.output_latex_summary, "w") as f:
        benchname = benchmark_name.replace('-O0', 'Ozero').replace('-O1', 'Oone').replace('-O2', 'Otwo').replace('-O3', 'Othree')
        f.write(f'\\newcommand{{\\{prefix}counted{benchname}binaries}}[0]{{{len(grouped)}}}\n')
        f.write(f'\\newcommand{{\\{prefix}winning{benchname}binaries}}[0]{{{win}}}\n')
        f.write(f'\\newcommand{{\\{prefix}varsin{benchname}}}[0]{{{len(df[df["Tool"] == our_tool])}}}\n')
        for tool in extension_map.values():
            toolname = tool.lower() if tool != our_tool else "ourtool"
            f.write(f'\\newcommand{{\\{prefix}avg{benchname}{toolname}}}[0]{{{round(grouped[tool].mean(), 3):.3f}}}\n')
    if args.verbose:
        print(f"Saved LaTeX summary to {args.output_latex_summary}")
