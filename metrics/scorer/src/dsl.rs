//! A DSL to write the evaluation rules more cleanly

use std::collections::BTreeSet;
use std::io::{Read, Seek, Write};
use std::path::Path;

use macro_rules_attribute::macro_rules_derive;
use trex::c_types::sign_normalized_c_primitives;
use trex::containers::unordered::{UnorderedMap, UnorderedSet};
use trex::structural::{AggregateSize, StructuralType};
use trex::type_rounding;

use crate::pointer_utils;
use crate::stats::LockedFile;
use crate::{utils::Var, Index, SerializableStructuralTypes, StructMayBePointer};

pub const RULES: Rule = Rule {
    property: Property::IsDefined,
    condition: Condition::BothAgree,
    if_false: (0.0, Proceed::Halt),
    if_true: (
        1.0,
        Proceed::Continue(&Rule {
            property: Property::IsCPointer,
            condition: Condition::BothAgree,
            if_false: (0.0, Proceed::Halt),
            if_true: (
                1.0,
                Proceed::Continue(&Rule {
                    property: Property::CPointerLevel,
                    condition: Condition::BothAgree,
                    if_false: (0.0, Proceed::Halt),
                    if_true: (
                        1.0,
                        Proceed::Continue(&Rule {
                            property: Property::IsCPointer,
                            condition: Condition::BothAreTrue,
                            if_true: (0.0, Proceed::ReRunRuleOnRecursiveCPointee),
                            if_false: (
                                0.0,
                                Proceed::Continue(&Rule {
                                    property: Property::IsCStruct,
                                    condition: Condition::BothAgree,
                                    if_false: (0.0, Proceed::Halt),
                                    if_true: (
                                        1.0,
                                        Proceed::Continue(&Rule {
                                            property: Property::SignIgnoredCPrimitive,
                                            condition: Condition::BothAgree,
                                            if_false: (0.0, Proceed::Halt),
                                            if_true: (
                                                1.0,
                                                Proceed::Continue(&Rule {
                                                    property: Property::CPrimitive,
                                                    condition: Condition::BothAgree,
                                                    if_false: (0.0, Proceed::Halt),
                                                    if_true: (1.0, Proceed::Halt),
                                                }),
                                            ),
                                        }),
                                    ),
                                }),
                            ),
                        }),
                    ),
                }),
            ),
        }),
    ),
};

pub struct ScoreStats {
    score: f64,
    domain_size: u64,
    failure_reasons: UnorderedMap<(Property, Condition), u64>,
    columns: Vec<(Property, Condition)>,
}

impl ScoreStats {
    pub fn new(rules: &Rule) -> Self {
        Self {
            score: Default::default(),
            domain_size: Default::default(),
            failure_reasons: Default::default(),
            columns: rules.property_conditions_product(),
        }
    }

    pub fn avg_score(&self) -> f64 {
        self.score / self.domain_size as f64
    }

    pub fn domain_size(&self) -> u64 {
        self.domain_size
    }

    pub fn total_score(&self) -> f64 {
        self.score
    }

    pub fn csv_headings(&self) -> String {
        let mut res = "AvgScore,NumVars".to_string();
        for variant in &self.columns {
            res += &format!(",{:?}-Not{:?}", variant.0, variant.1);
        }
        res
    }
    pub fn to_csv(&self) -> String {
        let mut res = format!(
            "{},{}",
            if self.domain_size > 0 {
                self.score / self.domain_size as f64
            } else {
                0.0
            },
            self.domain_size
        );
        for variant in &self.columns {
            res += &format!(",{}", self.failure_reasons.get(variant).unwrap_or(&0));
        }
        res
    }

    pub fn to_nlsv(&self) -> String {
        let mut res = String::new();
        res += &format!(
            "AvgScore: {}\n",
            if self.domain_size > 0 {
                self.score / self.domain_size as f64
            } else {
                0.0
            }
        );
        res += &format!("NumVars: {}\n", self.domain_size);
        for variant in &self.columns {
            let &val = self.failure_reasons.get(variant).unwrap_or(&0);
            if val > 0 {
                res += &format!("{:?}-Not{:?}: {}\n", variant.0, variant.1, val);
            }
        }
        res
    }

    fn failed_due_to(&mut self, property: Property, condition: Condition) {
        *self
            .failure_reasons
            .entry((property, condition))
            .or_default() += 1;
    }

