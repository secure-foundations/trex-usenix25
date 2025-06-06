import json
import sys


def parse_args():
    import argparse

    parser = argparse.ArgumentParser(
        description="Convert ReSym's output into C-like types."
    )
    parser.add_argument("input_file", help="Path to ReSym output")
    parser.add_argument("program_name", help="Name for the program")
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

    # Process the input_data
    output = process_data(input_data, args.program_name)

    # Write the output to the specified file or stdout
    if args.output:
        with open(args.output, "w") as outfile:
            outfile.write(output)
    else:
        print(output)


def debug_print(msg, name=None):
    import sys
    from pprint import pformat

    prefix = ""
    if name:
        prefix = f">> {name} = "

    prefix_len = len(prefix)
    formatted = pformat(msg, indent=2, width=80, compact=True)
    lines = formatted.splitlines()
    output = []
    for i, line in enumerate(lines):
        if i == 0:
            output.append(f"{prefix}{line}")
        else:
            output.append(f"{' ' * prefix_len}{line}'")
    print("\n".join(output), file=sys.stderr)


def process_data(input_data, program_name):
    functions = [transform(line) for line in input_data.splitlines()]
    variable_types = {}
    type_information = {}

    for vt, ti in functions:
        variable_types.update(vt)
        type_information.update(ti)

    output = f"PROGRAM\nname {program_name}\n\n"

    output += "VARIABLE_TYPES\n"
    for variable, typ in variable_types.items():
        output += f"\t{variable}\t{typ}\n"
    output += "\n"

    output += "TYPE_INFORMATION\n"
    for typ, info in type_information.items():
        output += f"\t{typ}\n{process_type_info(info)}\n"

    return output


# Shift the type information into the correct tab level for the output
def process_type_info(type_info):
    return "\n".join(f"\t\t{line}" for line in type_info.splitlines() if line.strip())


# Build a map of variable -> type from vardecoder prediction
def parse_vardecoder_predict(predict_str):
    if predict_str.strip().startswith(":"):
        return {}
    result = {}
    for line in predict_str.strip().split("\n"):
        if ": " in line:
            var, rest = line.split(":", 1)
            try:
                _, typename = rest.strip().split(",", 1)
            except ValueError:
                # print(
                #     "Got a weird line in prediction output", repr(line), file=sys.stderr
                # )
                continue
            result[var.strip()] = typename.strip()
    return result


class MemoryAccessError(Exception):
    pass


# Parse a memory access (str -> variable, shift, outsize)
def parse_mem_access(mem_access_str):
    import re

    # Patterns:
    plain_var_pattern = r"^\s*([a-zA-Z_]\w*)\s*$"
    casted_ptr_pattern = r"^\(\s*\w+\s*\**\s*\)\s*\(\s*([a-zA-Z_]\w*)\s*\+\s*(0x[\da-fA-F]+|\d+)\s*\)\s*$"
    array_access_pattern = r"^\s*([a-zA-Z_]\w*)\s*\[\s*(0x[\da-fA-F]+|\d+)\s*\]\s*$"

    match = re.match(plain_var_pattern, mem_access_str)
    if match:
        target = match.group(1)
        return target, 0, None

    match = re.match(casted_ptr_pattern, mem_access_str)
    if match:
        base = match.group(1)
        offset = int(match.group(2), 0)
        type_pattern = re.search(r"\((\w+)\s*\*\)", mem_access_str)
        size = None
        if type_pattern:
            type_str = type_pattern.group(1).lower()
            match type_str:
                case "long" | "ulong" | "undefined8" | "size_t" | "double" | "longlong":
                    size = 8  # Assumes 64-bit platform
                case "int" | "undefined4" | "uint" | "float":
                    size = 4
                case "short" | "ushort" | "undefined2":
                    size = 2
                case (
                    "char"
                    | "byte"
                    | "sbyte"
                    | "uchar"
                    | "undefined1"
                    | "undefined"
                    | "bool"
                ):
                    size = 1
                case "void":
                    size = 0
                case _:
                    # print(
                    #     f"[!!!] Unknown size for pointee: {type_str}", file=sys.stderr
                    # )
                    size = None
        return base, offset, size

    match = re.match(array_access_pattern, mem_access_str)
    if match:
        base = match.group(1)
        offset = int(match.group(2), 0)
        return base, offset, None

    raise MemoryAccessError(f"Unrecognized memory access pattern: {mem_access_str}")


