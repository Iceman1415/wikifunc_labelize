use std::collections::BTreeSet;

use futures::future;
use regex::Regex;
use serde_json::json;
use serde_json::Value;

use actix_web::{get, post, App, HttpResponse, HttpServer, Responder};
use async_recursion::async_recursion;

use cached::proc_macro::cached;
// use cached::proc_macro::io_cached;
// use cached::AsyncRedisCache;
use cached::TimedCache;
use thiserror::Error;

const DOMAIN: &str = "https://wikifunctions.beta.wmflabs.org/w";

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body(r#"<body>
    <div><h2>Get /</h2><div>This help page</div></div>
    <div><h2>Post /labelize</h2>
        <div>Append human readable labels to all strings in the json body that are ZIDs (Zxxx) or Global Keys (ZxxxKyyy)</div>
        <div>By default, the entire json body is labelized, and the prefered language of human readable labels are in order: Japanese (Z1830), Chinese (Z1006), English (Z1002)</div>
        <div>Alternatively you can supply your own order of prefered language, like so: <code>{"data": "zobject...", "langs": ["Z1830", "Z1006", "Z1002"]}</code></div>
    </div>
    <div><h2>Post /compacify</h2>
        <div>After labelize-ing the json body, we then "raises" the type (Z1K1) of ZObjects (all ZObjects has its type in the key Z1K1) and the type in Arrays (all Arrays have the type as the first element). This makes the json more readable.</div>
        <div>A custom order of prefered language can be provided similar to /labelize</div>
    </div>
</body>"#)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LabelledNode {
    readable_labels: BTreeSet<(String, String)>,
    z_label: String,
}

