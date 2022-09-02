use crate::simple_value::StringType;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimpleType(pub StringType);

// CompactKey is used for CompactValue, as the keys of objects
// CompactKeys are strings, attached with type information about its corresponding values
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompactKey {
    // If the string has no type attached, use StringType(s, Vec::new())
    // StringType(StringType)
    StringType(StringType, Vec<SimpleType>),
    Transient(Vec<SimpleType>),
}

impl From<StringType> for CompactKey {
    fn from(s: StringType) -> Self {
        Self::StringType(s, Vec::new())
    }
}

impl CompactKey {
    pub fn choose_lang(self, langs: &Vec<String>) -> String {
        match self {
            CompactKey::StringType(key, types) => {
                if types.len() == 0 {
                    key.choose_lang(langs)
                } else {
                    format!(
                        "{} [{}]",
                        key.choose_lang(langs),
                        types
                            .into_iter()
                            .map(|t| t.0.choose_lang(langs))
                            .collect::<Vec<String>>()
                            .join(", "),
                    )
                    .into()
                }
            }
            CompactKey::Transient(types) => format!(
                "[{}]",
                types
                    .into_iter()
                    .map(|t| t.0.choose_lang(langs))
                    .collect::<Vec<String>>()
                    .join(", "),
            ),
        }
    }
}