    pub fn write_to_or_update_csv(&self, path: &Path, program_name: &str) {
        assert!(!program_name.is_empty());

        // Obtain a write-exclusive-locked file
        let mut file = LockedFile::new(
            std::fs::File::options()
                .read(true)
                .create(true)
                .write(true)
                .open(&path)
                .unwrap(),
        );
        let header = format!("Program,{}", self.csv_headings());

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

        // Check that programs in the file are unique; if not, terminate loudly
        {
            let mut buf = vec![];
            file.rewind().unwrap();
            file.read_to_end(&mut buf).unwrap();
            let buf = String::from_utf8(buf).unwrap();
            let progs = buf
                .lines()
                .map(|line| line.split_once(',').unwrap().0)
                .collect::<Vec<_>>();
            assert_eq!(progs.len(), progs.iter().collect::<UnorderedSet<_>>().len());
        }

        // If this program already exists in the file, then remove that line.
        {
            let mut buf = vec![];
            file.rewind().unwrap();
            file.read_to_end(&mut buf).unwrap();
            let buf = String::from_utf8(buf)
                .unwrap()
                .lines()
                .filter(|line| !line.starts_with(&format!("{:?},", program_name)))
                .collect::<Vec<_>>()
                .join("\n");
            file.rewind().unwrap();
            file.set_len(0).unwrap();
            file.flush().unwrap();
            writeln!(file, "{buf}").unwrap();
        }

        // Actually write the information related to this program.
        writeln!(file, "{:?},{}", program_name, self.to_csv()).unwrap();

        // Explicitly drop file to make sure that this shows up _after_ the write
        drop(file);
    }
}

macro_rules! variants {
    (__internal_count $var:ident) => { 1 };
    ($( #[$attr:meta] )* $vis:vis enum $T:ident {
        $($variant:ident,)*
    }) => {
        impl $T {
            #[allow(dead_code)]
            const VARIANTS: [$T; $(
                variants!(__internal_count $variant) +
            )* 0] = [
                $($T::$variant,)*
            ];
        }
    };
}

#[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[macro_rules_derive(variants)]
enum Property {
    IsDefined,
    IsCPointer,
    CPointerLevel,
    IsSTPointer,
    STPointerLevel,
    Size,
    IsCStruct,
    SignIgnoredCPrimitive,
    CPrimitive,
}

#[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[macro_rules_derive(variants)]
enum Condition {
    BothAgree,
    BothAreTrue,
}

#[derive(Debug, PartialEq, Eq)]
enum Value {
    Bool(bool),
    ResU32U32(Result<u32, u32>),
    OptAggrSize(Option<AggregateSize>),
    SetOfStr(BTreeSet<String>),
}

impl Property {
    fn eval(&self, t: Type) -> Value {
        match self {
            Property::IsDefined => Value::Bool(t.idx.is_some()),
            Property::IsCPointer => {
                Value::Bool(pointer_utils::is_pointer(t.stype(), StructMayBePointer::No))
            }
            Property::CPointerLevel => Value::ResU32U32(pointer_utils::pointer_level(
                t.idx.unwrap(),
                t.types.types(),
                StructMayBePointer::No,
            )),
            Property::IsSTPointer => Value::Bool(pointer_utils::is_pointer(
                t.stype(),
                StructMayBePointer::Yes,
            )),
            Property::STPointerLevel => Value::ResU32U32(pointer_utils::pointer_level(
                t.idx.unwrap(),
                t.types.types(),
                StructMayBePointer::Yes,
            )),
            Property::Size => Value::OptAggrSize(t.stype().aggregate_size(t.types.types(), None)),
            Property::IsCStruct => Value::Bool(!t.stype().colocated_struct_fields.is_empty()),
            Property::CPrimitive | Property::SignIgnoredCPrimitive => {
                if pointer_utils::is_pointer(t.stype(), StructMayBePointer::No) {
                    slog_scope::crit!("Assuming all pointers are the same primitive. \
                                       This code path should never actually be hit on any reasonable use of the DSL.");
                    Value::SetOfStr(std::iter::once("pointer".into()).collect())
                } else if t.stype().colocated_struct_fields.is_empty() {
                    let c_prims =
                        type_rounding::recognize_union_of_c_primitives(t.stype()).unwrap();
                    Value::SetOfStr(match self {
                        Property::CPrimitive => c_prims,
                        Property::SignIgnoredCPrimitive => {
                            let map = sign_normalized_c_primitives();
                            c_prims
                                .into_iter()
                                .map(|p| {
                                    if let Some(sip) = map.get(&p) {
                                        sip.clone()
                                    } else if p.starts_with("padding") {
                                        p
                                    } else {
                                        unreachable!(
                                            "Could not find primitive {} in sign-normalization map",
                                            p
                                        )
                                    }
                                })
                                .collect()
                        }
                        _ => unreachable!(),
                    })
                } else {
                    Value::SetOfStr(std::iter::once("struct".into()).collect())
                }
            }
        }
    }
}

