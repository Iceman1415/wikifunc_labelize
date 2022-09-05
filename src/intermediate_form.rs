use serde_json::{json, Value};

use crate::compact_key::SimpleType;
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
// - Additional LabelledNode variant, used in .compress_monolingual()
//   this is similar to attaching type to the key, but here we're attaching to a value
// Tranformations (e.g. compress_monolingual()) are easy to do in IntermediateForm
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntermediateForm {
    StringType(StringType),
    LabelledNode(StringType, SimpleType),
    Array(Vec<IntermediateForm>),
    TypedArray(IntermediateType, Vec<IntermediateForm>),
    Object(IntermediateObjectType),
    TypedObject(IntermediateType, IntermediateObjectType),
}

impl From<TypedForm> for IntermediateForm {
    fn from(val: TypedForm) -> Self {
        match val {
            TypedForm::StringType(s) => Self::StringType(s),
            TypedForm::Array(arr) => Self::Array(arr.into_iter().map(|x| x.into()).collect()),
            TypedForm::TypedArray(typ, arr) => {
                Self::TypedArray(typ.into(), arr.into_iter().map(|x| x.into()).collect())
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

fn compress_reference(obj: IntermediateObjectType) -> IntermediateObjectType {
    obj.into_iter()
        .map(|(k, v)| (k, v.compress_reference()))
        .collect()
}

fn compress_string(obj: IntermediateObjectType) -> IntermediateObjectType {
    obj.into_iter()
        .map(|(k, v)| (k, v.compress_string()))
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

    fn compress_reference(self) -> Self {
        match self {
            IntermediateType::Simple(_) => self,
            // TODO: this seems bad, too many assumptions, need refactor
            IntermediateType::WithArgs(typ, args) => {
                if typ.is_labelled("Z9") {
                    match args.iter().find(|(k, _v)| k.is_labelled("Z9K1")) {
                        Some((_z9k1, v)) => match v {
                            IntermediateForm::StringType(s) => IntermediateType::Simple(s.clone()),
                            _ => todo!(),
                        },
                        None => match &args.iter().find(|(k, _v)| k.is_labelled("Z1K1")).unwrap().1
                        {
                            IntermediateForm::Object(obj) => {
                                match &obj.iter().find(|(k, _v)| k.is_labelled("Z9K1")).unwrap().1 {
                                    IntermediateForm::StringType(s) => IntermediateType::WithArgs(
                                        s.clone(),
                                        args.into_iter()
                                            .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                            .collect(),
                                    ),
                                    _ => todo!(),
                                }
                            }
                            _ => todo!(),
                        },
                    }
                } else {
                    IntermediateType::WithArgs(typ, compress_reference(args))
                }
            }
        }
    }

    fn compress_string(self) -> Self {
        match self {
            IntermediateType::Simple(_) => self,
            IntermediateType::WithArgs(typ, args) => {
                if typ.is_labelled("Z6") {
                    IntermediateType::Simple(
                        match &args.iter().find(|(k, _v)| k.is_labelled("Z6K1")).unwrap().1 {
                            IntermediateForm::StringType(s) => s.clone(),
                            _ => todo!(),
                        },
                    )
                } else {
                    IntermediateType::WithArgs(typ, compress_reference(args))
                }
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
            IntermediateForm::TypedArray(typ, v) => IntermediateForm::TypedArray(
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
            IntermediateForm::Array(arr) => IntermediateForm::Array(
                arr.into_iter().map(|x| x.drop_array_item_types()).collect(),
            ),
            IntermediateForm::Object(obj) => IntermediateForm::Object(drop_array_item_types(obj)),
            IntermediateForm::TypedObject(t, o) => {
                IntermediateForm::TypedObject(t.drop_array_item_types(), drop_array_item_types(o))
            }
            IntermediateForm::StringType(_) => self,
            IntermediateForm::LabelledNode(_, _) => self,
        }
    }

    pub fn compress_reference(self) -> Self {
        match self {
            IntermediateForm::TypedObject(IntermediateType::Simple(typ), obj) => {
                if typ.is_labelled("Z9") {
                    IntermediateForm::StringType(
                        match &obj.iter().find(|(k, _v)| k.is_labelled("Z9K1")).unwrap().1 {
                            IntermediateForm::StringType(s) => s.clone(),
                            _ => todo!("non-string value for Z9K1"),
                        },
                    )
                } else {
                    IntermediateForm::TypedObject(
                        IntermediateType::Simple(typ),
                        compress_reference(obj),
                    )
                }
            }
            IntermediateForm::TypedObject(typ, obj) => {
                IntermediateForm::TypedObject(typ.compress_reference(), compress_reference(obj))
            }
            IntermediateForm::StringType(_) => self,
            IntermediateForm::LabelledNode(_, _) => self,
            IntermediateForm::Array(v) => {
                IntermediateForm::Array(v.into_iter().map(|x| x.compress_reference()).collect())
            }
            IntermediateForm::TypedArray(typ, v) => IntermediateForm::TypedArray(
                typ.compress_reference(),
                v.into_iter().map(|x| x.compress_reference()).collect(),
            ),
            IntermediateForm::Object(obj) => IntermediateForm::Object(compress_reference(obj)),
        }
    }

    pub fn compress_string(self) -> Self {
        match self {
            IntermediateForm::TypedObject(IntermediateType::Simple(typ), obj) => {
                // if the object has type String (Z6)
                if typ.is_labelled("Z6") {
                    IntermediateForm::StringType(
                        // there should be key Z6K1 containing the actual string
                        match obj
                            .into_iter()
                            .find(|(k, _v)| k.is_labelled("Z6K1"))
                            .unwrap()
                            .1
                        {
                            // if the string is labelled, it should not be, we turn it back to a normal string
                            IntermediateForm::StringType(s) => StringType::String(s.to_raw()),
                            // ...wait can it be a function call?
                            _ => todo!("non-string value for Z6K1"),
                        },
                    )
                } else {
                    IntermediateForm::TypedObject(
                        IntermediateType::Simple(typ),
                        compress_string(obj),
                    )
                }
            }
            IntermediateForm::TypedObject(typ, obj) => {
                IntermediateForm::TypedObject(typ.compress_string(), compress_string(obj))
            }
            IntermediateForm::StringType(_) => self,
            IntermediateForm::LabelledNode(_, _) => self,
            IntermediateForm::Array(v) => {
                IntermediateForm::Array(v.into_iter().map(|x| x.compress_string()).collect())
            }
            IntermediateForm::TypedArray(typ, v) => IntermediateForm::TypedArray(
                typ.compress_string(),
                v.into_iter().map(|x| x.compress_string()).collect(),
            ),
            IntermediateForm::Object(obj) => IntermediateForm::Object(compress_string(obj)),
        }
    }

    pub fn compress_monolingual(self) -> Self {
        // we transform objects of type Z11 (Monolingual Text),
        // into a TypeLabelledNode of
        // key: the actual text, value of Z11K2
        // type: the language, value of Z11K1
        match self {
            IntermediateForm::TypedObject(IntermediateType::Simple(typ), obj) => {
                if typ.is_labelled("Z11") {
                    IntermediateForm::LabelledNode(
                        match &obj.iter().find(|(k, _v)| k.is_labelled("Z11K2")).unwrap().1 {
                            IntermediateForm::StringType(s) => s.clone(),
                            _ => todo!(),
                        },
                        match obj
                            .into_iter()
                            .find(|(k, _v)| k.is_labelled("Z11K1"))
                            .unwrap()
                            .1
                        {
                            IntermediateForm::StringType(s) => SimpleType(s),
                            _ => todo!(),
                        },
                    )
                } else {
                    IntermediateForm::TypedObject(
                        IntermediateType::Simple(typ),
                        compress_monolingual(obj),
                    )
                }
            }
            IntermediateForm::TypedObject(typ, obj) => {
                IntermediateForm::TypedObject(typ.compress_monolingual(), compress_monolingual(obj))
            }
            IntermediateForm::StringType(_) => self,
            IntermediateForm::LabelledNode(_, _) => self,
            IntermediateForm::Array(v) => {
                IntermediateForm::Array(v.into_iter().map(|x| x.compress_monolingual()).collect())
            }
            IntermediateForm::TypedArray(typ, v) => IntermediateForm::TypedArray(
                typ.compress_monolingual(),
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
            IntermediateForm::StringType(s) => s.choose_lang(langs).into(),
            IntermediateForm::LabelledNode(s, t) => {
                format!("{} [{}]", s.choose_lang(langs), t.0.choose_lang(langs),).into()
            }
            IntermediateForm::Array(v) => {
                Value::Array((v.into_iter().map(|x| x.choose_lang(langs))).collect())
            }
            IntermediateForm::TypedArray(typ, v) => Value::Array(
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
