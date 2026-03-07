use std::sync::Mutex;

use actix_cors::Cors;
use actix_web::{get, web, web::Data, App, HttpResponse, HttpServer, Responder};
use backend::embedding::EmbeddingModel;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, QueryBuilder, Postgres};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Facility {
    // PascalCase would produce "FacilityId", but the RIDB API sends "FacilityID".
    #[serde(rename = "FacilityID")]
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
struct FacilityResult {
    id: String,
    name: String,
    description: String,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    state: Option<String>,
    facility_type: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    radius_miles: Option<f64>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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
    let mut all_campgrounds: Vec<FacilityResult> = Vec::new();
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
            all_campgrounds.push(FacilityResult {
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

#[get("/search")]
async fn search(
    params: web::Query<SearchQuery>,
    pool: Data<PgPool>,
    model: Data<Mutex<EmbeddingModel>>,
) -> impl Responder {
    let q = params.q.trim().to_string();
    if q.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"error": "q must not be empty"}));
    }

    let embedding = {
        let mut m = match model.lock() {
            Ok(m) => m,
            Err(_) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Embedding model unavailable"}))
            }
        };
        match m.embed(&q) {
            Ok(v) => Vector::from(v),
            Err(e) => {
                eprintln!("Embed error: {e}");
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to generate embedding"}));
            }
        }
    };

    let mut builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT id, name, description FROM facilities WHERE 1=1 ");

    if let Some(state) = &params.state {
        if !state.is_empty() {
            builder.push(" AND state = ");
            builder.push_bind(state.to_uppercase());
        }
    }

    if let Some(fatype) = &params.facility_type {
        if !fatype.is_empty() {
            builder.push(" AND type ILIKE ");
            builder.push_bind(format!("%{}%", fatype));
        }
    }

    if let (Some(lat), Some(lon), Some(rad)) = (params.lat, params.lon, params.radius_miles) {
        // Haversine formula directly in PostgreSQL for radius filter in miles
        builder.push(" AND (3959 * acos(LEAST(1.0, cos(radians(");
        builder.push_bind(lat);
        builder.push(")) * cos(radians(lat)) * cos(radians(lon) - radians(");
        builder.push_bind(lon);
        builder.push(")) + sin(radians(");
        builder.push_bind(lat);
        builder.push(")) * sin(radians(lat))))) <= ");
        builder.push_bind(rad);
    }

    builder.push(" ORDER BY embedding <=> ");
    builder.push_bind(embedding);
    builder.push(" LIMIT 50");

    let rows = match builder.build().fetch_all(pool.get_ref()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("DB error: {e}");
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Database query failed"}));
        }
    };

    let results: Vec<FacilityResult> = rows
        .iter()
        .map(|r| FacilityResult {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
        })
        .collect();

    HttpResponse::Ok().json(results)
}

// ---------------------------------------------------------------------------
// App setup
// ---------------------------------------------------------------------------

async fn create_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPool::connect(&url)
        .await
        .expect("Failed to connect to database")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let pool = create_pool().await;

    let model = EmbeddingModel::load().expect("Failed to load embedding model");
    let model_data = Data::new(Mutex::new(model));

    let port = 3001u16;
    println!("Backend listening on port {port}");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();

        App::new()
            .app_data(Data::new(pool.clone()))
            .app_data(model_data.clone())
            .wrap(cors)
            .service(get_campgrounds)
            .service(search)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    // --- handler tests ---

    /// Missing RIDB_API_KEY must yield a 500 immediately, before any HTTP call
    /// is attempted. This verifies the env-guard at the top of the handler.
    #[actix_web::test]
    async fn test_missing_api_key_returns_500() {
        std::env::remove_var("RIDB_API_KEY");
        let app = test::init_service(App::new().service(get_campgrounds)).await;
        let req = test::TestRequest::get().uri("/campgrounds").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 500);
    }

    // --- Facility deserialization ---

    /// All fields present — standard RIDB response shape.
    #[actix_web::test]
    async fn test_facility_deserializes_full_record() {
        let json = serde_json::json!({
            "FacilityID": "233115",
            "FacilityName": "Rampart Range",
            "FacilityTypeDescription": "Campground",
            "FacilityDescription": "<p>Great views</p>",
            "GEOJSON": null,
            "Addresses": []
        });
        let f: Facility = serde_json::from_value(json).unwrap();
        assert_eq!(f.facility_id.as_deref(), Some("233115"));
        assert_eq!(f.facility_name.as_deref(), Some("Rampart Range"));
        assert_eq!(f.facility_type_description.as_deref(), Some("Campground"));
    }

    /// All optional fields absent — serde must not error and all fields must be None.
    #[actix_web::test]
    async fn test_facility_deserializes_with_all_nulls() {
        let json = serde_json::json!({
            "FacilityID": null,
            "FacilityName": null,
            "FacilityTypeDescription": null,
            "FacilityDescription": null,
            "GEOJSON": null,
            "Addresses": null
        });
        let f: Facility = serde_json::from_value(json).unwrap();
        assert!(f.facility_id.is_none());
        assert!(f.facility_name.is_none());
        assert!(f.facility_type_description.is_none());
    }

    // --- RidbResponse deserialization ---

    #[actix_web::test]
    async fn test_ridb_response_deserializes_empty_recdata() {
        let json = serde_json::json!({ "RECDATA": [] });
        let r: RidbResponse = serde_json::from_value(json).unwrap();
        assert!(r.recdata.is_empty());
    }

    #[actix_web::test]
    async fn test_ridb_response_deserializes_multiple_facilities() {
        let json = serde_json::json!({
            "RECDATA": [
                { "FacilityID": "1", "FacilityName": "A", "FacilityTypeDescription": null,
                  "FacilityDescription": null, "GEOJSON": null, "Addresses": null },
                { "FacilityID": "2", "FacilityName": "B", "FacilityTypeDescription": null,
                  "FacilityDescription": null, "GEOJSON": null, "Addresses": null }
            ]
        });
        let r: RidbResponse = serde_json::from_value(json).unwrap();
        assert_eq!(r.recdata.len(), 2);
        assert_eq!(r.recdata[0].facility_id.as_deref(), Some("1"));
        assert_eq!(r.recdata[1].facility_id.as_deref(), Some("2"));
    }

    // --- FacilityResult serialization ---

    #[actix_web::test]
    async fn test_campground_serializes_to_json() {
        let c = FacilityResult {
            id: "233115".to_string(),
            name: "Rampart Range".to_string(),
            description: "Campground".to_string(),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["id"], "233115");
        assert_eq!(v["name"], "Rampart Range");
        assert_eq!(v["description"], "Campground");
    }

    // --- FacilityResult construction from Facility ---

    /// None fields on a Facility must produce empty strings in the FacilityResult,
    /// matching the `unwrap_or_default()` calls in the handler.
    #[actix_web::test]
    async fn test_campground_defaults_on_none_facility_fields() {
        let f = Facility {
            facility_id: None,
            facility_name: None,
            facility_type_description: None,
            facility_description: None,
            geojson: None,
            addresses: None,
        };
        let c = FacilityResult {
            id: f.facility_id.unwrap_or_default(),
            name: f.facility_name.unwrap_or_default(),
            description: f.facility_type_description.unwrap_or_default(),
        };
        assert_eq!(c.id, "");
        assert_eq!(c.name, "");
        assert_eq!(c.description, "");
    }
}
