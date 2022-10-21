//! Contains definitions for variables and constant values.

#![allow(dead_code)]

pub use crate::values_utils::DummyFormatter; // Don't understand why we need to to this
pub use crate::values_utils::*;
use core::hash::Hash;
use macros::{generate_index_type, EnumAsGetters, EnumIsA, VariantIndexArity, VariantName};
use serde::Serialize;

// We need to manipulate a lot of indices for the types, variables, definitions,
// etc. In order not to confuse them, we define an index type for every one of
// them (which is just a struct with a unique usize field), together with some
// utilities like a fresh index generator. Those structures and utilities are
// generated by using macros.
// TODO: move
generate_index_type!(VarId);

/// Constant value
#[derive(Debug, PartialEq, Eq, Clone, VariantName, EnumIsA, EnumAsGetters, Serialize)]
pub enum ConstantValue {
    Scalar(ScalarValue),
    Bool(bool),
    Char(char),
    String(String),
}

/// It might be a good idea to use a structure:
/// `{ value: ??; int_ty: IntegerTy; }`
/// But then it is not obvious how to naturally store the integer (for instance,
/// in OCaml it is possible to use big integers).
///
/// Also, we don't automatically derive the serializer, because it would serialize
/// the values to integers, leading to potential overflows: we implement a custom
/// serialization, which serializes the values to strings.
#[derive(
    Debug, PartialEq, Eq, Copy, Clone, EnumIsA, EnumAsGetters, VariantName, VariantIndexArity, Hash,
)]
pub enum ScalarValue {
    Isize(isize),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Usize(usize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
}
