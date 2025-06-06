# Convert decompilation into files that ReSym's VarDecoder will accept.


def parse_args():
    import argparse

    parser = argparse.ArgumentParser(
        description="Convert `.decompiled-wvi` files to ReSym's input_data format."
    )
    parser.add_argument("input_file", help="Path to .decompiled-wvi file")
    parser.add_argument("-o", "--output", help="Path to output file; default to stdout")
    parser.add_argument(
        "-f",
        "--force-overwrite",
        action="store_true",
        help="Force overwrite of output file if it exists",
    )
    parser.add_argument(
        "-m",
        "--mangle-var-names",
        action="store_true",
        help="Mangle variable names in output",
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

    # Process the input_data
    output = process_vardecoder(input_data, args.mangle_var_names)

    # Write the output to the specified file or stdout
    if args.output:
        with open(args.output, "w") as outfile:
            outfile.write(output)
    else:
        print(output)


def mangle_var_names(function):
    import re

    code, varmap = function["code"], function["varmap"]
    assert len(varmap) <= len(
        MANGLED_VARIABLE_NAMES
    ), f"Too many variables to mangle. Need {len(varmap)} but only have {len(MANGLED_VARIABLE_NAMES)}."

    mangled_code = code
    for i, var in enumerate(varmap):
        prev_name = var["ident"]
        new_name = MANGLED_VARIABLE_NAMES[i]
        mangled_code = re.sub(rf"\b{re.escape(prev_name)}\b", new_name, mangled_code)
    mangled_varmap = [
        {
            "name": var["name"],
            "ident": MANGLED_VARIABLE_NAMES[i],
        }
        for i, var in enumerate(varmap)
    ]
    return {"code": mangled_code, "varmap": mangled_varmap}


def parse_input_data(input_data, mangle):
    input_data = input_data.replace("/**<<EOF>>**/", "")
    functions = input_data.strip().split("***************************/")
    functions = [
        f.strip().split("/************************\n") for f in functions if f.strip()
    ]

    def varmap(x):
        x = x.strip()
        if not x:
            return []
        split_vars = [t.split(": ") for t in x.split("\n")]
        for ni in split_vars:
            if len(ni) != 2:
                print("......")
                print(x)
                print("......")
                raise ValueError(f"Expected 2 parts in varmap split, got {len(ni)}")
        return [
            {
                "name": a.strip(),
                "ident": b.strip(),
            }
            for a, b in split_vars
        ]

    for i, f in enumerate(functions):
        if len(f) != 2:
            print("......")
            print(input_data.strip().split("***************************/")[i])
            print("......")
            raise ValueError(f"Expected 2 parts in function split, got {len(f)}")

    functions = [
        {
            "code": f[0].strip(),
            "varmap": varmap(f[1].split("varmap:")[1]),
        }
        for f in functions
    ]
    if mangle:
        functions = [mangle_var_names(f) for f in functions]
    return functions


def varstrings(varmap):
    return ", ".join([f"`{v['ident']}`" for v in varmap])


def process_vardecoder(input_data, mangle_var_names):
    import json

    functions = parse_input_data(input_data, mangle_var_names)
    functions = [
        {
            "input": f'What are the original name and data type of variables {varstrings(f["varmap"])}?\n```\n{f["code"]}\n```',
            "custom___varmap": f["varmap"],
        }
        for f in functions
    ]
    functions = [json.dumps(f, indent=None, separators=(",", ":")) for f in functions]
    assert not any(
        "\n" in f for f in functions
    ), "Newlines found in JSON output, which is unexpected and unusable for JSONL."
    return "\n".join(functions)


# TODO: Figure out a different set of mangled variable names
MANGLED_VARIABLE_NAMES = [
    # Fruits first
    "apple",
    "banana",
    "cherry",
    "date",
    "elderberry",
    "fig",
    "grape",
    "honeydew",
    "iceberg",
    "jackfruit",
    "kiwi",
    "lemon",
    "mango",
    "nectarine",
    "orange",
    "papaya",
    "quince",
    "raspberry",
    "strawberry",
    "tangerine",
    "uglifruit",
    "vanilla",
    "watermelon",
    "xiguafruit",
    "yellowfruit",
    "zucchini",
    # Then some famous people
    "einstein",
    "curie",
    "newton",
    "darwin",
    "hawking",
    "turing",
    "gates",
    "jobs",
    "lovelace",
    "franklin",
    "bohr",
    "feynman",
    "heisenberg",
    "planck",
    "pauling",
    "nash",
    "sagan",
    # Then some names of towns
    "paris",
    "london",
    "newyork",
    "tokyo",
    "berlin",
    "madrid",
    "rome",
    "sydney",
    "toronto",
    "losangeles",
    "chicago",
    "seattle",
    "bangalore",
    "singapore",
]

ORIG_MANGLED_VARIABLE_NAMES_LEN = len(MANGLED_VARIABLE_NAMES)
for i in range(0, 5):
    MANGLED_VARIABLE_NAMES += [
        f"{name}{i}"
        for name in MANGLED_VARIABLE_NAMES[:ORIG_MANGLED_VARIABLE_NAMES_LEN]
    ]


if __name__ == "__main__":
    main()
