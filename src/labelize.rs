use std::collections::BTreeSet;

use derive_more::Display;
use regex::Regex;

use actix_web::HttpResponse;
use actix_web::{error::ResponseError, http::header::ContentType};
use async_recursion::async_recursion;
use tracing::{debug, trace, warn};

use cached::proc_macro::cached;
use futures::future::{self, Shared};
use futures::{Future, FutureExt};
use std::pin::Pin;

use crate::simple_value::{LabelledNode, SimpleValue, StringType};
use serde_json::Value;

use crate::DOMAIN;

#[derive(Debug, PartialEq, Clone, Display)]
enum MyError {
    #[display(fmt = "network error: {}", _0)]
    NetworkError(String),
    #[display(fmt = "schema error: {}", _0)]
    SchemaError(String),
}

impl ResponseError for MyError {
    fn status_code(&self) -> reqwest::StatusCode {
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    }

    fn error_response(&self) -> HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(self.to_string())
    }
}

async fn _fetch(z_number: String) -> std::result::Result<Value, MyError> {
    debug!("fetching from wikifunction: {}", z_number);
    match reqwest::get(format!("{}/api.php?action=query&format=json&list=wikilambdaload_zobjects&wikilambdaload_zids={}&wikilambdaload_canonical=true", DOMAIN, &z_number)).await {
        Ok(res) => {
            debug!("fetched from wikifunction: {}", z_number);
            Ok(
                serde_json::from_str::<Value>(&res.text().await.unwrap())
                    .map_err(|_e| MyError::SchemaError("failed parsing wikifunction response".to_string()))?
                    .get("query")
                    .ok_or(MyError::SchemaError("no \"query\" key in wikifunction response".to_string()))?
                    .get("wikilambdaload_zobjects")
                    .ok_or(MyError::SchemaError("no \"wikilambdaload_zobjects\" key in wikifunction response".to_string()))?
                    .get(&z_number)
                    .ok_or(MyError::SchemaError(format!("no key for self ({}) in wikifunction response", z_number)))?
                    .get("data")
                    .ok_or(MyError::SchemaError("no \"data\" key in wikifunction response".to_string()))?
                    .to_owned()
            )
        },
        Err(e) => {
            warn!("error fetching {}: {}", z_number, e); 
            Err(MyError::NetworkError(e.to_string()))
        }
    }
}

// https://github.com/jaemk/cached/issues/81
#[cached(time = 600)]
fn fetch(
    z_number: String,
) -> Shared<Pin<Box<dyn Future<Output = std::result::Result<Value, MyError>> + std::marker::Send>>>
{
    return _fetch(z_number).boxed().shared();
}

async fn _labelize(s: String) -> std::result::Result<StringType, MyError> {
    trace!("labelize {}", s);
    if Regex::new(r"^Z\d+$").unwrap().is_match(&s) {
        let readable_labels = fetch(s.clone())
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
            .iter()
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
            .collect::<std::result::Result<_, MyError>>()?;
        Ok(StringType::LabelledNode(LabelledNode::from(
            readable_labels,
            s,
        )))
    } else if Regex::new(r"^Z\d+K\d+$").unwrap().is_match(&s) {
        let pat = s.split('K').collect::<Vec<_>>();
        let z_number = pat[0];
        // let k_number = pat[1].parse::<usize>().unwrap();

        let res = fetch(z_number.to_string()).await?;

        // example object: Z4, of type Z4
        // example object: Z811, of type Z8
        // example object: Z517, of type Z50
        // example: Z4K1 -> obj["Z2K2"]["Z4K2"][k_number]["Z3K3"]["Z12K1"][1]["Z11K2"]
        // example: Z8K1 -> obj["Z2K2"]["Z8K1"][k_number]["Z17K3"]["Z12K1"][1]["Z11K2"]
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
            // we now try to find the key-value in Z2K2, where the value is an array of objects
            .filter_map(|(_k, v)| v.as_array())
            .filter(|v| v.len() > 1 && v[1].is_object())
            // ...and one of the object has string value of matching ZxxxKyyy
            // or the object has an object value, which has a string value of matching ZxxxKyyy (one level of indirection)
            .filter_map(|v| {
                v.iter().filter_map(|x| x.as_object()).find(|o| {
                    o.iter().any(|(_k, v)| match v {
                        Value::String(vs) => vs.clone() == s,
                        Value::Object(vo) => {
                            vo.iter().any(|(_k, vv)| *vv == Value::String(s.clone()))
                        }
                        _ => false,
                    })
                })
            })
            .next()
            .unwrap()
            .iter()
            .filter_map(|(_k, v)| v.as_object())
            .find(|o| o.get("Z1K1") == Some(&Value::String("Z12".to_string())))
            .unwrap();

        let readable_labels = label_val
            .get("Z12K1")
            .ok_or(MyError::SchemaError(
                "no \"Z12K1\" key in wikifunction response".to_string(),
            ))?
            .as_array()
            .ok_or(MyError::SchemaError("Z12K1 is not an array".to_string()))?
            .iter()
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
                    format!(
                        "'{}'",
                        v.get("Z11K2")
                            .ok_or(MyError::SchemaError(
                                "no key Z11K1 in item of Z12K1".to_string(),
                            ))?
                            .as_str()
                            .ok_or(MyError::SchemaError("value of Z11K2 not a str".to_string()))?
                    ),
                ))
            })
            .collect::<std::result::Result<_, MyError>>()?;
        Ok(StringType::LabelledNode(LabelledNode::from(
            readable_labels,
            s,
        )))
    } else {
        Ok(StringType::String(s))
    }
}

async fn _labelize_wrapped(s: String) -> StringType {
    trace!("labelize wrapped {}", s);
    if s.is_empty() {
        return StringType::String(s);
    }
    match _labelize(s.clone()).await {
        Ok(out) => out,
        Err(err) => {
            warn!("error when parsing {}: {:?}", s, err);
            StringType::String(s)
        }
    }
}

#[async_recursion]
pub async fn labelize(v: Value) -> SimpleValue {
    trace!("_labelize_json {}", v);
    match v {
        Value::Null => unimplemented!(),
        Value::Bool(_b) => unimplemented!(),
        Value::Number(_n) => unimplemented!(),
        Value::String(s) => SimpleValue::StringType(_labelize_wrapped(s).await),
        Value::Array(a) => SimpleValue::Array(future::join_all(a.into_iter().map(labelize)).await),
        Value::Object(o) => SimpleValue::Object(BTreeSet::from_iter(
            future::join_all(
                o.into_iter()
                    .map(|(key, val)| future::join(_labelize_wrapped(key), labelize(val))),
            )
            .await,
        )),
    }
}
