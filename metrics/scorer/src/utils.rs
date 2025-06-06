use std::io::Read;

use trex::log::crit;

use trex::containers::unordered::UnorderedMap;

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone)]
pub struct Var(String);
impl Var {
    pub fn inner(&self) -> &str {
        &self.0
    }
}
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

pub fn gt_vars_to_test_vars(
    gt_vars: &UnorderedMap<Var, Var>,
    test_vars: &UnorderedMap<Var, Var>,
) -> UnorderedMap<Var, Var> {
    let rev = test_vars
        .iter()
        .map(|(k, v)| (v, k))
        .collect::<UnorderedMap<_, _>>();
    gt_vars
        .iter()
        .map(|(k, v)| {
            let vv: Var = (*rev.get(&v).unwrap_or(&k)).clone();
            (k.clone(), vv)
        })
        .collect::<UnorderedMap<Var, Var>>()
}

pub fn parse_vars(vars: &str) -> UnorderedMap<Var, Var> {
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
    let mut res: UnorderedMap<Var, Var> = Default::default();
    let mut lines = vars.iter().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == "" {
            continue;
        }
        assert!(line.starts_with('\t') && !line.starts_with("\t\t"));
        let external_var = line.trim().to_owned();
        let func_name = line.split_once('@').unwrap().1.trim();

        let mut internal_vars = vec![];
        while let Some(line) = lines.peek() {
            if !line.starts_with("\t\t") {
                break;
            }

            if let Some(v) = parse_varnode(func_name, lines.next().unwrap(), &external_var) {
                internal_vars.push(v.0);
            }
        }

        let prev = res.insert(Var(external_var.clone()), Var(internal_vars.join("+")));
        if prev.is_some() {
            crit!("Found external var again";
                  "external_var" => ?external_var,
                  "old" => ?prev,
                  "new" => ?internal_vars.join("+"));
        }
    }

    res
}

fn parse_varnode(func_name: &str, s: &str, _extvar: &str) -> Option<Var> {
    let mut res = String::new();
    res += func_name.trim();
    res += s.trim();
    Some(Var(res))
}

pub fn read_file_to_string(path: &std::path::PathBuf) -> String {
    let mut s = String::new();
    std::fs::File::open(path)
        .map_err(|e| format!("Could not open file: {:?}. Error: {}", path, e))
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

/// `named_bool!{tau}` declares a new enum `tau` which auto-coerces to `bool`. Allows for more
/// descriptive names to be used in (say) arguments, rather than just booleans.
#[macro_export]
macro_rules! named_bool {
    ($vis:vis $ty: ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $ty {
            Yes,
            No,
        }
        impl $ty {
            #[allow(dead_code)]
            fn new(b: bool) -> Self {
                match b {
                    true => $ty::Yes,
                    false => $ty::No,
                }
            }
            #[allow(dead_code)]
            fn yes(&self) -> bool {
                matches!(self, Self::Yes)
            }
        }
        impl Deref for $ty {
            type Target = bool;
            fn deref(&self) -> &Self::Target {
                match self {
                    $ty::Yes => &true,
                    $ty::No => &false,
                }
            }
        }
    };
}
