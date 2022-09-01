use serde_json::{json, Value};

use crate::simple_value::{SimpleValue, StringType};

type TypedObjectType = std::collections::BTreeSet<(StringType, TypedForm)>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Simple(StringType),
    WithArgs(StringType, TypedObjectType),
}

impl Type {
    pub fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            Type::Simple(k) => k.choose_lang(langs).into(),
            Type::WithArgs(typ, args) => {
                json!({"type": typ.choose_lang(langs), "args": Value::Object(
                    args.into_iter().map(|(k,v)| (k.choose_lang(langs).into(), v.choose_lang(langs))).collect()
                )})
            }
        }
    }
}

impl TryFrom<SimpleValue> for Type {
    type Error = ();

    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::StringType(k) => Ok(Type::Simple(k)),
            SimpleValue::Array(_) => Err(()),
            SimpleValue::Object(o) => {
                // if the value of Z1K1 is an object, the Z1K1 object itself should have a key Z1K1
                if let Some((z1k1, v)) = o.iter().find(|(k, _v)| k.is_labelled("Z1K1")).cloned() {
                    // We'll recursively look into the value of Z1K1, until it is a StringType and not an object.
                    // We then lift that StringType to the upper most level
                    let typ_of_typ = Type::try_from(v)?;
                    match typ_of_typ {
                        Type::Simple(s) => Ok(Type::WithArgs(
                            s,
                            o.into_iter()
                                .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                .map(|(k, v)| (k, v.into()))
                                .collect(),
                        )),
                        Type::WithArgs(typ, args) => Ok(Type::WithArgs(
                            typ,
                            o.into_iter()
                                .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                .map(|(k, v)| (k, v.into()))
                                .chain(std::iter::once((
                                    z1k1.clone(),
                                    TypedForm::Object(
                                        args.into_iter()
                                            .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                            .collect(),
                                    ),
                                )))
                                .collect(),
                        )),
                    }
                } else {
                    Err(())
                }
            }
        }
    }
}

/// By converting from SimpleValue to TypedForm,
/// we separate the types of ZObjects and Arrays from the rest of the data
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypedForm {
    StringType(StringType),
    // It would be great if I can distinguish typed array (Benjamin arrays) from regular untyped json arrays,
    // but the only way to distinguish them is to infer the type of the elements by comparing the "shape",
    // which is quite complicated...
    // Array(Vec<TypedForm>),
    Array(Type, Vec<TypedForm>),
    // All ZObjects should have a type, but just in case...
    Object(TypedObjectType),
    TypedObject(Type, TypedObjectType),
}

impl From<SimpleValue> for TypedForm {
    fn from(val: SimpleValue) -> Self {
        match val {
            SimpleValue::StringType(s) => Self::StringType(s),
            SimpleValue::Array(v) => {
                // we're assuming all arrays are "Benjamin arrays"
                // see: https://meta.wikimedia.org/wiki/Abstract_Wikipedia/Updates/2022-07-29
                let typ = Type::try_from(v[0].clone()).expect(
                    "the first item of an ZObject array should be the type of the elements",
                );
                Self::Array(typ, v.into_iter().skip(1).map(|x| x.into()).collect())
            }
            SimpleValue::Object(o) => {
                let z1k1 = o
                    .iter()
                    .find(|(k, _v)| k.is_labelled("Z1K1"))
                    .map(|x| x.clone());
                // if there is a key Z1K1 (type) in the object, we separate it
                // At a later stage the type will be merged into the parent object's key
                match z1k1 {
                    Some((_z1k1_key, typ)) => {
                        Self::TypedObject(
                            //TODO: handle if the value of Z1K1 cannot be converted into Type
                            typ.try_into().unwrap(),
                            o.into_iter()
                                .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                .map(|(k, v)| (k, v.into()))
                                .collect(),
                        )
                    }
                    None => Self::Object(o.into_iter().map(|(k, v)| (k, v.into())).collect()),
                }
            }
        }
    }
}

impl TypedForm {
    // this is mostly for debugging purpose, should not be returned via api
    pub fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            TypedForm::StringType(s) => s.choose_lang(langs).into(),
            TypedForm::Array(typ, v) => Value::Array(
                std::iter::once(typ.choose_lang(langs))
                    .chain(v.into_iter().map(|x| x.choose_lang(langs)))
                    .collect(),
            ),
            TypedForm::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k.choose_lang(langs), v.choose_lang(langs)))
                    .collect(),
            ),
            TypedForm::TypedObject(typ, o) => {
                json!({"debug type":typ.choose_lang(langs), "debug obj": Value::Object(
                    o.into_iter()
                        .map(|(k, v)| (k.choose_lang(langs), v.choose_lang(langs)))
                        .collect(),
                )})
            }
        }
    }
}
