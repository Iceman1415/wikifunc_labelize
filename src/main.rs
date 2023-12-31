use std::net::SocketAddr;

use serde_json::Value;

use actix_web::dev::Service;
use actix_web::http::header;
use actix_web::{route, App, HttpResponse, HttpServer, Responder};
use tracing::{debug, info};
use tracing_actix_web::TracingLogger;

use dotenv::dotenv;

const DOMAIN: &str = "https://wikifunctions.org/w";

#[route("/", method = "GET", method = "POST")]
async fn index() -> impl Responder {
    info!("index route");
    HttpResponse::Ok()
        .append_header(header::ContentType::html())
        .body(include_str!("../static/index.html"))
}

mod simple_value;
mod typed_form;
use typed_form::TypedForm;
mod intermediate_form;
use intermediate_form::IntermediateForm;
mod compact_key;
mod compact_value;
use compact_value::CompactValue;

mod labelize;
use labelize::labelize;

// default to english only
const DEFAULT_LANGS: [&str; 1] = ["Z1002"];

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
                            .iter()
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

#[route("/labelize", method = "GET", method = "POST")]
async fn labelize_route(req_body: String) -> impl Responder {
    info!("labelize route");
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = labelize(val).await;
    HttpResponse::Ok().json(val.choose_lang(&langs))
}

#[route("/debug", method = "GET", method = "POST")]
async fn debug_route(req_body: String) -> impl Responder {
    info!("debug route");
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = labelize(val).await;
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

#[route("/compactify", method = "GET", method = "POST")]
async fn compactify_route(req_body: String) -> impl Responder {
    info!("compactify route");
    let (val, langs) = match request_wrapper(req_body) {
        Ok((val, langs)) => (val, langs),
        Err(r) => return r,
    };
    let val = labelize(val).await;
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
            .wrap_fn(|req, srv| {
                let fut = srv.call(req);
                async {
                    info!("recieved request");
                    let res = fut.await?;
                    Ok(res)
                }
            })
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
    dotenv().ok();
    init_telemetry();

    run_server().await?;
    Ok(())
}
