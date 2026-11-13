use std::fmt;

use number_general::{ComplexType, FloatType, IntType, NumberType, UIntType};
use pathlink::{label, path_label, PathBuf, PathLabel, PathSegment};

const NUMBER_PREFIX: PathLabel = path_label(&["state", "scalar", "value", "number"]);

/// Minimal TinyChain class markers.
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
    let prefix_len = NUMBER_PREFIX[..].len();

    if path.len() < prefix_len {
        return None;
    }

    if path[..prefix_len] != NUMBER_PREFIX[..] {
        return None;
    }

    number_type_from_suffix(&path[prefix_len..])
}

// Keep parsing and canonical formatting on the same family/width mapping.
macro_rules! number_paths {
    ($($family:ident($class:ident::$any:ident) => $name:literal {
        $($width:literal => $variant:ident),+ $(,)?
    }),+ $(,)?) => {
        fn number_type_from_suffix(suffix: &[PathSegment]) -> Option<NumberType> {
            match suffix {
                [] => Some(NumberType::Number),
                [name] => match name.as_str() {
                    "bool" => Some(NumberType::Bool),
                    $($name => Some(NumberType::$family($class::$any)),)+
                    _ => None,
                },
                [family, width] => match (family.as_str(), width.as_str()) {
                    $($(($name, $width) => Some(NumberType::$family($class::$variant)),)+)+
                    _ => None,
                },
                _ => None,
            }
        }

        pub fn number_type_path(dtype: &NumberType) -> PathBuf {
            let prefix = PathBuf::from(NUMBER_PREFIX);
            match dtype {
                NumberType::Number => prefix,
                NumberType::Bool => prefix.append(label("bool")),
                $(NumberType::$family(dtype) => {
                    let prefix = prefix.append(label($name));
                    match dtype {
                        $class::$any => prefix,
                        $($class::$variant => prefix.append(label($width)),)+
                    }
                },)+
            }
        }
    };
}

number_paths! {
    Complex(ComplexType::Complex) => "complex" { "32" => C32, "64" => C64 },
    Float(FloatType::Float) => "float" { "32" => F32, "64" => F64 },
    Int(IntType::Int) => "int" { "8" => I8, "16" => I16, "32" => I32, "64" => I64 },
    UInt(UIntType::UInt) => "uint" { "8" => U8, "16" => U16, "32" => U32, "64" => U64 },
}
