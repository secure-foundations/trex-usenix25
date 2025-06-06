import pandas as pd
import sys
from glob import glob
import matplotlib.pyplot as plt
import matplotlib.ticker as mtick
from common import args, extensions, extension_map, scaling

# All the relevant files
files = {ext: glob(f"{sys.argv[1]}/*.{ext}") for ext in extensions}

# Read all the files into a single dataframe per extension
dfs = {ext: pd.concat([pd.read_csv(f) for f in files[ext]]) for ext in extensions}

# Collect the data into a single dataframe, adding the extension as a column
df = pd.concat([dfs[ext].assign(Tool=ext) for ext in extensions])

# Collect the data on counts per score and tool (the index at the end is to
# order the columns)
grouped = df.groupby(["Score", "Tool"]).size().unstack()[extensions]

# Make sure to include score counts for all `Score`s in range [0, 9]
for i in range(10):
    if i not in grouped.index:
        grouped.loc[i] = [0] * len(extensions)

# Replace all NaNs with 0s
grouped = grouped.fillna(0)

# Use integers instead of floats
grouped = grouped.astype(int)

# Sort the rows by score
grouped = grouped.sort_index()

# Rename the columns to the tool names
grouped = grouped.rename(columns=extension_map)

# Print the data
if args.verbose:
    print(grouped)

if args.output_csv:
    grouped.to_csv(args.output_csv)
    if args.verbose:
        print(f"Saved data to {args.output_csv}")

enabled_charts = {
    "Count over Score": False,
    "Score over Vars": False,
    "Net Score over Vars": False,
    "CDF of Score": True,
}
num_enabled_charts = sum(enabled_charts.values())

# Plot the data as a bar chart, with count on the y-axis and the "Score" on the
# x-axis, colored by tool (in order of extensions)
if args.output_figure or args.show_figure:
    fig, axes = plt.subplots(
        nrows=1,
        ncols=num_enabled_charts,
        figsize=(num_enabled_charts * 6 / scaling, 5 / scaling),
    )

    axes_iter = iter(axes) if num_enabled_charts > 1 else iter([axes])

    # Plot the raw data
    if enabled_charts["Count over Score"]:
        ax = next(axes_iter)
        grouped.plot.bar(
            # X-axis label
            xlabel="Score",
            # Y-axis label
            ylabel="Count",
            # Title
            title=None,
            # Legend
            legend=True,
            # Axis
            ax=ax,
        )

    # Plot with cumulative count on x-axis, and max score on y-axis, via a histogram
    if enabled_charts["Score over Vars"]:
        ax = next(axes_iter)
        for tool in extension_map.values():
            cumsum = grouped[tool].cumsum()

            xs = [0]
            ys = [0]
            for i in cumsum.index:
                if cumsum[i] == xs[-1]:
                    continue
                xs.append(xs[-1])
                ys.append(i)
                xs.append(cumsum[i])
                ys.append(i)

            ax.plot(
                xs,
                ys,
                label=tool,
                color=f"C{list(extension_map.values()).index(tool)}",
            )

        ax.set_xlabel("Number of variables")
        ax.set_ylabel("Score")
        ax.legend()

    # Plot with percentage max score on x-axis and percentage of variables on y-axis
    if enabled_charts["CDF of Score"]:
        ax = next(axes_iter)
        for tool in extension_map.values():
            cumsum = grouped[tool].cumsum()

            xs = [0]
            ys = [0]
            for i in cumsum.index:
                if cumsum[i] == ys[-1]:
                    continue
                ys.append(ys[-1])
                xs.append(i)
                ys.append(cumsum[i])
                xs.append(i)

            max_y = max(ys)
            ys = [100.0 * y / max_y for y in ys]

            ax.plot(
                xs,
                ys,
                label=tool,
                color=f"C{list(extension_map.values()).index(tool)}",
            )

        ax.yaxis.set_major_formatter(mtick.PercentFormatter())
        ax.set_xlabel("Score")
        ax.set_ylabel("Fraction of Variables")
        ax.legend()

    # Plot with cumulative count on x-axis, and total score on y-axis, via a histogram
    if enabled_charts["Net Score over Vars"]:
        ax = next(axes_iter)
        for tool in extension_map.values():
            xs = [0]
            ys = [0]
            for i in grouped[tool].index:
                xs.append(xs[-1] + grouped[tool][i])
                ys.append(ys[-1] + grouped[tool][i] * i)

            ax.plot(
                xs,
                ys,
                label=tool,
                color=f"C{list(extension_map.values()).index(tool)}",
            )

        ax.set_xlabel("Number of variables")
        ax.set_ylabel("Net Score")
        ax.legend()

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
