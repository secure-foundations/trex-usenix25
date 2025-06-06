use std::io::Read;
use std::path::PathBuf;

use clap::Parser;

use trex::{
    c_types::{BuiltIn, CType, CTypes, STypes},
    containers::unordered::UnorderedMap,
    log::*,
    serialize_structural::SerializableStructuralTypes,
};

/// A tool to convert C types into structural types
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Path to an exported types file, exported using `TypesExporter.java`
    file: PathBuf,
    /// Debug level (repeat for more: 0-warn, 1-info, 2-debug, 3-trace)
    #[clap(short, long, parse(from_occurrences))]
    debug: usize,
    /// Disable terminal logging, even for high severity alerts. Strongly discouraged for normal
    /// use.
    #[clap(long)]
    debug_disable_terminal_logging: bool,
}

fn main() {
    let args = Args::parse();
    let data = {
        let mut file = std::fs::File::open(&args.file).unwrap();
        let mut data = String::new();
        file.read_to_string(&mut data).unwrap();
        data
    };
    let _log_guard = slog_scope::set_global_logger(FileAndTermDrain::new(
        args.debug,
        args.debug_disable_terminal_logging,
        true,
        None,
    ));

    let var_info_lines: Vec<_> = data
        .trim()
        .lines()
        .skip_while(|&l| l != "VARIABLE_TYPES")
        .skip(1)
        .take_while(|&l| l != "TYPE_INFORMATION")
        .collect();

    let type_info_lines: Vec<_> = data
        .trim()
        .lines()
        .skip_while(|&l| l != "TYPE_INFORMATION")
        .skip(1)
        .collect();

    // println!("{}", type_info_lines.join("\n"));
    let ctypes = ctypes_parse_from(&type_info_lines);

    let vartypes = VTypes::parse_from(&var_info_lines);

    // dbg!(&ctypes);

    let stypes = vartypes.to_serializable_structural(ctypes.to_structural());

    println!("{}", stypes.serialize());
}

fn tabsplit(line: &str, n: usize) -> &str {
    line.trim().split('\t').nth(n).unwrap()
}

fn ctype_parse_from(name: &str, lines: &[&str]) -> CType {
    match tabsplit(lines[0], 0) {
        "DefaultDataType" => {
            assert_eq!(name, "undefined");
            CType::BuiltIn(BuiltIn::Undefined)
        }
        "BuiltInDataType" => CType::BuiltIn(match name {
            "void" => BuiltIn::Void,
            "bool" => BuiltIn::Bool,
            "char" => BuiltIn::Char,
            // XXX: Weird that Ghidra uses `string` to mean `char` in ultra rare instances
            "string" => BuiltIn::Char,
            "uchar" => BuiltIn::UChar,
            "byte" => BuiltIn::UChar,
            "sbyte" => BuiltIn::Char,
            "wchar_t" => BuiltIn::WCharT,
            "short" => BuiltIn::Short,
            "ushort" => BuiltIn::UShort,
            "word" => BuiltIn::UShort,
            "sword" => BuiltIn::Short,
            "int" => BuiltIn::Int,
            "uint" => BuiltIn::Uint,
            "dword" => BuiltIn::Uint,
            "sdword" => BuiltIn::Int,
            "long" => BuiltIn::Long,
            "ulong" => BuiltIn::ULong,
            "qword" => BuiltIn::ULong,
            "sqword" => BuiltIn::Long,
            // XXX: Weird that we get `longlong` and `long` to mean same thing
            "ulonglong" => BuiltIn::ULong,
            "longlong" => BuiltIn::Long,
            "uint16" => BuiltIn::ULong,
            "float" => BuiltIn::Float,
            "float4" => BuiltIn::Float,
            "double" => BuiltIn::Double,
            "float8" => BuiltIn::Double,
            "longdouble" => BuiltIn::LongDouble,
            "float10" => BuiltIn::LongDouble, // Is this ok?
            "undefined1" => BuiltIn::Undefined1,
            "undefined2" => BuiltIn::Undefined2,
            "undefined3" => BuiltIn::Undefined4, // Is this ok?
            "undefined4" => BuiltIn::Undefined4,
            "undefined5" => BuiltIn::Undefined8, // Is this ok?
            "undefined6" => BuiltIn::Undefined8, // Is this ok?
            "undefined7" => BuiltIn::Undefined8, // Is this ok?
            "undefined8" => BuiltIn::Undefined8,
            b => unreachable!("Unknown builtin {} {:?}", b, lines),
        }),
        "Union" => CType::Union(lines[1..].iter().map(|l| tabsplit(l, 2).into()).collect()),
        "Structure" => CType::Struct(
            lines[1..]
                .iter()
                .map(|l| (tabsplit(l, 1).parse().unwrap(), tabsplit(l, 2).into()))
                .collect(),
        ),
        "TypeDef" => CType::TypeDef(tabsplit(lines[0], 1).into()),
        "BitFieldDataType" => {
            debug!(
                "Currently unsupported BitFieldDataType. \
                 Expanding to full size via a typedef instead."
            );
            CType::TypeDef(tabsplit(lines[0], 1).into())
        }
        "Pointer" => CType::Pointer(
            tabsplit(lines[0], 1).parse().unwrap(),
            tabsplit(lines[0], 2).into(),
        ),
        "Enum" => CType::Enum(
            tabsplit(lines[0], 1).parse().unwrap(),
            lines[1..]
                .iter()
                .map(|l| tabsplit(l, 1).parse().unwrap())
                .collect(),
        ),
        "Array" => CType::FixedSizeArray(
            tabsplit(lines[0], 1).into(),
            tabsplit(lines[0], 2).parse().unwrap(),
            tabsplit(lines[0], 3).parse().unwrap(),
        ),
        "FunctionDefinition" => {
            assert_eq!(tabsplit(lines[0], 1), "CURRENTLY_EXPORT_UNIMPLEMENTED");
            CType::Code
        }
        t => unreachable!("Unknown {} {:?}", t, lines),
    }
}

