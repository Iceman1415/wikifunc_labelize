use std::collections::BTreeSet;
use std::net::SocketAddr;

use derive_more::Display;
use futures::future;
use regex::Regex;
use serde_json::Value;

use actix_web::{error::ResponseError, http::header::ContentType};
use actix_web::{get, post, App, HttpResponse, HttpServer, Responder};
use tracing::{debug, info, trace, warn};
use tracing_actix_web::TracingLogger;

use async_recursion::async_recursion;

use cached::proc_macro::cached;
use cached::TimedCache;

const DOMAIN: &str = "https://wikifunctions.beta.wmflabs.org/w";

#[get("/")]
async fn index() -> impl Responder {
    info!("index route");
    HttpResponse::Ok().body(r#"<body>
    <div><h2>GET /</h2><div>This help page</div></div>
    <div><h2>POST /labelize</h2>
        <div>Append human readable labels to all strings in the json body that are ZIDs (Zxxx) or Global Keys (ZxxxKyyy)</div>
        <div>By default, the entire json body is labelized, and the prefered language of human readable labels are in order: Japanese (Z1830), Chinese (Z1006), English (Z1002)</div>
        <div>Alternatively you can supply your own order of prefered language in the POST body, like so: <code>{"data": "zobject...", "langs": ["Z1830", "Z1006", "Z1002"]}</code></div>
    </div>
    <div><h2>POST /compacify</h2>
        <div>This tries to make the ZObject more readable by simplifying its structure.</div>
        <div>The main transformation we do is that we "raise" the type (Z1K1) of ZObjects (all ZObjects has its type in the key Z1K1) and the type in Arrays (all Arrays have the type as the first element) upwards. In other words, we separate the type information from the rest of the data. The type information is merged into the key of objects instead.</div>
        <div>We also simplify commonly seen simple objects:<ul>
            <li>String (Z6)</li>
            <li>Reference (Z9)</li>
            <li>Monolingual Text (Z11)</li>
            <li>other objects that only have one key-value pair</li>
        </ul></div>
        <div>A custom order of prefered language can be provided in the POST body, similar to /labelize</div>
    </div>
</body>"#)
}

mod simple_value;
use simple_value::{LabelledNode, SimpleValue, StringType};
mod typed_form;
use typed_form::TypedForm;
mod intermediate_form;
use intermediate_form::IntermediateForm;
mod compact_key;
mod compact_value;
use compact_value::CompactValue;

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

async fn fetch(z_number: &str) -> std::result::Result<Value, MyError> {
    debug!("fetching from wikifunction: {}", z_number);
    match reqwest::get(format!("{}/api.php?action=query&format=json&list=wikilambdaload_zobjects&wikilambdaload_zids={}&wikilambdaload_canonical=true",DOMAIN, z_number)).await {
        Ok(res) => {
            debug!("fetched from wikifunction: {}", z_number);
            Ok(
                serde_json::from_str::<Value>(&res.text().await.unwrap())
                    .map_err(|_e| MyError::SchemaError("failed parsing wikifunction response".to_string()))?
                    .get("query")
                    .ok_or(MyError::SchemaError("no \"query\" key in wikifunction response".to_string()))?
                    .get("wikilambdaload_zobjects")
                    .ok_or(MyError::SchemaError("no \"wikilambdaload_zobjects\" key in wikifunction response".to_string()))?
                    .get(z_number)
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

#[cached(
    type = "TimedCache<String, std::result::Result<StringType, MyError>>",
    create = "{ TimedCache:: with_lifespan_and_refresh(600, true) }",
    convert = r#"{ format!("{}", s) }"#
)]
async fn _labelize(s: String) -> std::result::Result<StringType, MyError> {
    trace!("labelize {}", s);
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
            .collect::<std::result::Result<_, MyError>>()?;
        Ok(StringType::LabelledNode(LabelledNode::from(
            readable_labels,
            s,
        )))
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
    if s == "" {
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
async fn _labelize_json(v: Value) -> SimpleValue {
    trace!("_labelize_json {}", v);
    match v {
        Value::Null => unimplemented!(),
        Value::Bool(_b) => unimplemented!(),
        Value::Number(_n) => unimplemented!(),
        Value::String(s) => SimpleValue::StringType(_labelize_wrapped(s).await),
        Value::Array(a) => {
            SimpleValue::Array(future::join_all(a.into_iter().map(|x| _labelize_json(x))).await)
        }
        Value::Object(o) => SimpleValue::Object(BTreeSet::from_iter(
            future::join_all(
                o.into_iter()
                    .map(|(key, val)| future::join(_labelize_wrapped(key), _labelize_json(val))),
            )
            .await,
        )),
    }
}

// the 3 languages (scripts) that I can read, arranged by ascending usage
const DEFAULT_LANGS: [&str; 3] = ["Z1830", "Z1006", "Z1002"];

fn request_wrapper(req_body: String) -> Result<(Value, Vec<String>), HttpResponse> {
    debug!("parsing req body");
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
    info!("labelize route");
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = _labelize_json(val).await;
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[post("/debug")]
async fn debug_route(req_body: String) -> impl Responder {
    info!("debug route");
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = _labelize_json(val).await;
    let val: TypedForm = val.into();
    use std::io::Write;
    writeln!(
        std::fs::File::create("./log/1_typed.json").unwrap(),
        "{}",
        val.clone().choose_lang(&langs)
    )
    .unwrap();
    let val: IntermediateForm = val.into();
    writeln!(
        std::fs::File::create("./log/2_intermediate.json").unwrap(),
        "{}",
        val.clone().choose_lang(&langs)
    )
    .unwrap();
    let val = val.compress_monolingual();
    let val = val.drop_array_item_types();
    writeln!(
        std::fs::File::create("./log/3_processed.json").unwrap(),
        "{}",
        val.clone().choose_lang(&langs)
    )
    .unwrap();
    let val: CompactValue = val.into();
    writeln!(
        std::fs::File::create("./log/4_compact.json").unwrap(),
        "{}",
        val.clone().choose_lang(&langs)
    )
    .unwrap();
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[post("/compactify")]
async fn compactify_route(req_body: String) -> impl Responder {
    info!("compactify route");
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = _labelize_json(val).await;
    let val = IntermediateForm::from(TypedForm::from(val));
    let val = val.compress_reference();
    let val = val.compress_string();
    let val = val.compress_monolingual();
    let val = val.drop_array_item_types();
    let val: CompactValue = val.into();
    let val = val.compress_simple_classes();
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[tracing::instrument]
async fn run_server() -> std::io::Result<()> {
    let addr: SocketAddr = "0.0.0.0:8000".parse().unwrap();
    info!("Listening on http://{}", addr);
    HttpServer::new(|| {
        App::new()
            .wrap(TracingLogger::default())
            .service(index)
            .service(labelize_route)
            .service(compactify_route)
            .service(debug_route)
    })
    .bind(addr)?
    .run()
    .await
}

mod tracing_utils;
use tracing_utils::init_telemetry;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_telemetry();

    run_server().await?;
    Ok(())
}
