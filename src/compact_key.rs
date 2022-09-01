use crate::simple_value::StringType;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimpleType(pub StringType);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompactKey {
    StringType(StringType),
    TypedLabelledNode(StringType, SimpleType),
    Transient(SimpleType),
}

impl From<StringType> for CompactKey {
    fn from(s: StringType) -> Self {
        Self::StringType(s)
    }
}

impl CompactKey {
    pub fn choose_lang(self, langs: &Vec<String>) -> String {
        match self {
            CompactKey::StringType(n) => n.choose_lang(langs),
            CompactKey::TypedLabelledNode(key, typ) => {
                format!("{} [{}]", key.choose_lang(langs), typ.0.choose_lang(langs),).into()
            }
            CompactKey::Transient(typ) => format!("[{}]", typ.0.choose_lang(langs),),
        }
    }
}