fn ctypes_parse_from(lines: &[&str]) -> CTypes {
    let mut ctypes = UnorderedMap::new();

    let mut start = 0;
    while start < lines.len() {
        assert_eq!(lines[start].chars().nth(0).unwrap(), '\t');
        assert_ne!(lines[start].chars().nth(1).unwrap(), '\t');

        let name = &lines[start][1..];
        let desc_len = lines[start + 1..]
            .iter()
            .take_while(|l| l.starts_with("\t\t"))
            .count();
        let end = start + 1 + desc_len;
        let ctype = ctype_parse_from(name, &lines[start + 1..end]);
        start = end;

        if ctypes.contains_key(name) {
            if ctypes.get(name).unwrap() != &ctype {
                warn!(
                    "Found a definition again in the input, and there is disagreement. Ignoring new.";
                    "name" => name,
                    "old" => ?ctypes.get(name).unwrap(),
                    "new" => ?ctype,
                );
            } else {
                info!(
                    "Found a definition again in the input, but it matches the old definition.";
                    "name" => name,
                );
            }
        }
        ctypes.insert(name.into(), ctype);
    }

    CTypes { ctypes }
}

#[derive(Debug)]
struct VTypes {
    vtypes: UnorderedMap<String, String>,
}

impl VTypes {
    fn to_serializable_structural(&self, stypes: STypes) -> SerializableStructuralTypes<Var> {
        SerializableStructuralTypes::new(
            self.vtypes
                .iter()
                .filter_map(|(k, v)| {
                    if let Some(&ti) = stypes.type_map.get(v) {
                        Some((k.clone().into(), ti))
                    } else {
                        error!("Variable did not have a type in map"; "var" => k);
                        None
                    }
                })
                .collect(),
            stypes
                .type_map
                .iter()
                .map(|(k, v)| (*v, mangling::mangle(k.bytes())))
                .collect(),
            stypes.types,
        )
    }
}

impl VTypes {
    fn parse_from(lines: &[&str]) -> Self {
        let mut ret = Self {
            vtypes: Default::default(),
        };

        for line in lines.iter() {
            if line.trim() == "" {
                continue;
            }
            let (v, t) = if let Some(vt) = line.trim().split_once('\t') {
                vt
            } else {
                crit!("Line did not have a tab character"; "line" => ?line);
                continue;
            };
            let prev = ret.vtypes.insert(v.to_owned(), t.to_owned());
            if let Some(prev) = prev {
                if prev == t {
                    info!("Re-inserting same type for variable"; "type" => t, "variable" => v);
                } else {
                    warn!(
                        "Trying to insert new type where previous type existed. Ignoring old and using new type.";
                        "new" => t,
                        "old" => prev,
                        "var" => v,
                    );
                }
            }
        }

        ret
    }
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Var(String);
impl From<String> for Var {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl std::fmt::Display for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        assert!(!self.0.contains('\t'));
        write!(f, "{}", self.0)
    }
}
impl trex::serialize_structural::Parseable for Var {
    fn parse_from(s: &str) -> Option<Self> {
        if s.contains('\t') {
            None
        } else {
            Some(Self(s.to_owned()))
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct DemangledVar(String);
impl From<String> for DemangledVar {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl std::fmt::Display for DemangledVar {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", mangling::mangle(self.0.bytes()))
    }
}
impl trex::serialize_structural::Parseable for DemangledVar {
    fn parse_from(s: &str) -> Option<Self> {
        Some(Self(
            String::from_utf8(mangling::demangle(s).unwrap()).unwrap(),
        ))
    }
}
