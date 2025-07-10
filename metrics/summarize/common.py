import argparse
import os
import sys

parser = argparse.ArgumentParser()
parser.add_argument("path", help="Path to the directory containing the eval files")
parser.add_argument("--output-csv", help="Path to the output CSV", required=False)
parser.add_argument("--output-figure", help="Path to the output figure", required=False)
parser.add_argument(
    "--output-latex-summary", help="Path to the output LaTeX summary", required=False
)
parser.add_argument("--benchmark-name", help="Name of the benchmark", required=False)
parser.add_argument(
    "--verbose", help="Print data verbosely", action="store_true", required=False
)
parser.add_argument(
    "--show-figure", help="Show the figure", action="store_true", required=False
)
args = parser.parse_args()

if args.output_latex_summary and not args.benchmark_name:
    parser.error("--output-latex-summary requires --benchmark-name")

# The extensions of the files to be read, in order of graph color
extensions = [
    "scorer-trex",
    "scorer-ghidra-wvi",
    "scorer-baselinetrivial",
]

our_tool = "TRex"

extension_map = {
    "scorer-trex": our_tool,
    "scorer-ghidra-wvi": "Ghidra",
    "scorer-baselinetrivial": "Baseline",
}

if os.getenv("ENABLE_RESYM") and args.benchmark_name in ("coreutils", "spec"):
    extensions.append("scorer-resym")
    extension_map["scorer-resym"] = "ReSym"

if os.getenv("ENABLE_GEN"):
    if not "scorer-resym" in extensions:
        # Skipping geneval for purely non-ML benchmarks
        sys.exit(0)
    extensions = [f"gen-{ext}" for ext in extensions]
    extension_map = {f"gen-{ext}": tool for ext, tool in extension_map.items()}

our_tool_ext = [ext for ext, tool in extension_map.items() if tool == our_tool][0]

# Figure scaling, so that fonts are readable size
scaling = 1.5
