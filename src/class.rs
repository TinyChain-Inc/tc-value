use hr_id::label;
use pathlink::{PathBuf, PathSegment};

const PREFIX: [&str; 3] = ["state", "scalar", "value"];

fn prefix_path() -> PathBuf {
    PREFIX
        .iter()
        .fold(PathBuf::new(), |path, segment| path.append(label(segment)))
}

/// Value type paths (URI-based type declarations).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    None,
    Number,
}

impl ValueType {
    pub fn path(&self) -> PathBuf {
        let prefix = prefix_path();
        match self {
            ValueType::None => prefix.append(label("none")),
            ValueType::Number => prefix.append(label("number")),
        }
    }

    pub fn from_path(path: &[PathSegment]) -> Option<Self> {
        if path.len() != PREFIX.len() + 1 {
            return None;
        }

        if !path
            .iter()
            .zip(PREFIX.iter())
            .all(|(segment, prefix)| segment.as_str() == *prefix)
        {
            return None;
        }

        match path[PREFIX.len()].as_str() {
            "none" => Some(ValueType::None),
            "number" => Some(ValueType::Number),
            _ => None,
        }
    }
}
