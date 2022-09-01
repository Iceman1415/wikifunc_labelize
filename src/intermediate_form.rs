use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::compact_key::{CompactKey, SimpleType};
use crate::simple_value::StringType;
use crate::typed_form::{Type, TypedForm};

// Compared to TypedForm, we allow more possible variants
// - StringType became CompactKey
// Tranformations (e.g. compress_monolingual()) are easy to do in IntermediateForm
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntermediateForm {
    KeyType(CompactKey),
    Array(Type, Vec<IntermediateForm>),
    Object(BTreeSet<(StringType, IntermediateForm)>),
    // in the intermediate form, we pull the type of objects out
    TypedObject(Type, BTreeSet<(StringType, IntermediateForm)>),
}

impl From<TypedForm> for IntermediateForm {
    fn from(val: TypedForm) -> Self {
        match val {
            TypedForm::StringType(s) => Self::KeyType(CompactKey::from(s)),
            TypedForm::Array(typ, arr) => {
                Self::Array(typ, arr.into_iter().map(|x| x.into()).collect())
            }
            TypedForm::Object(obj) => {
                Self::Object(obj.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            TypedForm::TypedObject(typ, obj) => {
                Self::TypedObject(typ, obj.into_iter().map(|(k, v)| (k, v.into())).collect())
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
            IntermediateForm::Object(o) => IntermediateForm::Object(
                o.into_iter()
                    .map(|(k, v)| (k, v.drop_array_item_types()))
                    .collect(),
            ),
            IntermediateForm::TypedObject(t, o) => IntermediateForm::TypedObject(
                t,
                o.into_iter()
                    .map(|(k, v)| (k, v.drop_array_item_types()))
                    .collect(),
            ),
            IntermediateForm::KeyType(_) => self,
        }
    }

    pub fn compress_monolingual(self) -> Self {
        // we transform objects of type Z11 (Monolingual Text),
        // into only a TypeLabelledNode of
        // key: the actual text, value of Z11K2
        // type: the language, value of Z11K1
        match self {
            IntermediateForm::TypedObject(Type::Simple(typ), val) => {
                if typ.is_labelled("Z11") {
                    IntermediateForm::KeyType(CompactKey::TypedLabelledNode(
                        match &val.iter().find(|(k, _v)| k.is_labelled("Z11K2")).unwrap().1 {
                            IntermediateForm::KeyType(CompactKey::StringType(k)) => k.clone(),
                            _ => todo!(),
                        },
                        match val
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
                        Type::Simple(typ),
                        val.into_iter()
                            .map(|(k, v)| (k, v.compress_monolingual()))
                            .collect(),
                    )
                }
            }
            IntermediateForm::TypedObject(typ, val) => IntermediateForm::TypedObject(
                typ,
                val.into_iter()
                    .map(|(k, v)| (k, v.compress_monolingual()))
                    .collect(),
            ),
            IntermediateForm::KeyType(_) => self,
            IntermediateForm::Array(typ, v) => IntermediateForm::Array(
                typ,
                v.into_iter().map(|x| x.compress_monolingual()).collect(),
            ),
            IntermediateForm::Object(obj) => IntermediateForm::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, v.compress_monolingual()))
                    .collect(),
            ),
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
