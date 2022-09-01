use serde_json::{json, Value};

use crate::compact_key::{CompactKey, SimpleType};
use crate::simple_value::StringType;
use crate::typed_form::{Type, TypedForm};

type IntermediateObjectType = std::collections::BTreeSet<(StringType, IntermediateForm)>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntermediateType {
    Simple(StringType),
    WithArgs(StringType, IntermediateObjectType),
}

impl From<Type> for IntermediateType {
    fn from(t: Type) -> Self {
        match t {
            Type::Simple(s) => Self::Simple(s),
            Type::WithArgs(typ, args) => {
                Self::WithArgs(typ, args.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

impl IntermediateType {
    pub fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            Self::Simple(k) => k.choose_lang(langs).into(),
            Self::WithArgs(typ, args) => {
                json!({"type": typ.choose_lang(langs), "args": Value::Object(
                    args.into_iter().map(|(k,v)| (k.choose_lang(langs).into(), v.choose_lang(langs))).collect()
                )})
            }
        }
    }
}

// Compared to TypedForm, we allow more possible variants
// - StringType became CompactKey
// Tranformations (e.g. compress_monolingual()) are easy to do in IntermediateForm
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntermediateForm {
    KeyType(CompactKey),
    Array(IntermediateType, Vec<IntermediateForm>),
    Object(IntermediateObjectType),
    // in the intermediate form, we pull the type of objects out
    TypedObject(IntermediateType, IntermediateObjectType),
}

impl From<TypedForm> for IntermediateForm {
    fn from(val: TypedForm) -> Self {
        match val {
            TypedForm::StringType(s) => Self::KeyType(CompactKey::from(s)),
            TypedForm::Array(typ, arr) => {
                Self::Array(typ.into(), arr.into_iter().map(|x| x.into()).collect())
            }
            TypedForm::Object(obj) => {
                Self::Object(obj.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            TypedForm::TypedObject(typ, obj) => Self::TypedObject(
                typ.into(),
                obj.into_iter().map(|(k, v)| (k, v.into())).collect(),
            ),
        }
    }
}

fn drop_array_item_types(obj: IntermediateObjectType) -> IntermediateObjectType {
    obj.into_iter()
        .map(|(k, v)| (k, v.drop_array_item_types()))
        .collect()
}

fn compress_monolingual(obj: IntermediateObjectType) -> IntermediateObjectType {
    obj.into_iter()
        .map(|(k, v)| (k, v.compress_monolingual()))
        .collect()
}

impl IntermediateType {
    fn drop_array_item_types(self) -> Self {
        match self {
            IntermediateType::Simple(_) => self,
            IntermediateType::WithArgs(typ, args) => {
                IntermediateType::WithArgs(typ, drop_array_item_types(args))
            }
        }
    }

    fn compress_monolingual(self) -> Self {
        match self {
            IntermediateType::Simple(_) => self,
            IntermediateType::WithArgs(typ, args) => {
                IntermediateType::WithArgs(typ, compress_monolingual(args))
            }
        }
    }
}

impl IntermediateForm {
    pub fn drop_array_item_types(self) -> Self {
        match self {
            IntermediateForm::Array(typ, v) => IntermediateForm::Array(
                typ,
                v.into_iter()
                    .map(|x| match x {
                        IntermediateForm::TypedObject(_typ, obj) => {
                            IntermediateForm::Object(obj).drop_array_item_types()
                        }
                        _ => x.drop_array_item_types(),
                    })
                    .collect(),
            ),
            IntermediateForm::Object(obj) => IntermediateForm::Object(drop_array_item_types(obj)),
            IntermediateForm::TypedObject(t, o) => {
                IntermediateForm::TypedObject(t.drop_array_item_types(), drop_array_item_types(o))
            }
            IntermediateForm::KeyType(_) => self,
        }
    }

    pub fn compress_monolingual(self) -> Self {
        // we transform objects of type Z11 (Monolingual Text),
        // into only a TypeLabelledNode of
        // key: the actual text, value of Z11K2
        // type: the language, value of Z11K1
        match self {
            IntermediateForm::TypedObject(IntermediateType::Simple(typ), obj) => {
                if typ.is_labelled("Z11") {
                    IntermediateForm::KeyType(CompactKey::TypedLabelledNode(
                        match &obj.iter().find(|(k, _v)| k.is_labelled("Z11K2")).unwrap().1 {
                            IntermediateForm::KeyType(CompactKey::StringType(k)) => k.clone(),
                            _ => todo!(),
                        },
                        match obj
                            .into_iter()
                            .find(|(k, _v)| k.is_labelled("Z11K1"))
                            .unwrap()
                            .1
                        {
                            IntermediateForm::KeyType(CompactKey::StringType(k)) => SimpleType(k),
                            _ => todo!(),
                        },
                    ))
                } else {
                    IntermediateForm::TypedObject(
                        IntermediateType::Simple(typ),
                        compress_monolingual(obj),
                    )
                }
            }
            IntermediateForm::TypedObject(typ, obj) => {
                IntermediateForm::TypedObject(typ, compress_monolingual(obj))
            }
            IntermediateForm::KeyType(_) => self,
            IntermediateForm::Array(typ, v) => IntermediateForm::Array(
                typ,
                v.into_iter().map(|x| x.compress_monolingual()).collect(),
            ),
            IntermediateForm::Object(obj) => IntermediateForm::Object(compress_monolingual(obj)),
        }
    }
}

impl IntermediateForm {
    // this is mostly for debugging purpose, should not be returned via api
    pub fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            IntermediateForm::KeyType(k) => k.choose_lang(langs).into(),
            IntermediateForm::Array(typ, v) => Value::Array(
                std::iter::once(typ.choose_lang(langs))
                    .chain(v.into_iter().map(|x| x.choose_lang(langs)))
                    .collect(),
            ),
            IntermediateForm::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k.choose_lang(langs), v.choose_lang(langs)))
                    .collect(),
            ),
            IntermediateForm::TypedObject(typ, o) => {
                json!({"debug type":typ.choose_lang(langs), "debug obj": Value::Object(
                    o.into_iter()
                        .map(|(k, v)| (k.choose_lang(langs), v.choose_lang(langs)))
                        .collect(),
                )})
            }
        }
    }
}