impl LabelledNode {
    fn choose_lang(self, langs: &Vec<String>) -> String {
        format!(
            "{}: {}",
            self.z_label,
            langs
                .iter()
                .find_map(|lang| self.readable_labels.iter().find(|&label| label.0 == *lang))
                .unwrap_or(
                    self.readable_labels
                        .iter()
                        .next()
                        .unwrap_or(&("".to_string(), "<no label>".to_string()))
                )
                .1
                .clone()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum StringType {
    String(String),
    LabelledNode(LabelledNode),
}

impl StringType {
    fn is_labelled(&self, label: &str) -> bool {
        match self {
            Self::String(s) => s == label,
            StringType::LabelledNode(n) => n.z_label == label,
        }
    }
}

impl StringType {
    fn choose_lang(self, langs: &Vec<String>) -> String {
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum SimpleValue {
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
    fn choose_lang(self, langs: &Vec<String>) -> Value {
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

#[derive(Error, Debug, PartialEq, Clone)]
enum MyError {
    #[error("network error `{0}`")]
    NetworkError(String),
    #[error("mismatch in schema between returned data from wikifunction and expectation, `{0}`")]
    SchemaError(String),
    // #[error("error with redis cache `{0}`")]
    // RedisError(String),
}

async fn fetch(z_number: &str) -> std::result::Result<Value, MyError> {
    println!("fetching {}", z_number);
    match reqwest::get(format!("{}/api.php?action=query&format=json&list=wikilambdaload_zobjects&wikilambdaload_zids={}&wikilambdaload_canonical=true",DOMAIN, z_number)).await {
        Ok(res) => Ok(
            serde_json::from_str::<Value>(&res.text().await.unwrap()).expect("failed parsing wikifunction response")
        .get("query")
        .expect("no \"query\" key in wikifunction response")
        .get("wikilambdaload_zobjects")
        .expect("no \"wikilambdaload_zobjects\" key in wikifunction response")
        .get(z_number)
        .expect(    &format!("no key for self ({}) in wikifunction response", z_number))
        .get("data")
        .expect("no \"data\" key in wikifunction response")
        .to_owned()
        ),
        Err(x) => {eprintln!("error fetching {}: {}", z_number, x); Err(MyError::NetworkError(x.to_string()))}
    }
}

// #[io_cached(
//     map_error = r##"|e| MyError::RedisError(format!("{:?}", e))"##,
//     type = "AsyncRedisCache<String, String>",
//     create = r##" {
//         AsyncRedisCache::new("cached_redis_prefix", 600)
//             .set_refresh(true)
//             .set_connection_string("redis://localhost:6379")
//             .build()
//             .await
//             .expect("error building redis cache")
//     } "##
// )]
#[cached(
    type = "TimedCache<String, std::result::Result<StringType, MyError>>",
    create = "{ TimedCache:: with_lifespan_and_refresh(600, true) }",
    convert = r#"{ format!("{}", s) }"#
)]
async fn _labelize(s: String) -> std::result::Result<StringType, MyError> {
    // println!("labelize {}", s);
    if Regex::new(r"^Z\d+$").unwrap().is_match(&s) {
        let readable_labels = fetch(&s)
            .await?
            .get("Z2K3")
            .ok_or(MyError::SchemaError(
                "wikifunction response is not a Persistent Object, no Z2K3 key ".to_string(),
            ))?
            .get("Z12K1")
            .ok_or(MyError::SchemaError(
                "no Z12K1 (Multilingual Text) key in Persistent Object".to_string(),
            ))?
            .as_array()
            .ok_or(MyError::SchemaError("Z12K1 is not an array".to_string()))?
            .into_iter()
            .skip(1)
            .map(|v| -> std::result::Result<(String, String), MyError> {
                Ok((
                    v.get("Z11K1")
                        .ok_or(MyError::SchemaError(
                            "no key Z11K1 in item of Z12K1".to_string(),
                        ))?
                        .as_str()
                        .ok_or(MyError::SchemaError("value of Z11K1 not a str".to_string()))?
                        .to_string(),
                    v.get("Z11K2")
                        .ok_or(MyError::SchemaError(
                            "no key Z11K1 in item of Z12K1".to_string(),
                        ))?
                        .as_str()
                        .ok_or(MyError::SchemaError("value of Z11K2 not a str".to_string()))?
                        .to_string(),
                ))
            })
            .collect::<std::result::Result<BTreeSet<_>, MyError>>()?;
        // Ok(format!("{} ({})", res, s))
        Ok(StringType::LabelledNode(LabelledNode {
            readable_labels,
            z_label: s,
        }))
    } else if Regex::new(r"^Z\d+K\d+$").unwrap().is_match(&s) {
        let pat = s.split("K").collect::<Vec<_>>();
        let z_number = pat[0];
        let k_number = pat[1].parse::<usize>().unwrap();

        let res = fetch(z_number).await?;

        // example object: Z4, of type Z4
        // example object: Z811, of type Z8
        // example object: Z517, of type Z50
        // example: Z4 -> obj["Z2K2"]["Z4K2"][k_number]["Z3K3"]["Z12K1"][1]["Z11K2"]
        // example: Z8 -> obj["Z2K2"]["Z8K1"][k_number]["Z17K3"]["Z12K1"][1]["Z11K2"]
        // we are trying to get the label for some ZxxxKyyy
        // we have fetched the data for Zxxx
        // first of all, Zxxx is an persistent object because it has a Z-number
        // the label for the keys are always stored in Z2K2: value
        let label_val = res
            .get("Z2K2")
            .ok_or(MyError::SchemaError(
                "wikifunction response is not a Persistent Object, no Z2K2 key ".to_string(),
            ))?
            .as_object()
            .ok_or(MyError::SchemaError(
                "value of Z2K2 is not object".to_string(),
            ))?
            .iter()
            // we now try to find the key-value, where...
            // the value is an array of objects containing Z12 values
            .filter(|&(_k, v)| v.is_array())
            .filter(|&(_k, v)| v.as_array().unwrap().len() > k_number)
            .map(|(_k, v)| {
                v.as_array().unwrap()[k_number]
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|&(_k, v)| v.is_object())
                    .filter(|&(_k, v)| {
                        v.as_object().unwrap().get("Z1K1")
                            == Some(&Value::String("Z12".to_string()))
                    })
                    .next()
                    .unwrap()
                    .1
            })
            .next()
            .unwrap();

        let readable_labels = label_val
            .get("Z12K1")
            .ok_or(MyError::SchemaError(
                "no \"Z12K1\" key in wikifunction response".to_string(),
            ))?
            .as_array()
            .ok_or(MyError::SchemaError("Z12K1 is not an array".to_string()))?
            .into_iter()
            .skip(1)
            .map(|v| -> std::result::Result<(String, String), MyError> {
                Ok((
                    v.get("Z11K1")
                        .ok_or(MyError::SchemaError(
                            "no key Z11K1 in item of Z12K1".to_string(),
                        ))?
                        .as_str()
                        .ok_or(MyError::SchemaError("value of Z11K1 not a str".to_string()))?
                        .to_string(),
                    v.get("Z11K2")
                        .ok_or(MyError::SchemaError(
                            "no key Z11K1 in item of Z12K1".to_string(),
                        ))?
                        .as_str()
                        .ok_or(MyError::SchemaError("value of Z11K2 not a str".to_string()))?
                        .to_string(),
                ))
            })
            .collect::<std::result::Result<BTreeSet<_>, MyError>>()?;
        Ok(StringType::LabelledNode(LabelledNode {
            readable_labels,
            z_label: s,
        }))
    } else {
        Ok(StringType::String(s))
    }
}

async fn _labelize_wrapped_key(s: String) -> StringType {
    if s == "" {
        return StringType::String(s);
    }
    // println!("labelize wrapped {}", s);
    match _labelize(s.clone()).await {
        Ok(out) => out,
        Err(err) => {
            eprintln!("error when parsing {}: {:?}", s, err);
            StringType::String(s)
        }
    }
}

async fn _labelize_wrapped(s: String) -> SimpleValue {
    if s == "" {
        return SimpleValue::StringType(StringType::String(s));
    }
    // println!("labelize wrapped {}", s);
    match _labelize(s.clone()).await {
        Ok(out) => out.into(),
        Err(err) => {
            eprintln!("error when parsing {}: {:?}", s, err);
            SimpleValue::StringType(StringType::String(s))
        }
    }
}

#[async_recursion]
async fn _labelize_json(v: Value) -> SimpleValue {
    // println!("_labelize_json_wrapped {}", v);
    match v {
        Value::Null => unimplemented!(),
        Value::Bool(_b) => unimplemented!(),
        Value::Number(_n) => unimplemented!(),
        Value::String(s) => _labelize_wrapped(s).await,
        Value::Array(a) => {
            SimpleValue::Array(future::join_all(a.into_iter().map(|x| _labelize_json(x))).await)
        }
        Value::Object(o) => {
            SimpleValue::Object(BTreeSet::from_iter(
                future::join_all(o.into_iter().map(|(key, val)| {
                    future::join(_labelize_wrapped_key(key), _labelize_json(val))
                }))
                .await,
            ))
        }
    }
}

//
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Type {
    Simple(StringType),
    WithArgs(StringType, BTreeSet<(StringType, SimpleValue)>),
}

impl Type {
    fn choose_lang(self, langs: &Vec<String>) -> Value {
        match self {
            Type::Simple(k) => k.choose_lang(langs).into(),
            Type::WithArgs(typ, args) => {
                json!({"type": typ.choose_lang(langs), "args": SimpleValue::Object(args).choose_lang(langs)})
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
                match o.iter().find(|(k, _v)| k.is_labelled("Z1K1")).cloned() {
                    Some((z1k1, v)) => {
                        let typ_of_typ: Type = v.try_into()?;
                        match typ_of_typ {
                            Type::Simple(s) => Ok(Type::WithArgs(
                                s,
                                o.into_iter()
                                    .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                    .collect(),
                            )),
                            Type::WithArgs(typ, args) => Ok(Type::WithArgs(
                                typ,
                                o.into_iter()
                                    .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                    .chain(std::iter::once((
                                        z1k1.clone(),
                                        SimpleValue::Object(
                                            args.into_iter()
                                                .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                                                .collect(),
                                        ),
                                    )))
                                    .collect(),
                            )),
                        }
                    }
                    None => Err(()),
                }
            }
        }
    }
}

// we "compactify" by putting the type of objects into the "key" when we stringify
// so an object now has 3 fields, the key, the type, and the value
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum IntermediateForm {
    KeyType(CompactKey),
    Array(Type, Vec<IntermediateForm>),
    Object(BTreeSet<(StringType, IntermediateForm)>),
    // in the intermediate form, we pull the type of objects out
    TypedObject(Type, BTreeSet<(StringType, IntermediateForm)>),
}

impl From<SimpleValue> for IntermediateForm {
    fn from(val: SimpleValue) -> Self {
        match val {
            SimpleValue::StringType(k) => Self::KeyType(k.into()),
            SimpleValue::Array(v) => {
                let typ = Type::try_from(v[0].clone()).expect(
                    "first item of an ZObject array should be the type of the array's elements",
                );
                Self::Array(typ, v.into_iter().skip(1).map(|x| x.into()).collect())
            }
            SimpleValue::Object(o) => {
                let z1k1 = o
                    .iter()
                    .find(|(k, _v)| k.is_labelled("Z1K1"))
                    .map(|x| x.clone());
                // if there is a key Z1K1 in the object (aka the object has a type)
                // we try to raise the type upward / outside, into the key of the object
                match z1k1 {
                    // if the type is simple, we can just move it
                    Some((_z1k1_key, SimpleValue::StringType(typ))) => Self::TypedObject(
                        Type::Simple(typ),
                        o.into_iter()
                            .filter(|(k, _v)| !k.is_labelled("Z1K1"))
                            .map(|(k, v)| (k, v.into()))
                            .collect(),
                    ),
                    Some((_z1k1_key, SimpleValue::Array(_typ))) => todo!(),
                    // if the type is an object...
                    Some((_z1k1_key, SimpleValue::Object(typ))) => {
                        //TODO: handle if the value of Z1K1 cannot be converted into Type
                        let converted_type: Type = SimpleValue::Object(typ).try_into().unwrap();
                        Self::TypedObject(
                            converted_type,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SimpleType(StringType);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum CompactKey {
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
    fn choose_lang(self, langs: &Vec<String>) -> String {
        match self {
            CompactKey::StringType(n) => n.choose_lang(langs),
            CompactKey::TypedLabelledNode(key, typ) => {
                format!("{} [{}]", key.choose_lang(langs), typ.0.choose_lang(langs),).into()
            }
            CompactKey::Transient(typ) => format!("[{}]", typ.0.choose_lang(langs),),
        }
    }
}

// we "compactify" by putting the type of objects into the "key" when we stringify
// so an object now has 3 fields, the key, the type, and the value
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum CompactValue {
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
            IntermediateForm::KeyType(k) => CompactValue::KeyType(k.into()),
            IntermediateForm::Array(Type::Simple(_), v) => {
                CompactValue::Array(v.into_iter().map(|x| x.into()).collect())
            }
            IntermediateForm::Array(Type::WithArgs(_typ, type_args), v) => CompactValue::Array(
                std::iter::once(IntermediateForm::from(SimpleValue::Object(type_args)).into())
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
                                        IntermediateForm::from(SimpleValue::Object(type_args))
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
    fn choose_lang(self, langs: &Vec<String>) -> Value {
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

impl IntermediateForm {
    fn drop_array_item_types(self) -> Self {
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

    fn compress_monolingual(self) -> Self {
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
    fn choose_lang(self, langs: &Vec<String>) -> Value {
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

// the 3 languages (scripts) that I can read, arranged by ascending usage
const DEFAULT_LANGS: [&str; 3] = ["Z1830", "Z1006", "Z1002"];

fn request_wrapper(req_body: String) -> Result<(Value, Vec<String>), HttpResponse> {
    let v: Value = match serde_json::from_str(&req_body) {
        Ok(v) => v,
        Err(_) => {
            return Err(HttpResponse::BadRequest()
                .reason("invalid json object")
                .finish())
        }
    };
    match v {
        Value::Object(obj) => {
            // if the request body has both key "data" and key "langs",
            // we use the custom supplied langs when calling choose_lang()
            if obj.contains_key("data") && obj.contains_key("langs") {
                // but the value of "langs" could've been any kind of Value
                // we have to make sure it is a Vec<String>
                match obj.get("langs").unwrap() {
                    Value::Array(langs) => {
                        let langs: Vec<String> = langs
                            .into_iter()
                            .map(|x| match x {
                                Value::String(s) => Ok(s.clone()),
                                _ => Err(HttpResponse::BadRequest()
                                    .reason("value of langs should be an array of string")
                                    .finish()),
                            })
                            .collect::<Result<Vec<String>, _>>()?;

                        // TODO: can we not clone the data?
                        Ok((obj.get("data").unwrap().clone(), langs))
                    }
                    _ => Err(HttpResponse::BadRequest()
                        .reason("value of langs should be an array of string")
                        .finish()),
                }
            } else {
                Ok((
                    Value::Object(obj),
                    DEFAULT_LANGS.into_iter().map(|s| s.to_string()).collect(),
                ))
            }
        }
        _ => Ok((
            v,
            DEFAULT_LANGS.into_iter().map(|s| s.to_string()).collect(),
        )),
    }
}

#[post("/labelize")]
async fn labelize_route(req_body: String) -> impl Responder {
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = _labelize_json(val).await;
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[post("/debug")]
async fn debug_route(req_body: String) -> impl Responder {
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = _labelize_json(val).await;
    let val: IntermediateForm = val.into();
    use std::io::Write;
    writeln!(
        std::fs::File::create("./log/debug.json").unwrap(),
        "{}",
        val.clone().choose_lang(&langs)
    )
    .unwrap();
    let val = val.compress_monolingual();
    let val = val.drop_array_item_types();
    writeln!(
        std::fs::File::create("./log/debug2.json").unwrap(),
        "{}",
        val.clone().choose_lang(&langs)
    )
    .unwrap();
    let val: CompactValue = val.into();
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[post("/compactify")]
async fn compactify_route(req_body: String) -> impl Responder {
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = _labelize_json(val).await;
    let val: IntermediateForm = val.into();
    let val = val.compress_monolingual();
    let val = val.drop_array_item_types();
    let val: CompactValue = val.into();
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(hello)
            .service(labelize_route)
            .service(compactify_route)
            .service(debug_route)
    })
    .bind(("127.0.0.1", 3939))?
    .run()
    .await
}
