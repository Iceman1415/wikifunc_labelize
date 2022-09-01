use std::collections::BTreeSet;

use serde_json::Value;

use crate::compact_key::{CompactKey, SimpleType};
use crate::intermediate_form::IntermediateForm;
use crate::simple_value::{SimpleValue, StringType};
use crate::typed_form::{Type, TypedForm};

// we "compactify" by putting the type of objects into the "key" when we stringify
// so an object now has 3 fields, the key, the type, and the value
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
    type_args: BTreeSet<(StringType, SimpleValue)>,
) -> CompactValue {
    // let z1k1 = _labelize("Z1K1".to_string()).await.unwrap();
    let z1k1 = StringType::String("Z1K1".to_string());
    let converted_obj: CompactValue = IntermediateForm::Object(obj).into();
    match converted_obj {
        CompactValue::Object(converted_obj) => CompactValue::Object(
            converted_obj
                .into_iter()
                .chain(std::iter::once((
                    CompactKey::StringType(z1k1),
                    CompactValue::Object(
                        type_args
                            .into_iter()
                            .map(|(k, v)| (k.into(), v.into()))
                            .collect(),
                    ),
                )))
                .collect(),
        ),
        _ => unreachable!(),
    }
}

impl From<IntermediateForm> for CompactValue {
    fn from(val: IntermediateForm) -> Self {
        match val {
            IntermediateForm::KeyType(k) => CompactValue::KeyType(k),
            IntermediateForm::Array(Type::Simple(_), v) => {
                CompactValue::Array(v.into_iter().map(|x| x.into()).collect())
            }
            IntermediateForm::Array(Type::WithArgs(_typ, type_args), v) => CompactValue::Array(
                std::iter::once(
                    IntermediateForm::from(TypedForm::from(SimpleValue::Object(type_args))).into(),
                )
                .chain(v.into_iter().map(|x| x.into()))
                .collect(),
            ),
            IntermediateForm::Object(o) => CompactValue::Object(
                o.into_iter()
                    .map(|(k, v)| match v {
                        // for each typed value in object, we pull the type outward
                        IntermediateForm::TypedObject(typ, obj) => match typ {
                            Type::Simple(typ) => (
                                CompactKey::TypedLabelledNode(k, SimpleType(typ)),
                                IntermediateForm::Object(obj).into(),
                            ),
                            Type::WithArgs(typ, type_args) => (
                                CompactKey::TypedLabelledNode(k, SimpleType(typ)),
                                rebuild_obj_with_type_args(obj, type_args),
                            ),
                        },
                        IntermediateForm::Array(typ, v) => match typ {
                            Type::Simple(typ) => (
                                CompactKey::TypedLabelledNode(k, SimpleType(typ)),
                                CompactValue::Array(v.into_iter().map(|x| x.into()).collect()),
                            ),
                            Type::WithArgs(typ, type_args) => (
                                CompactKey::TypedLabelledNode(k, SimpleType(typ)),
                                CompactValue::Array(
                                    std::iter::once(
                                        IntermediateForm::from(TypedForm::from(
                                            SimpleValue::Object(type_args),
                                        ))
                                        .into(),
                                    )
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
                    Type::Simple(typ) => (
                        CompactKey::Transient(SimpleType(typ)),
                        IntermediateForm::Object(obj).into(),
                    ),
                    Type::WithArgs(typ, type_args) => (
                        CompactKey::Transient(SimpleType(typ)),
                        rebuild_obj_with_type_args(obj, type_args),
                    ),
                }]))
            }
        }
    }
}

impl CompactValue {
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
