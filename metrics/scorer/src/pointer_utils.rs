use std::ops::Deref;

use trex::{
    joinable_container::{Container, Index, IndexSet},
    structural::StructuralType,
};

named_bool! {pub StructMayBePointer} // XXX: What to do about non-struct-arrays?

pub fn is_pointer(this: &StructuralType, struct_may_be_pointer: StructMayBePointer) -> bool {
    if !this.pointer_to.is_some() {
        return false;
    }
    if *struct_may_be_pointer {
        this.pointer_to.is_some()
    } else {
        if !this.colocated_struct_fields.is_empty() {
            return false;
        }
        // XXX: What to do about non-struct arrays?
        this.pointer_to.is_some()
    }
}

/// Get the number of pointer dereferences that need to be done to a type to reach a primitive
/// non-pointer type.
///
/// If a type is fully recursive, returns an `Err` with the number of dereferences required to
/// hit the recursion.
pub fn pointer_level(
    idx: Index,
    types: &Container<StructuralType>,
    struct_may_be_pointer: StructMayBePointer,
) -> Result<u32, u32> {
    let mut idx = idx;
    let mut seen = IndexSet::new();
    let mut level = 0;

    while !seen.contains(idx) {
        seen.insert(idx);
        let this = types.get(idx);
        if is_pointer(this, struct_may_be_pointer) {
            level += 1;
            idx = this.pointer_to.unwrap();
        } else {
            return Ok(level);
        }
    }

    Err(level)
}

/// Same as [`pointer_level`] but for fully recursive types, returns the number of dereferences
/// required to hit the recursion.
pub fn pointer_level_upto_recursion(
    idx: Index,
    types: &Container<StructuralType>,
    struct_may_be_pointer: StructMayBePointer,
) -> u32 {
    match pointer_level(idx, types, struct_may_be_pointer) {
        Ok(v) | Err(v) => v,
    }
}

/// Get the recursive pointee (i.e., dereference [`pointer_level`] number of times)
pub fn recursive_pointee(
    idx: Index,
    types: &Container<StructuralType>,
    struct_may_be_pointer: StructMayBePointer,
) -> Index {
    if *struct_may_be_pointer {
        unimplemented!()
    }
    let mut idx = idx;
    for _ in 0..pointer_level(idx, types, struct_may_be_pointer).unwrap() {
        idx = types.get(idx).pointer_to.unwrap();
    }
    idx
}
