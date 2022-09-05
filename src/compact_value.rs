use std::collections::BTreeSet;

use serde_json::Value;

use crate::compact_key::{CompactKey, SimpleType};
use crate::intermediate_form::{IntermediateForm, IntermediateType};
use crate::simple_value::{SimpleValue, StringType};

// CompactValue is the final type, ready to be converted back to json Value
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompactValue {
    KeyType(CompactKey),
    Array(Vec<CompactValue>),
    Object(BTreeSet<(CompactKey, CompactValue)>),
}

impl From<SimpleValue> for CompactValue {
    fn from(val: SimpleValue) -> Self {
        match val {
            SimpleValue::StringType(k) => CompactValue::KeyType(k.into()),
            SimpleValue::Array(a) => CompactValue::Array(a.into_iter().map(|x| x.into()).collect()),
            SimpleValue::Object(o) => {
                CompactValue::Object(o.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
            }
        }
    }
}

fn rebuild_obj_with_type_args(
    obj: BTreeSet<(StringType, IntermediateForm)>,
    type_args: BTreeSet<(StringType, IntermediateForm)>,
) -> CompactValue {
    // let z1k1 = _labelize("Z1K1".to_string()).await.unwrap();
    let z1k1 = StringType::String("!Z1K1".to_string());
    let converted_obj: CompactValue = IntermediateForm::Object(obj).into();
    let converted_args: CompactValue = IntermediateForm::Object(type_args).into();
    match (converted_obj, converted_args) {
        (CompactValue::Object(obj), CompactValue::Object(args)) => CompactValue::Object(
            obj.into_iter()
                .chain(std::iter::once((
                    CompactKey::from(z1k1),
                    CompactValue::Object(args),
                )))
                .collect(),
        ),
        _ => unreachable!(),
    }
}

impl From<IntermediateForm> for CompactValue {
    fn from(val: IntermediateForm) -> Self {
        match val {
            IntermediateForm::StringType(s) => CompactValue::KeyType(CompactKey::from(s)),
            IntermediateForm::LabelledNode(s, t) => {
                CompactValue::KeyType(CompactKey::StringType(s, vec![t]))
            }
            IntermediateForm::Array(v) => {
                CompactValue::Array(v.into_iter().map(|x| x.into()).collect())
            }
            IntermediateForm::TypedArray(IntermediateType::Simple(_), v) => {
                CompactValue::Array(v.into_iter().map(|x| x.into()).collect())
            }
            IntermediateForm::TypedArray(IntermediateType::WithArgs(_typ, type_args), v) => {
                CompactValue::Array(
                    std::iter::once(IntermediateForm::Object(type_args).into())
                        .chain(v.into_iter().map(|x| x.into()))
                        .collect(),
                )
            }
            IntermediateForm::Object(o) => CompactValue::Object(
                o.into_iter()
                    .map(|(k, v)| match v {
                        // for each typed value in object, we pull the type outward
                        IntermediateForm::TypedObject(typ, obj) => match typ {
                            IntermediateType::Simple(typ) => (
                                CompactKey::StringType(k, vec![SimpleType(typ)]),
                                IntermediateForm::Object(obj).into(),
                            ),
                            IntermediateType::WithArgs(typ, type_args) => (
                                CompactKey::StringType(k, vec![SimpleType(typ)]),
                                rebuild_obj_with_type_args(obj, type_args),
                            ),
                        },
                        IntermediateForm::TypedArray(typ, v) => match typ {
                            IntermediateType::Simple(typ) => (
                                CompactKey::StringType(k, vec![SimpleType(typ)]),
                                CompactValue::Array(v.into_iter().map(|x| x.into()).collect()),
                            ),
                            IntermediateType::WithArgs(typ, type_args) => (
                                CompactKey::StringType(k, vec![SimpleType(typ)]),
                                CompactValue::Array(
                                    std::iter::once(IntermediateForm::Object(type_args).into())
                                        .chain(v.into_iter().map(|x| x.into()))
                                        .collect(),
                                ),
                            ),
                        },
                        _ => (k.into(), v.into()),
                    })
                    .collect(),
            ),
            IntermediateForm::TypedObject(typ, obj) => {
                CompactValue::Object(BTreeSet::from([match typ {
                    IntermediateType::Simple(typ) => (
                        CompactKey::Transient(vec![SimpleType(typ)]),
                        IntermediateForm::Object(obj).into(),
                    ),
                    IntermediateType::WithArgs(typ, type_args) => (
                        CompactKey::Transient(vec![SimpleType(typ)]),
                        rebuild_obj_with_type_args(obj, type_args),
                    ),
                }]))
            }
        }
    }
}

impl CompactValue {
    pub fn compress_simple_classes(self) -> Self {
        match self {
            CompactValue::KeyType(_) => self,
            CompactValue::Array(arr) => Self::Array(
                arr.into_iter()
                    .map(|x| x.compress_simple_classes())
                    .collect(),
            ),
            CompactValue::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, v.compress_simple_classes()))
                    .map(|(key, val)| match val {
                        CompactValue::KeyType(_) | CompactValue::Array(_) => (key, val),
                        CompactValue::Object(inner_obj) => {
                            if inner_obj.len() == 1 {
                                let (inner_k, inner_v) = inner_obj.into_iter().next().unwrap();
                                let inner_k: Vec<_> = match inner_k {
                                    CompactKey::StringType(k, t) => std::iter::once(SimpleType(k))
                                        .chain(t.into_iter())
                                        .collect(),
                                    CompactKey::Transient(t) => t.into_iter().collect(),
                                };
                                match key {
                                    CompactKey::StringType(k, t) => (
                                        CompactKey::StringType(
                                            k,
                                            t.into_iter().chain(inner_k.into_iter()).collect(),
                                        ),
                                        inner_v,
                                    ),
                                    CompactKey::Transient(t) => (
                                        CompactKey::Transient(
                                            t.into_iter().chain(inner_k.into_iter()).collect(),
                                        ),
                                        inner_v,
                                    ),
                                }
                            } else {
                                (
                                    key,
                                    CompactValue::Object(inner_obj).compress_simple_classes(),
                                )
                            }
                        }
                    })
                    .collect(),
            ),
        }
    }

    pub fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            CompactValue::KeyType(k) => k.choose_lang(langs).into(),
            CompactValue::Array(v) => {
                Value::Array(v.into_iter().map(|x| x.choose_lang(langs)).collect())
            }
            CompactValue::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k.choose_lang(langs), v.choose_lang(langs)))
                    .collect(),
            ),
        }
    }
}
