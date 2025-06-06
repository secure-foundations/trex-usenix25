use once_cell::sync::Lazy;
use std::sync::Mutex;
use trex::containers::unordered::UnorderedSet;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Out {
    Stdout,
    Stderr,
}

static MESSAGES: Lazy<Mutex<UnorderedSet<(Out, String)>>> =
    Lazy::new(|| Mutex::new(UnorderedSet::new()));
#[allow(dead_code)]

pub(crate) fn eprint_once_aux(s: String) {
    if MESSAGES.lock().unwrap().insert((Out::Stderr, s.clone())) {
        eprint!("{}", s);
    }
}

#[allow(dead_code)]
pub(crate) fn print_once_aux(s: String) {
    if MESSAGES.lock().unwrap().insert((Out::Stdout, s.clone())) {
        print!("{}", s);
    }
}

#[allow(dead_code)]
pub(crate) fn dbg_once_aux(key: String, value: String) {
    if MESSAGES.lock().unwrap().insert((Out::Stderr, key.clone())) {
        eprintln!("{} = {}", key, value);
    }
}

#[allow(unused_macros)]
macro_rules! eprint_once {
    ($($t:tt)*) => {
        crate::term_once::eprint_once_aux(format!($($t)*));
    }
}

#[allow(unused_macros)]
macro_rules! eprintln_once {
    ($($t:tt)*) => {
        crate::term_once::eprint_once_aux(format!("{}\n", format_args!($($t)*)));
    }
}

#[allow(unused_macros)]
macro_rules! print_once {
    ($($t:tt)*) => {
        crate::term_once::print_once_aux(format!($($t)*));
    }
}

#[allow(unused_macros)]
macro_rules! println_once {
    ($($t:tt)*) => {
        crate::term_once::print_once_aux(format!("{}\n", format_args!($($t)*)));
    }
}

#[allow(unused_macros)]
macro_rules! dbg_once {
    ($($t:tt)*) => {
        let temp = $($t)*;
        crate::term_once::dbg_once_aux(
            format!("[{}:{}:{}] {}", file!(), line!(), column!(), stringify!($($t)*)),
            format!("{:?}", temp),
        );
        #[allow(path_statements)]
        {
            temp
        }
    };
}