# Remove `const ` and `*` from type
def cleaned_type(typ, expect_pointer):
    typ = typ.strip()
    if typ.startswith("const "):
        typ = typ.split(" ", 1)[1].strip()
        return cleaned_type(typ, expect_pointer)
    if expect_pointer:
        assert typ.endswith("*"), f"Expected pointer type, got {typ}"
    if typ.endswith("*"):
        typ = typ[:-1].strip()
        return cleaned_type(typ, False)
    return typ


# Build a map of (variable, shift) -> (fieldname, fieldtype, outsize)
#
# Also return the variable -> type from fielddecoder prediction
#
# shift is the *non* size-multiplied offset
def parse_fielddecoder_predict(predict_str):
    if predict_str.strip().startswith(":"):
        return {}, {}
    result_vartype = {}
    result_fieldtype = {}
    for line in predict_str.strip().split("\n"):
        if line.strip() == "":
            continue
        if line.count("->") != 1 or line.count(":") != 1 or line.count(",") != 2:
            # print(
            #     "Got a weird line in prediction output, that does not match the expected format",
            #     repr(line),
            #     "Counts: ",
            #     (
            #         line.count("->"),
            #         line.count(":"),
            #         line.count(","),
            #     ),
            #     "Ignoring.",
            #     file=sys.stderr,
            # )
            continue
        try:
            mem_access, rest = line.strip().split(":", 1)
            var, shift, outsize = parse_mem_access(mem_access.strip())
            name_and_vartype, rest = rest.strip().split("->", 1)
            _, vartype = name_and_vartype.strip().split(",", 1)
            result_vartype[var.strip()] = vartype.strip()
            fieldname, fieldtype = rest.strip().split(",", 1)
            try:
                result_fieldtype.setdefault(cleaned_type(vartype, True), {})[shift] = (
                    fieldname.strip(),
                    fieldtype.strip(),
                    outsize,
                )
            except AssertionError:
                # print(
                #     "Got a weird line in prediction output, where the type was not a pointer",
                #     repr(line),
                #     "Ignoring.",
                #     file=sys.stderr,
                # )
                continue
        except MemoryAccessError:
            # print(
            #     "Got a weird line in prediction output (mem access)",
            #     repr(line),
            #     "Ignoring.",
            #     file=sys.stderr,
            # )
            continue
        except ValueError:
            print(
                "Got a weird line in prediction output",
                repr(line),
                file=sys.stderr,
            )
            raise
    return (result_vartype, result_fieldtype)


# Process the `type` into both the expansion, as well as (possibly empty)
# triggers of possible other types.
def expand_type(typ, field_map, base_size_map):
    if typ in BUILTIN_TYPES:
        return "BuiltInDataType", []
    if typ.startswith("const ") or typ.startswith("struct "):
        real_type = typ.split(" ", 1)[1].strip()
        return f"TypeDef\t{real_type}", [real_type]
    if typ in TYPEDEF_TYPES:
        real_type = TYPEDEF_TYPES[typ]
        return f"TypeDef\t{real_type}", [real_type]
    if typ.endswith("]") and "[" in typ:
        # This is an array type, e.g., `char[10]` or `int[5]`
        #
        # We get the number of elements in the array, and then return the base type
        base_type = typ.split("[", 1)[0].strip()
        try:
            size_of_element = base_size_map[base_type]
        except KeyError:
            if base_type + " *" in BASE_SIZE_MAP:
                size_of_element = BASE_SIZE_MAP[base_type + " *"]
            elif base_type.endswith("*"):
                # Assuming 64-bit pointer size
                size_of_element = 8
            else:
                # debug_print(base_type, "base_type")
                size_of_element = 1
        assert size_of_element != 0, f"Size of {base_type} is 0"
        try:
            number_of_elements = int(typ.split("[", 1)[1].split("]", 1)[0])
        except ValueError:
            number_of_elements = 1
        return f"Array\t{base_type}\t{size_of_element}\t{number_of_elements}", [
            base_type
        ]
    if typ.endswith("*"):
        real_type = typ[:-1].strip()
        return f"Pointer\t8\t{real_type}", [real_type]
    if typ in field_map:
        if typ not in base_size_map:
            # raise ValueError(f"Type {typ} not in base size map {base_size_map!r}")
            return "TypeDef\tundefined1", ["undefined1"]
        base_size = base_size_map[typ]
        result_type = f"Structure\n"
        triggered = set()
        for i, (shift, (field_name, field_type, outsize)) in enumerate(
            field_map[typ].items()
        ):
            if outsize is None:
                if field_type.strip().endswith("*"):
                    outsize = 8
                else:
                    outsize = base_size
            result_type += (
                f"\t{i}\t{shift * base_size}\t{field_type}\t{outsize}\t{field_name}\n"
            )
            triggered.add(field_type)
        if typ in triggered or "const " + typ in triggered:
            # We got a literally broken recursive type e.g., `struct foo{foo;}`.
            # We _must_ mark this as undefined.
            return "TypeDef\tundefined1", ["undefined1"]
        return result_type, list(triggered)
    if typ == "undefined":
        return "DefaultDataType", []
    # debug_print(typ, "typ")
    return f"TypeDef\tundefined1", ["undefined1"]