#[derive(Default)]
struct Log {
    score: f64,
    log: String,
}
impl Log {
    fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Proceed {
    Halt,
    /// Repeatedly dereference C pointers until a non-pointer type is found, apply original rule on
    /// these types. Can be run only if both sides are known to be pointers.
    ReRunRuleOnRecursiveCPointee,
    Continue(&'static Rule),
    SameAsOther,
}

pub struct Rule {
    property: Property,
    condition: Condition,
    if_false: (f64, Proceed),
    if_true: (f64, Proceed),
}

impl Rule {
    fn property_conditions_product(&self) -> Vec<(Property, Condition)> {
        std::iter::once((self.property, self.condition))
            .chain(match self.if_true.1 {
                Proceed::Halt | Proceed::ReRunRuleOnRecursiveCPointee => {
                    vec![]
                }
                Proceed::Continue(r) => Rule::property_conditions_product(r),
                Proceed::SameAsOther => {
                    unreachable!("SameAsOther should only be applied to `if_false` if ever used")
                }
            })
            .chain(match self.if_false.1 {
                Proceed::Halt | Proceed::SameAsOther | Proceed::ReRunRuleOnRecursiveCPointee => {
                    vec![]
                }
                Proceed::Continue(r) => Rule::property_conditions_product(r),
            })
            .collect()
    }

    fn compute_internal(
        &self,
        inp: &Input,
        log: &mut Log,
        stats: &mut ScoreStats,
        top_level: &Self,
    ) {
        assert!(self.if_true.0 >= 0.);
        assert!(self.if_false.0 <= 0.);
        let success = match self.condition {
            Condition::BothAgree => self.property.eval(inp.test()) == self.property.eval(inp.gt()),
            Condition::BothAreTrue => {
                match (self.property.eval(inp.test()), self.property.eval(inp.gt())) {
                    (Value::Bool(true), Value::Bool(true)) => true,
                    (Value::Bool(false), Value::Bool(false)) => false,
                    (Value::Bool(_), Value::Bool(_)) => panic!(
                        "Property `{:?}` must be agreed on by both before `BothAreTrue` can be used.",
                        self.property,
                    ),
                    _ => unreachable!(),
                }
            }
        };
        let (scoremod, proceed) = if success { self.if_true } else { self.if_false };
        log.score += scoremod;
        log.log += if success { " " } else { " !" };
        log.log += &match self.condition {
            Condition::BothAgree => format!("Agree{:?}", self.property),
            Condition::BothAreTrue => format!("{:?}", self.property),
        };
        match proceed {
            Proceed::Halt => {
                if !success {
                    stats.failed_due_to(self.property, self.condition);
                    log.log += " HALT";
                }
            }
            Proceed::Continue(rule) => {
                rule.compute_internal(inp, log, stats, top_level);
            }
            Proceed::SameAsOther => {
                assert!(
                    !success,
                    "SameAsOther should only be applied to `if_false` if ever used"
                );
                match self.if_true.1 {
                    Proceed::Continue(rule) => rule.compute_internal(inp, log, stats, top_level),
                    _ => unreachable!("SameAsOther should only be used if other is `Continue`"),
                }
            }
            Proceed::ReRunRuleOnRecursiveCPointee => {
                let pointee_inp = Input::unflip(inp.flip().map(|inp| {
                    assert!(pointer_utils::is_pointer(
                        inp.stype(),
                        StructMayBePointer::No
                    ));
                    Type {
                        types: inp.types,
                        idx: Some(pointer_utils::recursive_pointee(
                            inp.idx.unwrap(),
                            inp.types.types(),
                            StructMayBePointer::No,
                        )),
                    }
                }));
                top_level.compute_internal(&pointee_inp, log, stats, top_level);
            }
        }
    }

    pub fn compute_one(&self, inp: &Input, stats: &mut ScoreStats) -> (f64, String) {
        let mut log = Log::new();

        self.compute_internal(inp, &mut log, stats, self);

        stats.score += log.score;
        stats.domain_size += 1;

        (log.score, log.log)
    }
}

pub struct TestGTPair<T> {
    pub test: T,
    pub gt: T,
}

impl<T> TestGTPair<T> {
    pub fn map<U, F: Fn(&T) -> U>(&self, f: F) -> TestGTPair<U> {
        TestGTPair {
            test: f(&self.test),
            gt: f(&self.gt),
        }
    }
}

struct Type<'a> {
    types: &'a SerializableStructuralTypes<Var>,
    idx: Option<Index>,
}
impl<'a> Type<'a> {
    fn stype(&self) -> &StructuralType {
        self.types.type_at(
            self.idx
                .expect("Type::stype must only be called on types that are known to be defined."),
        )
    }
}

pub struct Input<'a> {
    types: TestGTPair<&'a SerializableStructuralTypes<Var>>,
    idxs: TestGTPair<Option<Index>>,
}

impl<'a> Input<'a> {
    pub fn new(
        types: TestGTPair<&'a SerializableStructuralTypes<Var>>,
        idxs: TestGTPair<Option<Index>>,
    ) -> Self {
        Self { types, idxs }
    }

    fn flip(&self) -> TestGTPair<Type<'a>> {
        TestGTPair {
            test: Type {
                types: self.types.test,
                idx: self.idxs.test,
            },
            gt: Type {
                types: self.types.gt,
                idx: self.idxs.gt,
            },
        }
    }

    fn unflip(types: TestGTPair<Type<'a>>) -> Self {
        Self {
            types: TestGTPair {
                test: types.test.types,
                gt: types.gt.types,
            },
            idxs: TestGTPair {
                test: types.test.idx,
                gt: types.gt.idx,
            },
        }
    }

    fn test(&'a self) -> Type<'a> {
        self.flip().test
    }

    fn gt(&'a self) -> Type<'a> {
        self.flip().gt
    }
}
