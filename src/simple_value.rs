use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

// We store human readable labels (map {natural language ZID: label}) along with the ZID
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelledNode {
    readable_labels: BTreeMap<String, String>,
    z_label: String,
}

impl LabelledNode {
    pub fn from(readable_labels: BTreeMap<String, String>, z_label: String) -> Self {
        Self {
            readable_labels,
            z_label,
        }
    }

    pub fn choose_lang(self, langs: &Vec<String>) -> String {
        format!(
            "{}: {}",
            self.z_label,
            langs
                .iter()
                .find_map(|lang| self.readable_labels.get(lang))
                .unwrap_or(
                    self.readable_labels
                        .iter()
                        .map(|(_lang, label)| label)
                        .next()
                        .unwrap_or(&"<no label>".to_string())
                )
                .clone()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StringType {
    String(String),
    LabelledNode(LabelledNode),
}

impl StringType {
    pub fn is_labelled(&self, label: &str) -> bool {
        match self {
            Self::String(s) => s == label,
            StringType::LabelledNode(n) => n.z_label == label,
        }
    }

    pub fn choose_lang(self, langs: &Vec<String>) -> String {
        match self {
            StringType::String(s) => s,
            StringType::LabelledNode(n) => n.choose_lang(langs),
        }
    }
}

impl From<String> for StringType {
    fn from(s: String) -> Self {
        StringType::String(s)
    }
}

// we restrict possible variants when converting from Value, dropping Null, Bool, and Number
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SimpleValue {
    StringType(StringType),
    Array(Vec<SimpleValue>),
    Object(BTreeSet<(StringType, SimpleValue)>),
}

impl From<StringType> for SimpleValue {
    fn from(k: StringType) -> Self {
        SimpleValue::StringType(k)
    }
}

impl SimpleValue {
    pub fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            SimpleValue::StringType(s) => s.choose_lang(langs).into(),
            SimpleValue::Array(v) => {
                Value::Array(v.into_iter().map(|x| x.choose_lang(langs)).collect())
            }
            SimpleValue::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k.choose_lang(langs), v.choose_lang(langs)))
                    .collect(),
            ),
        }
    }
}
