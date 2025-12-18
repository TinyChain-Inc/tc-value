use std::fmt;

use number_general::{ComplexType, FloatType, IntType, NumberType, UIntType};
use pathlink::{label, path_label, Label, PathBuf, PathLabel, PathSegment};

const NUMBER_PREFIX: PathLabel = path_label(&["state", "scalar", "value", "number"]);

const LABEL_BOOL: Label = label("bool");
const LABEL_COMPLEX: Label = label("complex");
const LABEL_FLOAT: Label = label("float");
const LABEL_INT: Label = label("int");
const LABEL_UINT: Label = label("uint");
const LABEL_8: Label = label("8");
const LABEL_16: Label = label("16");
const LABEL_32: Label = label("32");
const LABEL_64: Label = label("64");

/// Minimal TinyChain class markers for transitional crates.
pub trait Class: fmt::Debug + Sized {}

/// Native (Rust) implementations of TinyChain classes.
pub trait NativeClass: Class {
    /// Attempt to resolve this class from a fully qualified path.
    fn from_path(path: &[PathSegment]) -> Option<Self>
    where
        Self: Sized;

    /// Return the fully qualified path of this class.
    fn path(&self) -> PathBuf;
}

pub fn number_type_from_path(path: &[PathSegment]) -> Option<NumberType> {
    let prefix_len = NUMBER_PREFIX.len();

    if path.len() < prefix_len {
        return None;
    }

    if path[..prefix_len] != NUMBER_PREFIX[..] {
        return None;
    }

    number_type_from_suffix(&path[prefix_len..])
}

fn number_type_from_suffix(suffix: &[PathSegment]) -> Option<NumberType> {
    match suffix {
        [] => Some(NumberType::Number),
        [seg] => match seg.as_str() {
            "bool" => Some(NumberType::Bool),
            "complex" => Some(NumberType::Complex(ComplexType::Complex)),
            "float" => Some(NumberType::Float(FloatType::Float)),
            "int" => Some(NumberType::Int(IntType::Int)),
            "uint" => Some(NumberType::UInt(UIntType::UInt)),
            _ => None,
        },
        [category, size] => match (category.as_str(), size.as_str()) {
            ("complex", "32") => Some(NumberType::Complex(ComplexType::C32)),
            ("complex", "64") => Some(NumberType::Complex(ComplexType::C64)),
            ("float", "32") => Some(NumberType::Float(FloatType::F32)),
            ("float", "64") => Some(NumberType::Float(FloatType::F64)),
            ("int", "8") => Some(NumberType::Int(IntType::I8)),
            ("int", "16") => Some(NumberType::Int(IntType::I16)),
            ("int", "32") => Some(NumberType::Int(IntType::I32)),
            ("int", "64") => Some(NumberType::Int(IntType::I64)),
            ("uint", "8") => Some(NumberType::UInt(UIntType::U8)),
            ("uint", "16") => Some(NumberType::UInt(UIntType::U16)),
            ("uint", "32") => Some(NumberType::UInt(UIntType::U32)),
            ("uint", "64") => Some(NumberType::UInt(UIntType::U64)),
            _ => None,
        },
        _ => None,
    }
}

pub fn number_type_path(dtype: &NumberType) -> PathBuf {
    let prefix = PathBuf::from(NUMBER_PREFIX);
    match dtype {
        NumberType::Bool => prefix.append(LABEL_BOOL),
        NumberType::Complex(ct) => append_complex_path(prefix, *ct),
        NumberType::Float(ft) => append_float_path(prefix, *ft),
        NumberType::Int(it) => append_int_path(prefix, *it),
        NumberType::UInt(ut) => append_uint_path(prefix, *ut),
        NumberType::Number => prefix,
    }
}

// TODO: When `number-general` lives in this workspace, replace these bit-size
// suffix matchers with a shared helper so the path construction logic stays in
// one place.
fn append_complex_path(mut prefix: PathBuf, ct: ComplexType) -> PathBuf {
    prefix = prefix.append(LABEL_COMPLEX);
    match ct {
        ComplexType::Complex => prefix,
        ComplexType::C32 => prefix.append(LABEL_32),
        ComplexType::C64 => prefix.append(LABEL_64),
    }
}

fn append_float_path(mut prefix: PathBuf, ft: FloatType) -> PathBuf {
    prefix = prefix.append(LABEL_FLOAT);
    match ft {
        FloatType::Float => prefix,
        FloatType::F32 => prefix.append(LABEL_32),
        FloatType::F64 => prefix.append(LABEL_64),
    }
}

fn append_int_path(mut prefix: PathBuf, it: IntType) -> PathBuf {
    prefix = prefix.append(LABEL_INT);
    match it {
        IntType::Int => prefix,
        IntType::I8 => prefix.append(LABEL_8),
        IntType::I16 => prefix.append(LABEL_16),
        IntType::I32 => prefix.append(LABEL_32),
        IntType::I64 => prefix.append(LABEL_64),
    }
}

fn append_uint_path(mut prefix: PathBuf, ut: UIntType) -> PathBuf {
    prefix = prefix.append(LABEL_UINT);
    match ut {
        UIntType::UInt => prefix,
        UIntType::U8 => prefix.append(LABEL_8),
        UIntType::U16 => prefix.append(LABEL_16),
        UIntType::U32 => prefix.append(LABEL_32),
        UIntType::U64 => prefix.append(LABEL_64),
    }
}
