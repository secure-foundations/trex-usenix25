use macro_rules_attribute::macro_rules_derive;
use std::{
    cmp::Ordering,
    fs::File,
    io::{Read, Seek, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

#[rustfmt::skip]
pub mod stat_cmpable {
    pub trait StatCmpable { fn to_f64(&self) -> f64; }
    impl StatCmpable for bool { fn to_f64(&self) -> f64 { *self as u64 as f64 } }
    impl StatCmpable for usize { fn to_f64(&self) -> f64 { *self as f64 } }
    impl StatCmpable for u32 { fn to_f64(&self) -> f64 { *self as f64 } }
    impl StatCmpable for u64 { fn to_f64(&self) -> f64 { *self as f64 } }
    impl StatCmpable for f64 { fn to_f64(&self) -> f64 { *self } }
}
use stat_cmpable::StatCmpable;

#[derive(Default)]
pub struct StatAboveBelow {
    pub gt_is_smaller: u64,
    pub gt_is_smaller_by: f64,
    pub gt_is_larger: u64,
    pub gt_is_larger_by: f64,
}

pub struct Cmp<T: StatCmpable> {
    pub test: T,
    pub gt: T,
}

impl StatAboveBelow {
    pub fn increment<T: StatCmpable>(&mut self, Cmp { test, gt }: Cmp<T>) -> bool {
        let gt: f64 = gt.to_f64();
        let test: f64 = test.to_f64();
        match gt.partial_cmp(&test).unwrap() {
            Ordering::Less => {
                self.gt_is_smaller += 1;
                self.gt_is_smaller_by += test - gt;
                true
            }
            Ordering::Equal => false,
            Ordering::Greater => {
                self.gt_is_larger += 1;
                self.gt_is_larger_by += gt - test;
                true
            }
        }
    }
}

macro_rules! csv_fields {
    (__internal_count $n:ident StatAboveBelow) => { 4 };
    (__internal_count $n:ident $t:ident) => { 1 };
    (__internal_list [ $($res:tt)* ]) => {
        [ $($res)* ]
    };
    (__internal_list $n:ident StatAboveBelow $($rest:ident)* [ $($res:tt)* ]) => {
        csv_fields!(__internal_list $($rest)* [
            $($res)*
            concat!(stringify!($n), "_gt_is_smaller_count"),
            concat!(stringify!($n), "_gt_is_smaller_total_dist"),
            concat!(stringify!($n), "_gt_is_larger_count"),
            concat!(stringify!($n), "_gt_is_larger_total_dist"),
        ])
    };
    (__internal_list $n:ident $ty:ident $($rest:ident)* [ $($res:tt)* ]) => {
        csv_fields!(__internal_list $($rest)* [
            $($res)*
            stringify!($n),
        ])
    };
    (__internal_write $f:expr, $n:expr, String) => {
        write!($f, "{:?}", $n).unwrap();
    };
    (__internal_write $f:expr, $n:expr, usize) => {
        write!($f, "{}", $n).unwrap();
    };
    (__internal_write $f:expr, $n:expr, StatAboveBelow) => {
        write!(
            $f, "{},{},{},{}",
            $n.gt_is_smaller,
            $n.gt_is_smaller_by,
            $n.gt_is_larger,
            $n.gt_is_larger_by,
        ).unwrap();
    };
    (__internal_write $f:expr, $n:expr, $t:ident) => {
        compile_error!(concat!("Currently unsupported printer for ", stringify!($t)))
    };
    (__internal_write_on_fields $f:expr, $n1:expr, $t1:ident, $($n:expr, $t:ident),*) => {
        csv_fields!(__internal_write $f, $n1, $t1);
        $(
            write!($f, ",").unwrap();
            csv_fields!(__internal_write $f, $n, $t);
        )*
    };
    (__internal_write_nlsv_one $f:expr, $width:expr, $name:ident, $n:expr, String) => {
        write!($f, "{name:width$}: {value:?},\n", name=stringify!($name), width=$width, value=$n).unwrap();
    };
    (__internal_write_nlsv_one $f:expr, $width:expr, $name:ident, $n:expr, usize) => {
        if $n > 0 {
            write!($f, "{name:width$}: {value},\n", name=stringify!($name), width=$width, value=$n).unwrap();
        }
    };
    (__internal_write_nlsv_one $f:expr, $width:expr, $name:ident, $n:expr, StatAboveBelow) => {
        if $n.gt_is_smaller > 0 {
            write!($f, "{name:width$}: {value},\n", name=concat!(stringify!($name), "_gt_is_smaller_count"), width=$width, value=$n.gt_is_smaller).unwrap();
            write!($f, "{name:width$}: {value},\n", name=concat!(stringify!($name), "_gt_is_smaller_total_dist"), width=$width, value=$n.gt_is_smaller_by).unwrap();
        }
        if $n.gt_is_larger > 0 {
            write!($f, "{name:width$}: {value},\n", name=concat!(stringify!($name), "_gt_is_larger_count"), width=$width, value=$n.gt_is_larger).unwrap();
            write!($f, "{name:width$}: {value},\n", name=concat!(stringify!($name), "_gt_is_larger_total_dist"), width=$width, value=$n.gt_is_larger_by).unwrap();
        }
    };
    (__internal_write_nlsv_one $f:expr, $width:expr, $name:ident, $n:expr, $t:ident) => {
        compile_error!(concat!("Currently unsupported nlsv printer for ", stringify!($t)))
    };
    (__internal_write_nlsv $f:expr, $width:expr, $($name:ident, $n:expr, $t:ident),*) => {
        $(csv_fields!(__internal_write_nlsv_one $f, $width, $name, $n, $t);)*
    };
    ($( #[$attr:meta] )* pub struct $T:ident {
        $(
            pub $fname:ident : $fty:ident,
        )*
    }) => {
        impl $T {
            const CSV_FIELD_NAMES: [&'static str;
                                    $(csv_fields!(__internal_count $fname $fty) +)* 0]
                = csv_fields!(__internal_list $($fname $fty)* [ ]);

            fn to_csv_fields(&self) -> String {
                use std::fmt::Write;
                let mut ret = String::new();
                csv_fields!(__internal_write_on_fields &mut ret, $(self.$fname, $fty),*);
                ret
            }

            pub fn to_nlsv(&self) -> String {
                use std::fmt::Write;
                let width = 0;
                let mut ret = String::new();
                csv_fields!(__internal_write_nlsv &mut ret, width, $($fname, self.$fname, $fty),*);
                ret
            }
        }
    };
}

/// Statistics computed on a particular program
#[derive(Default)]
#[macro_rules_derive(csv_fields)]
pub struct Stats {
    pub program: String,
    pub number_of_ground_truth_vars: usize,
    pub number_of_vars_with_reconstructed_types: usize,
    pub cost_c_total: usize,
    pub cost_structural_total: usize,
    pub cost_c_size: StatAboveBelow,
    pub cost_first_primitive_size: StatAboveBelow,
    pub cost_c_pointer_level: StatAboveBelow,
    pub cost_structural_pointer_level: StatAboveBelow,
    pub cost_structural_int_primitives: StatAboveBelow,
    pub cost_structural_float_primitives: StatAboveBelow,
    pub cost_structural_bool_primitives: StatAboveBelow,
    pub cost_structural_code_primitives: StatAboveBelow,
    pub cost_aggregateness: StatAboveBelow,
    pub cost_aggregate_offsets: StatAboveBelow,
}

impl Stats {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Default::default()
        }
    }

    pub fn write_to_csv(&self, path: PathBuf) {
        assert!(!self.program.is_empty());

        // Obtain a write-exclusive-locked file
        let mut file = LockedFile::new(
            std::fs::File::options()
                .read(true)
                .create(true)
                .write(true)
                .open(&path)
                .unwrap(),
        );
        let header = {
            Stats::CSV_FIELD_NAMES
                .into_iter()
                .map(|x| format!("{x:?}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        // If the header is not in the file, truncate the file entirely, and start over.
        {
            let mut buf = vec![];
            file.rewind().unwrap();
            file.read_to_end(&mut buf).unwrap();
            if !String::from_utf8(buf).unwrap().contains(&header) {
                file.rewind().unwrap();
                file.set_len(0).unwrap();
                file.flush().unwrap();
            }
        }

        // An empty file needs the header
        if file.metadata().unwrap().len() == 0 {
            writeln!(file, "{header}").unwrap();
            file.flush().unwrap();
        }

        // If this program already exists in the file, then remove that line.
        {
            let mut buf = vec![];
            file.rewind().unwrap();
            file.read_to_end(&mut buf).unwrap();
            let buf = String::from_utf8(buf)
                .unwrap()
                .lines()
                .filter(|line| !line.starts_with(&format!("{:?},", self.program)))
                .collect::<Vec<_>>()
                .join("\n");
            file.rewind().unwrap();
            file.set_len(0).unwrap();
            file.flush().unwrap();
            writeln!(file, "{buf}").unwrap();
        }

        // Actually write the information related to this program.
        writeln!(file, "{}", self.to_csv_fields()).unwrap();
    }
}

/// An RAII-guarded write-exclusive-locked file. Locks on creation, unlocks on drop.
pub struct LockedFile {
    f: File,
}
impl LockedFile {
    pub fn new(f: File) -> Self {
        use fs2::FileExt;
        f.lock_exclusive().unwrap();
        Self { f }
    }
}
impl Drop for LockedFile {
    fn drop(&mut self) {
        use fs2::FileExt;
        self.f.flush().unwrap();
        #[allow(unstable_name_collisions)]
        self.f.unlock().unwrap();
    }
}
impl Deref for LockedFile {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        &self.f
    }
}
impl DerefMut for LockedFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.f
    }
}
