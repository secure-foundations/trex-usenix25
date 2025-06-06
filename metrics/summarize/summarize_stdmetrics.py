# Arguments
#
#  argv[1] = path to directory containing the `.stdmetrics` CSV files
#  argv[2] = output CSV file path (if not specified, output to stdout)

import os
import sys
import pandas as pd
import glob

if len(sys.argv) != 2 and len(sys.argv) != 3:
    print(
        "Usage: python summarize_stdmetrics.py <path_dir_with_stdmetrics_files> [output_csv_path]"
    )
    sys.exit(1)

stdmetrics_dir = sys.argv[1]
if not os.path.isdir(stdmetrics_dir):
    print(f"Error: {stdmetrics_dir} is not a valid directory.")
    sys.exit(1)

output_csv_path = None
if len(sys.argv) == 3:
    output_csv_path = sys.argv[2]
    if os.path.exists(output_csv_path):
        print(f"Warning: {output_csv_path} already exists. It will be overwritten.")


# Get all .stdmetrics files in the directory
stdmetrics_files = glob.glob(os.path.join(stdmetrics_dir, "*.stdmetrics"))
if not stdmetrics_files:
    print(f"No .stdmetrics files found in {stdmetrics_dir}.")
    sys.exit(1)


combined_metrics = {}

# Iterate through each .stdmetrics file
for stdmetrics_file in stdmetrics_files:
    # Read the CSV file
    df = pd.read_csv(stdmetrics_file)

    # Iterate through each row in the DataFrame
    for index, row in df.iterrows():
        tool = row["tool"]
        total = row["total"]
        tp = row["tp"]
        fp = row["fp"]
        fn = row["fn"]

        # Initialize the tool entry if it doesn't exist
        if tool not in combined_metrics:
            combined_metrics[tool] = {
                "total": 0,
                "tp": 0,
                "fp": 0,
                "fn": 0,
            }

        # Update the metrics
        combined_metrics[tool]["total"] += total
        combined_metrics[tool]["tp"] += tp
        combined_metrics[tool]["fp"] += fp
        combined_metrics[tool]["fn"] += fn

# Convert the combined metrics to a DataFrame
summary_df = pd.DataFrame.from_dict(
    combined_metrics, orient="index", columns=["total", "tp", "fp", "fn"]
)

# Calculate precision, recall, and F1 score
summary_df["precision"] = summary_df["tp"] / (summary_df["tp"] + summary_df["fp"])
summary_df["recall"] = summary_df["tp"] / (summary_df["tp"] + summary_df["fn"])
summary_df["f1"] = (2 * summary_df["precision"] * summary_df["recall"]) / (
    summary_df["precision"] + summary_df["recall"]
)
# Reset index to have 'tool' as a column
summary_df.reset_index(inplace=True)
summary_df.rename(columns={"index": "tool"}, inplace=True)

# Sort the DataFrame by tool name
summary_df.sort_values(by="tool", inplace=True)

# Round the precision, recall, and F1 score to 2 decimal places
summary_df["precision"] = summary_df["precision"].round(2)
summary_df["recall"] = summary_df["recall"].round(2)
summary_df["f1"] = summary_df["f1"].round(2)

# Save the summary DataFrame to a CSV file, or print to stdout
if output_csv_path:
    summary_df.to_csv(output_csv_path, index=False)
    print(f"Summary saved to {output_csv_path}.")
else:
    print(summary_df.to_csv(index=False))
