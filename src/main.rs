use std::net::SocketAddr;

use serde_json::Value;

use actix_web::dev::Service;
use actix_web::{route, App, HttpResponse, HttpServer, Responder};
use tracing::{debug, info};
use tracing_actix_web::TracingLogger;

use dotenv::dotenv;

const DOMAIN: &str = "https://wikifunctions.beta.wmflabs.org/w";

#[route("/", method = "GET", method = "POST")]
async fn index() -> impl Responder {
    info!("index route");
    HttpResponse::Ok().body(r#"<!doctype html>
<html lang=en-US>
<meta charset=utf8>
<body>
    <div><h1>Wikifunction Labelize - API for Making ZObjects Human Readable</h1></div>
    <div><h2>Links</h2>
        <span><a href="https://documenter.getpostman.com/view/15455445/VUxVq42m">Usage: Postaman documentation</a></span>
        <span><a href="https://github.com/Iceman1415/wikifunc_labelize">Sourcecode</a></span>
    </div>
    <div><h2>GET /</h2><div>This help page</div></div>
    <div><h2>POST /labelize</h2>
        <div>Append human readable labels to all strings in the json body that are ZIDs (Zxxx) or Global Keys (ZxxxKyyy)</div>
        <div>By default, the entire json body is labelized, and the prefered language of human readable labels are in order: Japanese (Z1830), Chinese (Z1006), English (Z1002)</div>
        <div>Alternatively you can supply your own order of prefered language in the POST body, like so: <code>{"data": "zobject...", "langs": ["Z1830", "Z1006", "Z1002"]}</code></div>
    </div>
    <div><h2>POST /compacify</h2>
        <div>This tries to make the ZObject even more readable by simplifying its structure.</div>
        <div>The main transformation we do is that we "raise" the type (Z1K1) of ZObjects (all ZObjects has its type in the key Z1K1) and the type in Arrays (all Arrays have the type as the first element) upwards. In other words, we separate the type information from the rest of the data. The type information is merged into the key of objects instead.</div>
        <div>We also simplify commonly seen simple objects:<ul>
            <li>String (Z6)</li>
            <li>Reference (Z9)</li>
            <li>Monolingual Text (Z11)</li>
            <li>other objects that only have one key-value pair</li>
        </ul></div>
        <div>A custom order of prefered language can be provided in the POST body, similar to /labelize</div>
    </div>
    <div><h2>Notes</h2>
        <div><h3>Follow original HTTP Method</h3>
            <div>When using Postman, POST requests seems to be converted into GET requests on toolforge. The request may then fail if the payload is too large for a GET request. This problem seems to be solved when I enabled the setting for "Redirect with the original HTTP method instead of the default behavior of redirecting with GET."</div>
        </div>
        <div><h3>Processing time</h3>
            <div>The tool needs to fetch info about all the ZObjects mentioned in the request payload. Depending on the payload, fetching all these information may take 10 seconds or more.</div>
            <div>The results are cached for 10 minutes.</div>
        </div>
        <div><h3>Feedback wanted</h3>
            <div>This tool is still in active development (2022-09-04)</div>
            <div>Please do contact me and provide feedback, if the output is not what you expected.</div>
        </div>
    </div>
    <div><h2>Contact</h2><ul>
        <li>email: iceman1415@protonmail.com</li>
        <li>wikimedia / phabricator / etc: Iceman1415</li>
        <li>discord: Iceman#7876</li>
    </ul></div>
</body>
</html>"#)
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
