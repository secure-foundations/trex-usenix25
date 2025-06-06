use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use trex::{
    containers::unordered::{UnorderedMap, UnorderedSet},
    il::ExternalVariable,
    joinable_container::{Container, Index},
    log::*,
    serialize_structural::SerializableStructuralTypes,
    structural::StructuralType,
};

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Path to ground-truth variable map
    gt_vars: PathBuf,
    /// Debug level (repeat for more: 0-warn, 1-info, 2-debug, 3-trace)
    #[clap(short, long, parse(from_occurrences))]
    debug: usize,
    /// Disable terminal logging, even for high severity alerts. Strongly discouraged for normal
    /// use.
    #[clap(long)]
    debug_disable_terminal_logging: bool,
    /// Output to file; stdout if not specified.
    #[clap(short = 'o', long)]
    output: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    let vars = std::fs::read_to_string(args.gt_vars).expect("Variables file could not be read");

    // Sanity check that we have a lift-able variables file
    assert!(vars.starts_with("PROGRAM\n"));
    assert!(vars.contains("VARIABLES\n"));

    // Grab the variables section
    let vars: Vec<&str> = vars
        .trim()
        .lines()
        .skip_while(|&l| l != "VARIABLES")
        .skip(1)
        .take_while(|&l| !l.is_empty())
        .collect();

    // Parse out each variable
    let mut variable_sizes: UnorderedMap<ExternalVariable, usize> = UnorderedMap::new();
    let mut lines = vars.iter().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == "" {
            continue;
        }
        assert!(line.starts_with('\t') && !line.starts_with("\t\t"));
        let external_var = ExternalVariable(line.trim().to_owned());

        while let Some(line) = lines.peek() {
            if !line.starts_with("\t\t") {
                break;
            }
            let line = lines.next().unwrap();

            if let Some(sz) = parse_size_from_varnode(line) {
                if let Some(&vsz) = variable_sizes.get(&external_var) {
                    if vsz != sz {
                        warn!(
                            "Inconsistent variable sizing";
                            "sz" => sz,
                            "vsz" => vsz,
                            "external_var" => ?external_var,
                        );
                        variable_sizes.insert(external_var.clone(), sz.max(vsz));
                    }
                } else {
                    variable_sizes.insert(external_var.clone(), sz);
                }
            } else {
                unreachable!("Expected varnode with size information. Got line: {line:?}");
            }
        }
    }

    let unique_sizes = variable_sizes
        .values()
        .cloned()
        .collect::<UnorderedSet<usize>>();

    let mut types: Container<StructuralType> = Container::new();
    let unknown_type_with_size: UnorderedMap<usize, Index> = unique_sizes
        .iter()
        .cloned()
        .map(|sz| {
            let mut typ = StructuralType::default();
            typ.set_upper_bound_size(sz);
            typ.copy_sizes.extend(
                [1, 2, 4, 8, 16]
                    .into_iter()
                    .take_while(|&x| x < sz)
                    .chain([sz]),
            );
            (sz, types.insert(typ))
        })
        .collect();

    let res = SerializableStructuralTypes::new(
        variable_sizes
            .into_iter()
            .map(|(v, sz)| (v, *unknown_type_with_size.get(&sz).unwrap()))
            .collect(),
        unique_sizes
            .into_iter()
            .map(|sz| {
                (
                    *unknown_type_with_size.get(&sz).unwrap(),
                    format!("unknown{sz}"),
                )
            })
            .collect(),
        types,
    );

    if let Some(path) = args.output {
        write!(std::fs::File::create(path).unwrap(), "{}", res.serialize()).unwrap();
    } else {
        println!("{}", res.serialize());
    }
}

fn parse_size_from_varnode(line: &str) -> Option<usize> {
    let line = line.trim();
    assert!(line.starts_with('('));
    assert!(line.ends_with(')'));
    assert_eq!(line.chars().filter(|&c| c == ',').count(), 2);
    let mut components = line
        .trim()
        .strip_prefix('(')
        .unwrap()
        .strip_suffix(')')
        .unwrap()
        .split(',')
        .map(|x| x.trim());
    let _addrspace = components.next().unwrap();
    let _offset =
        usize::from_str_radix(components.next().unwrap().strip_prefix("0x").unwrap(), 16).unwrap();
    let size = components.next().unwrap().parse::<usize>().unwrap();
    assert_eq!(components.next(), None);

    Some(size)
}