# Get the base size from the decompilation `input` string for all `variables`
def get_base_size(input_str, variables):
    import re

    result = {}
    for v in variables:
        if v in input_str:
            re_match = re.search(rf"\b{re.escape(v)}\b", input_str)
            if not re_match:
                # If variable is not found in the input string, we can skip it
                continue
            prefix = input_str[: re_match.start()]
            done_with_var = False
            for possible_end in BASE_SIZE_MAP:
                if prefix.endswith(possible_end):
                    result[v] = BASE_SIZE_MAP[possible_end]
                    done_with_var = True
                    break
            if not done_with_var:
                if prefix.endswith(" *"):
                    # There are many arbitrary types that show up (e.g., `lconv
                    # *`/ `dev_t *` etc that are not actually derefenced at
                    # _separate_ base values, but only internally). We just skip these.
                    continue
                else:
                    raise Exception(f"Unknown base size for {v}: {prefix!r}.")
    return result


# Transform the single line of input data into the strctured format we need.
def transform(input_data):
    data = json.loads(input_data)
    variables = {x["ident"]: x["name"] for x in data["custom___varmap"]}
    vd_type_map = parse_vardecoder_predict(data.get("vardecoder___predict", ""))
    fd_type_map, fd_field_map = parse_fielddecoder_predict(data.get("predict", ""))

    # debug_print(variables, "variables")
    # debug_print(vd_type_map, "vd_type_map")
    # debug_print(fd_type_map, "fd_type_map")

    vd_type_map.update(fd_type_map)
    type_map = vd_type_map

    # debug_print(type_map, "type_map")

    base_size_vmap = get_base_size(data["input"], variables.keys())
    if any(base_size_vmap[v] == 0 for v in base_size_vmap):
        # We have a base size of 0, which means we have no idea what the type is.
        # We just skip it.
        for v in list(base_size_vmap.keys()):
            if base_size_vmap[v] == 0:
                del base_size_vmap[v]
    base_size_map = {
        cleaned_type(vd_type_map[v], False): base_size_vmap.get(v)
        for v in type_map
        if v in base_size_vmap
    }

    variable_types = {}
    for var, typ in vd_type_map.items():
        mapped_var = find_variable_in_map(var, variables)
        if mapped_var is None:
            continue
        variable_types[mapped_var] = vd_type_map[var]

    types_to_handle = list(vd_type_map.values())
    type_expansions = {}
    while any(t not in type_expansions for t in types_to_handle):
        # Pick a random type to handle
        typ = [t for t in types_to_handle if t not in type_expansions][0]
        expansion, newly_triggered = expand_type(typ, fd_field_map, base_size_map)
        type_expansions[typ] = expansion
        types_to_handle.extend(newly_triggered)

    return variable_types, type_expansions


def find_variable_in_map(var, var_map):
    if var in var_map:
        return var_map[var]
    # print(
    #     f"Warning: Variable {var} not found in variable map. Skipping.",
    #     file=sys.stderr,
    # )
    return None


