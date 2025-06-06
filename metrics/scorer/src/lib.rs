#[macro_use]
pub mod term_once;
#[macro_use]
pub mod utils;
pub mod dsl;
pub mod pointer_utils;
pub mod stats;

use trex::{joinable_container::Index, serialize_structural::SerializableStructuralTypes};

use crate::pointer_utils::StructMayBePointer;
