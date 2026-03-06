use actix_cors::Cors;
use actix_web::{get, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Facility {
    facility_id: Option<String>,
    facility_name: Option<String>,
    facility_type_description: Option<String>,
    facility_description: Option<String>,
    #[serde(rename = "GEOJSON")]
    geojson: Option<serde_json::Value>,
    addresses: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct RidbResponse {
    #[serde(rename = "RECDATA")]
    recdata: Vec<Facility>,
}

#[derive(Debug, Serialize)]
struct Campground {
    id: String,
    name: String,
    description: String,
}

#[get("/campgrounds")]
async fn get_campgrounds() -> impl Responder {
    let api_key = match std::env::var("RIDB_API_KEY") {
        Ok(k) => k,
        Err(_) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "RIDB_API_KEY not set"}))
        }
    };

    let client = reqwest::Client::new();
    let mut all_campgrounds: Vec<Campground> = Vec::new();
    let mut offset = 0usize;
    let limit = 50usize;

    loop {
        let url = format!(
            "https://ridb.recreation.gov/api/v1/facilities?state=CO&facilitytype=Campground&limit={}&offset={}&apikey={}",
            limit, offset, api_key
        );

        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Request error: {e}");
                return HttpResponse::BadGateway()
                    .json(serde_json::json!({"error": "Failed to reach RIDB API"}));
            }
        };

        let body: RidbResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Parse error: {e}");
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to parse RIDB response"}));
            }
        };

        let count = body.recdata.len();
        for f in body.recdata {
            all_campgrounds.push(Campground {
                id: f.facility_id.unwrap_or_default(),
                name: f.facility_name.unwrap_or_default(),
                description: f.facility_type_description.unwrap_or_default(),
            });
        }

        if count < limit {
            break;
        }
        offset += limit;
    }

    HttpResponse::Ok().json(all_campgrounds)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let port = 3001u16;
    println!("Backend listening on port {port}");

    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();

        App::new().wrap(cors).service(get_campgrounds)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
