use std::sync::LazyLock;

use backend::embedding::EmbeddingModel;
use pgvector::Vector;
use regex::Regex;
use serde::Deserialize;
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Unified error type
// ---------------------------------------------------------------------------

type SeedResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ---------------------------------------------------------------------------
// Regex statics — compiled once for the lifetime of the process
// ---------------------------------------------------------------------------

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("invalid TAG_RE"));

static ENTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(?:[a-zA-Z]+|#\d+);").expect("invalid ENTITY_RE"));

static WHITESPACE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("invalid WHITESPACE_RE"));

// ---------------------------------------------------------------------------
// JSON types matching the Recreation.gov RIDB payload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RidbPayload {
    #[serde(rename = "RECDATA")]
    records: Vec<RidbFacility>,
}

#[derive(Debug, Deserialize)]
struct RidbFacility {
    #[serde(rename = "FacilityID")]
    id: String,

    #[serde(rename = "FacilityName")]
    name: String,

    // Optional in the real API — default to empty string when absent/null.
    #[serde(rename = "FacilityDescription", default)]
    description: String,

    #[serde(rename = "FacilityKeywords", default)]
    keywords: String,

    #[serde(rename = "FacilityLatitude", default)]
    lat: f64,

    #[serde(rename = "FacilityLongitude", default)]
    lon: f64,

    #[serde(rename = "FacilityReservable", default)]
    is_reservable: bool,

    #[serde(rename = "FacilityTypeDescription", default)]
    facility_type: String,
}

// ---------------------------------------------------------------------------
// A clean, flat record ready for insertion
// ---------------------------------------------------------------------------

struct FacilityRecord {
    id: String,
    name: String,
    description: String,
    embedding_text: String,
    lat: f64,
    lon: f64,
    is_reservable: bool,
    facility_type: String,
    state: String,
}

// ---------------------------------------------------------------------------
// HTML stripping
// ---------------------------------------------------------------------------

fn strip_html(raw: &str) -> String {
    let no_tags = TAG_RE.replace_all(raw, " ");
    let no_entities = ENTITY_RE.replace_all(&no_tags, " ");
    WHITESPACE_RE
        .replace_all(no_entities.trim(), " ")
        .into_owned()
}

// ---------------------------------------------------------------------------
// RIDB API fetching — paginates until all facilities across all states are retrieved
// ---------------------------------------------------------------------------

const STATES: &[&str] = &[
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN",
    "IA", "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV",
    "NH", "NJ", "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN",
    "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY",
];

async fn fetch_all_facilities(api_key: &str) -> SeedResult<Vec<FacilityRecord>> {
    let client = reqwest::Client::new();
    let mut all: Vec<FacilityRecord> = Vec::new();
    let limit = 50usize;

    for &state in STATES {
        let mut offset = 0usize;
        println!("Seeder: fetching facilities for state: {}", state);

        loop {
            let url = format!(
                "https://ridb.recreation.gov/api/v1/facilities\
                 ?state={}&limit={}&offset={}&apikey={}",
                state, limit, offset, api_key
            );

            let resp = client.get(&url).send().await?;
            if !resp.status().is_success() {
                return Err(format!("RIDB API returned {}", resp.status()).into());
            }

            let payload: RidbPayload = resp.json().await?;
            let count = payload.records.len();

            let records: Vec<FacilityRecord> = payload
                .records
                .into_iter()
                .map(|f| {
                    let clean_desc = strip_html(&f.description);
                    let embedding_text = format!("{} {} {}", f.name, clean_desc, f.keywords);
                    FacilityRecord {
                        id: f.id,
                        name: f.name,
                        description: clean_desc,
                        embedding_text,
                        lat: f.lat,
                        lon: f.lon,
                        is_reservable: f.is_reservable,
                        facility_type: f.facility_type,
                        state: state.to_string(),
                    }
                })
                .collect();

            all.extend(records);

            if count < limit {
                break;
            }
            offset += limit;
        }
    }

    println!("Seeder: {} total facilities fetched", all.len());
    Ok(all)
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

async fn connect() -> SeedResult<PgPool> {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    Ok(PgPool::connect(&url).await?)
}

async fn run_schema(pool: &PgPool) -> SeedResult<()> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(pool)
        .await?;

    // IF NOT EXISTS — safe to re-run; existing rows are preserved.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS facilities (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            description   TEXT NOT NULL,
            embedding     vector(384),
            lat           FLOAT8,
            lon           FLOAT8,
            is_reservable BOOLEAN,
            type          TEXT,
            state         TEXT
        )",
    )
    .execute(pool)
    .await?;

    // Add columns introduced after the initial schema — safe no-op if they already exist.
    sqlx::query("ALTER TABLE facilities ADD COLUMN IF NOT EXISTS state TEXT")
        .execute(pool)
        .await?;

    Ok(())
}

