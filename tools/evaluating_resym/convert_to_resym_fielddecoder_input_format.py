import re
import json


def parse_args():
    import argparse

    parser = argparse.ArgumentParser(
        description="Convert ReSym's VarDecoder output to format acceptable by ReSym's fielddecoder."
    )
    parser.add_argument("input_file", help="Path to input VarDecoder output")
    parser.add_argument("-o", "--output", help="Path to output file; default to stdout")
    parser.add_argument(
        "-f",
        "--force-overwrite",
        action="store_true",
        help="Force overwrite of output file if it exists",
    )
    return parser.parse_args()


def main():
    import os
    import sys

    args = parse_args()

    # Check if input_data file exists
    if not os.path.isfile(args.input_file):
        print(f"Error: Input file '{args.input_file}' does not exist.", file=sys.stderr)
        sys.exit(1)

    # Check if output file exists and handle overwrite option
    if args.output:
        if os.path.isfile(args.output) and not args.force_overwrite:
            print(
                f"Error: Output file '{args.output}' already exists. Use -f to overwrite.",
                file=sys.stderr,
            )
            sys.exit(1)

    # Read the input_data file
    with open(args.input_file, "r") as infile:
        input_data = infile.read()

    # Process the input_data based on the specified type
    output = process_vardecoder_to_fielddecoder(input_data)

    # Write the output to the specified file or stdout
    if args.output:
        with open(args.output, "w") as outfile:
            outfile.write(output)
    else:
        print(output)


def process_vardecoder_to_fielddecoder(input_data):
    functions = [transform(line) for line in input_data.splitlines()]
    functions = [json.dumps(f, indent=None, separators=(",", ":")) for f in functions]
    return "\n".join(functions)


PRIMITIVE_TYPES = {
    "int",
    "float",
    "void",
    "char",
    "double",
    "short",
    "long",
    "unsigned",
    "signed",
    "bool",
    "size_t",
}


def is_primitive(typename: str) -> bool:
    tokens = re.split(r"[\s\*]+", typename.strip())
    return all(tok in PRIMITIVE_TYPES for tok in tokens if tok)


def is_simple_primitive_or_pointer_to_primitive_or_similar(typename: str) -> bool:
    typename = typename.strip().replace("const ", "").replace("*", "")
    return is_primitive(typename)


def debug_print(msg):
    import sys
    from pprint import pprint

    pprint(msg, stream=sys.stderr)


def extract_memory_accesses(code: str):
    access_map = {}

    # Match expressions like (char *)(param_1 + 0x17)
    ptr_pattern = re.compile(
        r"""
        \(\s*                    # opening paren of cast
        (?P<type>[^\(\)]+)         # cast type
        \s*\)\s*                 # closing paren of cast
        \(\s*                    # opening paren for pointer arithmetic
        (?P<var>[a-zA-Z_]\w*)    # variable name
        \s*\+\s*
        (?P<offset>0x[0-9a-fA-F]+|\d+)   # offset
        \s*\)                    # closing paren
        """,
        re.VERBOSE,
    )

    for match in ptr_pattern.finditer(code):
        var = match.group("var")
        offset = match.group("offset")
        typ = match.group("type")
        # Reconstruct just the cast expression
        expr = f"({typ})({var} + {offset})"
        access_map.setdefault(var, {})[offset] = expr

    # Handle array-style access: var[index]
    index_pattern = re.compile(r"\b([a-zA-Z_][\w]*)\s*\[\s*([^\]]+?)\s*\]")
    for match in index_pattern.finditer(code):
        var, index = match.groups()
        access_map.setdefault(var, {})[index] = f"{var}[{index}]"

    return access_map


# Build a map of variable -> type from 'predict'
def parse_predict(predict_str):
    result = {}
    for line in predict_str.strip().split("\n"):
        if ": " in line:
            var, rest = line.split(":", 1)
            try:
                _, typename = rest.strip().split(",", 1)
            except ValueError:
                print("Got a weird line in prediction output", repr(line))
                typename = rest.strip()
            result[var.strip()] = typename.strip()

    return result


def transform(line):
    item = json.loads(line)
    raw_code = re.search(r"```(.*?)```", item["input"], re.DOTALL)
    code = raw_code.group(1) if raw_code else ""
    predict_map = parse_predict(item.get("predict", ""))
    mem_access = extract_memory_accesses(code)

    fieldmap = {}

    # ensure all params from predict map have a "0" entry if type is non-primitive
    for var, typ in predict_map.items():
        base_var = var.strip()
        if base_var not in fieldmap:
            fieldmap[base_var] = {}
        if not is_simple_primitive_or_pointer_to_primitive_or_similar(typ):
            fieldmap.setdefault(base_var, {})["0"] = base_var

    # add all discovered memory accesses
    for var, accesses in mem_access.items():
        if var.strip() in predict_map:
            fieldmap.setdefault(var, {}).update(accesses)

    # output format
    out_obj = {
        "input": f"```{code}```\nWhat are the variable name and type for the following memory accesses:",
        "custom___fieldmap": None,
    }
    if fieldmap:
        access_vars = sorted(
            {off for offsets in fieldmap.values() for off in offsets.values()}
        )
        # Make sure the non-parenthesized member of access_vars is first though
        access_vars = [v for v in access_vars if not "(" in v and not ")" in v] + [
            v for v in access_vars if "(" in v or ")" in v
        ]
        if access_vars:
            if not (out_obj["input"].endswith(",") or out_obj["input"].endswith(":")):
                out_obj["input"] += ","
            out_obj["input"] += ", ".join(access_vars)
    out_obj["input"] += "?\n"
    out_obj["custom___fieldmap"] = fieldmap
    out_obj["custom___varmap"] = item.get("custom___varmap", {})
    if "timeout" in item:
        out_obj["vardecoder___timeout"] = item["timeout"]
    out_obj["vardecoder___predict"] = item.get("predict", "")
    if "time" in item:
        out_obj["vardecoder___time"] = item["time"]

    return out_obj


if __name__ == "__main__":
    main()