BUILTIN_TYPES = {
    "void",
    "bool",
    "char",
    "string",
    "uchar",
    "byte",
    "sbyte",
    "wchar_t",
    "short",
    "ushort",
    "word",
    "sword",
    "int",
    "uint",
    "dword",
    "sdword",
    "long",
    "ulong",
    "qword",
    "sqword",
    "ulonglong",
    "longlong",
    "uint16",
    "float",
    "float4",
    "double",
    "float8",
    "longdouble",
    "float10",
    "undefined1",
    "undefined2",
    "undefined3",
    "undefined4",
    "undefined5",
    "undefined6",
    "undefined7",
    "undefined8",
}

TYPEDEF_TYPES = {
    "_Bool": "bool",
    "boolean": "bool",
    "bool_t": "bool",
    "intmax_t": "int64_t",
    "uintmax_t": "uint64_t",
    "u64": "uint64_t",
    "u32": "uint32_t",
    "u16": "uint16_t",
    "u8": "uint8_t",
    "s64": "int64_t",
    "i64": "int64_t",
    "s32": "int32_t",
    "i32": "int32_t",
    "s16": "int16_t",
    "i16": "int16_t",
    "s8": "int8_t",
    "i8": "int8_t",
    "f32": "float",
    "f64": "double",
    "u_char": "uchar",
    "u_int": "uint",
    "u_short": "ushort",
    "u_long": "ulong",
    "unsigned char": "uchar",
    "uint8_t": "byte",
    "uint16_t": "ushort",
    "uint32_t": "uint",
    "uint64_t": "ulonglong",
    "int8_t": "sbyte",
    "int16_t": "short",
    "int32_t": "int",
    "int64_t": "longlong",
    "short int": "short",
    "unsigned short int": "ushort",
    "short unsigned int": "ushort",
    "long int": "long",
    "long unsigned int": "ulong",
    "unsigned long int": "ulong",
    "size_t": "uint64_t",
    "uint32_t[-]": "uint32_t",
    "unsigned int": "uint",
}


BASE_SIZE_MAP = {
    "void *": 0,
    "bool *": 1,
    "char *": 1,
    "signed char *": 1,
    "unsigned char *": 1,
    "byte *": 1,
    "undefined *": 1,
    "undefined1 *": 1,
    "uint8_t *": 1,
    "short *": 2,
    "signed short *": 2,
    "unsigned short *": 2,
    "undefined2 *": 2,
    "uint16_t *": 2,
    "int *": 4,
    "signed int *": 4,
    "unsigned int *": 4,
    "off_t *": 4,
    "wchar_t *": 4,
    "undefined4 *": 4,
    "float *": 4,
    "uint32_t *": 4,
    "long *": 8,
    "unsigned long *": 8,
    "size_t *": 8,
    "ssize_t *": 8,
    "ptrdiff_t *": 8,
    "intptr_t *": 8,
    "uintptr_t *": 8,
    "intmax_t *": 8,
    "uintmax_t *": 8,
    "long long *": 8,
    "unsigned long long *": 8,
    "wint_t *": 8,
    "double *": 8,
    "undefined8 *": 8,
    "uint64_t *": 8,
    "long double *": 16,
    # Weird ones that show up once in a while
    "unkbyte10 *": 10,
    "undefined3 *": 3,
    "undefined5 *": 5,
    "undefined6 *": 6,
    "undefined7 *": 7,
    "undefined9 *": 9,
    # Void pointers are not allowed to be dereferenced
    "void *": 0,
    # Pointer to pointer is always 8 bytes, because we are on a 64-bit
    "**": 8,
    # Pointer to a function is always 8 bytes, because we are on a 64-bit
    "(*": 8,
    "code *": 8,
    # Regular integers and such are not pointers, so shifting is just a normal
    # move by a one (i.e., no multiplication by the size of the type). We check
    # this simply by looking for a terminal space.
    " ": 1,
    # Every once in a while, we get weird output that breaks parsing, this
    # simply skips those few rare cases.
    "\n": 0,
    ":": 0,
    "stat ": 0,
    "(": 0,
    "off_t ": 0,
}


if __name__ == "__main__":
    main()