async fn seed(
    pool: &PgPool,
    records: &[FacilityRecord],
    model: &mut EmbeddingModel,
) -> SeedResult<usize> {
    let mut tx = pool.begin().await?;
    let mut inserted = 0usize;

    for r in records {
        let embedding = Vector::from(model.embed(&r.embedding_text)?);

        let rows_affected = sqlx::query(
            "INSERT INTO facilities (id, name, description, embedding, lat, lon, is_reservable, type, state)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&r.id)
        .bind(&r.name)
        .bind(&r.description)
        .bind(embedding)
        .bind(r.lat)
        .bind(r.lon)
        .bind(r.is_reservable)
        .bind(&r.facility_type)
        .bind(&r.state)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        inserted += rows_affected as usize;
    }

    tx.commit().await?;
    Ok(inserted)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- strip_html ---

    #[test]
    fn test_strip_html_removes_block_tags() {
        assert_eq!(strip_html("<h2>Hello</h2>"), "Hello");
    }

    #[test]
    fn test_strip_html_removes_self_closing_tags() {
        assert_eq!(strip_html("Line1<br />Line2"), "Line1 Line2");
    }

    #[test]
    fn test_strip_html_removes_nested_tags() {
        assert_eq!(strip_html("<p><strong>Bold</strong> text</p>"), "Bold text");
    }

    #[test]
    fn test_strip_html_replaces_entities_with_space() {
        assert_eq!(strip_html("Hello &amp; World"), "Hello World");
        assert_eq!(strip_html("a &lt; b"), "a b");
    }

    #[test]
    fn test_strip_html_collapses_whitespace() {
        assert_eq!(strip_html("<p>Hello</p><p>World</p>"), "Hello World");
    }

    #[test]
    fn test_strip_html_trims_leading_trailing_whitespace() {
        assert_eq!(strip_html("  <p>text</p>  "), "text");
    }

    #[test]
    fn test_strip_html_empty_input() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn test_strip_html_plain_text_passthrough() {
        assert_eq!(strip_html("just plain text"), "just plain text");
    }

    // --- JSON deserialization ---

    #[test]
    fn test_ridb_facility_deserializes_all_fields() {
        let json = serde_json::json!({
            "FacilityID": "233115",
            "FacilityName": "Rampart Range Campground",
            "FacilityDescription": "<p>Great views</p>",
            "FacilityKeywords": "camping, hiking",
            "FacilityLatitude": 39.1834,
            "FacilityLongitude": -104.8612,
            "FacilityReservable": true,
            "FacilityTypeDescription": "Campground"
        });
        let f: RidbFacility = serde_json::from_value(json).unwrap();
        assert_eq!(f.id, "233115");
        assert_eq!(f.name, "Rampart Range Campground");
        assert!((f.lat - 39.1834).abs() < 1e-6);
        assert!((f.lon - -104.8612).abs() < 1e-6);
        assert!(f.is_reservable);
        assert_eq!(f.facility_type, "Campground");
    }

    /// The real RIDB API sometimes omits optional fields — serde defaults must fill them in.
    #[test]
    fn test_ridb_facility_deserializes_with_missing_optional_fields() {
        let json = serde_json::json!({
            "FacilityID": "99999",
            "FacilityName": "Mystery Camp"
        });
        let f: RidbFacility = serde_json::from_value(json).unwrap();
        assert_eq!(f.id, "99999");
        assert_eq!(f.description, "");
        assert_eq!(f.keywords, "");
        assert_eq!(f.lat, 0.0);
        assert_eq!(f.lon, 0.0);
        assert!(!f.is_reservable);
        assert_eq!(f.facility_type, "");
    }

    #[test]
    fn test_ridb_payload_deserializes_multiple_records() {
        let json = serde_json::json!({
            "RECDATA": [
                {
                    "FacilityID": "1", "FacilityName": "Camp A",
                    "FacilityDescription": "", "FacilityKeywords": "",
                    "FacilityLatitude": 40.0, "FacilityLongitude": -105.0,
                    "FacilityReservable": false, "FacilityTypeDescription": "Campground"
                },
                {
                    "FacilityID": "2", "FacilityName": "Camp B",
                    "FacilityDescription": "", "FacilityKeywords": "",
                    "FacilityLatitude": 38.0, "FacilityLongitude": -106.0,
                    "FacilityReservable": true, "FacilityTypeDescription": "Campground"
                }
            ]
        });
        let payload: RidbPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.records.len(), 2);
        assert_eq!(payload.records[0].id, "1");
        assert_eq!(payload.records[1].id, "2");
    }

    // --- embedding_text construction ---

    #[test]
    fn test_embedding_text_format() {
        let name = "Test Camp";
        let clean_desc = strip_html("<p>A nice campground</p>");
        let keywords = "camping hiking";
        let text = format!("{} {} {}", name, clean_desc, keywords);
        assert_eq!(text, "Test Camp A nice campground camping hiking");
    }

    // --- L2 normalisation ---

    #[test]
    fn test_l2_normalize_scales_to_unit_sphere() {
        let mut v = vec![0.0f32; 384];
        v[0] = 3.0;
        v[1] = 4.0;
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-12 {
            for x in &mut v {
                *x /= norm;
            }
        }
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
        assert!(v[2..].iter().all(|&x| x == 0.0));
        let new_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((new_norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero_vector_unchanged() {
        let mut v = vec![0.0f32; 384];
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-12 {
            for x in &mut v {
                *x /= norm;
            }
        }
        assert!(v.iter().all(|&x| x == 0.0));
    }

    // --- mean pooling ---

    #[test]
    fn test_mean_pool_uniform_mask() {
        let hidden: &[f32] = &[
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
        ];
        let mask_f = vec![1.0f32; 3];
        let dims = 4usize;
        let mask_sum: f32 = mask_f.iter().sum();
        let mut pooled = vec![0.0f32; dims];
        for (i, &w) in mask_f.iter().enumerate() {
            for d in 0..dims {
                pooled[d] += hidden[i * dims + d] * w;
            }
        }
        for x in &mut pooled {
            *x /= mask_sum;
        }
        let third = 1.0f32 / 3.0;
        assert!((pooled[0] - third).abs() < 1e-6);
        assert!((pooled[1] - third).abs() < 1e-6);
        assert!((pooled[2] - third).abs() < 1e-6);
        assert_eq!(pooled[3], 0.0);
    }

    #[test]
    fn test_mean_pool_masked_token_excluded() {
        let hidden: &[f32] = &[3.0, 6.0, 9.0, 9.0];
        let mask_f = vec![1.0f32, 0.0];
        let dims = 2usize;
        let mask_sum: f32 = mask_f.iter().sum();
        let mut pooled = vec![0.0f32; dims];
        for (i, &w) in mask_f.iter().enumerate() {
            for d in 0..dims {
                pooled[d] += hidden[i * dims + d] * w;
            }
        }
        for x in &mut pooled {
            *x /= mask_sum;
        }
        assert!((pooled[0] - 3.0).abs() < 1e-6);
        assert!((pooled[1] - 6.0).abs() < 1e-6);
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> SeedResult<()> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("RIDB_API_KEY").expect("RIDB_API_KEY must be set");

    println!("Seeder: fetching facilities across all states from RIDB API");
    let records = fetch_all_facilities(&api_key).await?;

    println!("Seeder: loading embedding model");
    let mut model = EmbeddingModel::load()?;
    println!("Seeder: model ready");

    let pool = connect().await?;
    println!("Seeder: connected to database");

    run_schema(&pool).await?;
    println!("Seeder: schema ready");

    let inserted = seed(&pool, &records, &mut model).await?;
    println!(
        "Seeder: done — {} new rows inserted, {} already existed",
        inserted,
        records.len() - inserted
    );

    Ok(())
}
