use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;

use clap::Parser;

use trex::serialize_structural::{Parseable, SerializableStructuralTypes};

use trex::joinable_container::IndexMap;

use scorer::utils::Var;

/// A tool to compute "standard" scores for various tools, as measured against a ground truth
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct CliArgs {
    /// Path to ground-truth structural types file
    #[clap(long)]
    ground_truth: PathBuf,
    /// Debug level (repeat for more: 0-warn, 1-info, 2-debug, 3-trace)
    #[clap(short, long, parse(from_occurrences))]
    debug: usize,
    /// Disable terminal logging, even for high severity alerts. Strongly discouraged for normal
    /// use.
    #[clap(long)]
    debug_disable_terminal_logging: bool,
    /// Output to CSV file, if not specified, outputs to stdout.
    #[clap(long)]
    output_csv: Option<PathBuf>,
    /// Path to various structural types (`*.*-st`) files; the extensions will be used to identify
    /// tool names.
    structural_types: Vec<PathBuf>,
}

struct TestData {
    kind: String,
    data: SerializableStructuralTypes<Var>,
}

impl TestData {
    fn read_from_path(path: &PathBuf) -> Self {
        let data = scorer::utils::read_file_to_string(path);
        let extension = path
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        assert!(extension == "gtst" || extension.ends_with("-st"));
        let kind = extension.replace("-st", "").to_string();
        let data = SerializableStructuralTypes::<Var>::parse_from(&data).unwrap();
        Self { kind, data }
    }
}

#[derive(Debug)]
struct Statistics {
    kind: String,
    total_count: usize,
    true_positive: usize,
    false_positive: usize,
    false_negative: usize,
}
impl Statistics {
    fn new(kind: String) -> Self {
        Self {
            kind,
            total_count: 0,
            true_positive: 0,
            false_positive: 0,
            false_negative: 0,
        }
    }
}

fn main() {
    let args = CliArgs::parse();
    let _log_guard = slog_scope::set_global_logger(trex::log::FileAndTermDrain::new(
        args.debug,
        args.debug_disable_terminal_logging,
        true,
        None,
    ));

    let gt = TestData::read_from_path(&args.ground_truth);
    let tests = args
        .structural_types
        .iter()
        .map(TestData::read_from_path)
        .collect::<Vec<_>>();

    let mut csv: String = "tool,total,tp,fp,fn\n".into();
    for test in tests.iter() {
        let stats = analyze_test_data(&gt, test);
        csv += &format!(
            "{},{},{},{},{}\n",
            stats.kind,
            stats.total_count,
            stats.true_positive,
            stats.false_positive,
            stats.false_negative
        );
    }
    let csv = csv.trim_end_matches('\n');

    if let Some(csv_path) = args.output_csv {
        let mut file = std::fs::File::create(csv_path).unwrap();
        file.write_all(csv.as_bytes()).unwrap();
    } else {
        println!("{}", csv);
    }
}

fn analyze_test_data(gt: &TestData, test: &TestData) -> Statistics {
    let mut stats = Statistics::new(test.kind.clone());
    for (var, gti) in gt.data.var_type_iter() {
        let tdoi = test.data.index_of_type_for(var).filter(|&tdi| {
            // Some tools (e.g., Ghidra) have no way of marking a variable as "unknown", which
            // means if it knows a variable name, and is unable to give it a type, it will mark
            // the type as `undefined`. For TRex, we have a separate notion of `undefined` and
            // "unknown" (where we output nothing). By ignoring `undefined`s, we thus bring both
            // into sync.
            //
            // If something is marked as undefined, we consider it as the reconstruction tool
            // giving up. Note that this does _not_ mean that `undefined1`/`undefined2`/... is
            // marked as giving up. Instead, only `undefined` is marked as giving up.
            !trex::c_types::is_undefined_padding(test.data.types().get(tdi))
        });
        let tdi = if let Some(tdi) = tdoi {
            tdi
        } else if trex::c_types::is_undefined_padding(gt.data.types().get(gti)) {
            // If both ground truth and test data are marked as `undefined`, that is actually a true
            // positive. We handle this separately here because there are two kinds of `undefined`
            // (see above), and we don't want to penalize any tool for handling it differently from
            // the other.
            stats.total_count += 1;
            stats.true_positive += 1;
            continue;
        } else {
            stats.total_count += 1;
            stats.false_negative += 1;
            continue;
        };

        // We now compute the equivalent C type, literally as a string strings, and look for whether
        // they match. Since we provide no names, and are doing this via a fully new container, all
        // alpha conversion issues are automatically accounted for.

        // Set up the index clones
        let mut gti = gti.clone();
        let mut tdi = tdi.clone();
        // Use them as roots to get entirely new type sets.
        let subset_gt = gt.data.types().deep_clone([&mut gti].into_iter());
        let subset_td = test.data.types().deep_clone([&mut tdi].into_iter());
        // Set up varmaps
        let gt_varmap = [(Var::from(String::from("x")), gti)]
            .into_iter()
            .collect::<BTreeMap<Var, _>>();
        let td_varmap = [(Var::from(String::from("x")), tdi)]
            .into_iter()
            .collect::<BTreeMap<Var, _>>();
        // Obtain the serializable forms
        let ser_gt = SerializableStructuralTypes::new(gt_varmap, IndexMap::new(), subset_gt);
        let ser_td = SerializableStructuralTypes::new(td_varmap, IndexMap::new(), subset_td);
        // Get the C-type printable form
        let ct_gt = trex::c_type_printer::PrintableCTypes::new(&ser_gt);
        let ct_td = trex::c_type_printer::PrintableCTypes::new(&ser_td);
        // Serialize them to strings and compare
        let c_gt = ct_gt.to_string();
        let c_td = ct_td.to_string();
        stats.total_count += 1;
        if c_gt != c_td {
            stats.false_positive += 1;
            continue;
        } else {
            stats.true_positive += 1;
        }
    }
    stats
}
