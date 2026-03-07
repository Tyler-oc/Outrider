use std::sync::LazyLock;

use backend::embedding::EmbeddingModel;
use chrono::NaiveDate;
use pgvector::Vector;
use regex::Regex;
use serde::Deserialize;
use sqlx::{PgPool, Row};

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

    // Raw date string from the API, e.g. "2024-03-15" or "2024-03-15T00:00:00Z".
    // We parse it manually rather than relying on serde so we can handle both formats.
    #[serde(rename = "LastUpdatedDate", default)]
    last_updated_raw: String,
}

// ---------------------------------------------------------------------------
// A clean, flat record ready for upsert
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
    /// None for delta-fetched records where state context is unavailable.
    /// The DB upsert does not overwrite the stored state, so existing rows keep it.
    state: Option<String>,
    last_updated: NaiveDate,
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
// Date parsing — handles both "YYYY-MM-DD" and "YYYY-MM-DDTHH:MM:SSZ"
// Falls back to today's date so the record is still ingested.
// ---------------------------------------------------------------------------

fn parse_date(s: &str) -> NaiveDate {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ").map(|dt| dt.date())
        })
        .unwrap_or_else(|_| chrono::Local::now().date_naive())
}

// ---------------------------------------------------------------------------
// Convert a raw API record into an insertion-ready FacilityRecord.
// `state` is Some("CO") for full-fetch records, None for delta-fetch records.
// ---------------------------------------------------------------------------

fn to_record(f: RidbFacility, state: Option<&str>) -> FacilityRecord {
    let clean_desc = strip_html(&f.description);
    let embedding_text = format!("{} {} {}", f.name, clean_desc, f.keywords);
    let last_updated = if f.last_updated_raw.is_empty() {
        chrono::Local::now().date_naive()
    } else {
        parse_date(&f.last_updated_raw)
    };
    FacilityRecord {
        id: f.id,
        name: f.name,
        description: clean_desc,
        embedding_text,
        lat: f.lat,
        lon: f.lon,
        is_reservable: f.is_reservable,
        facility_type: f.facility_type,
        state: state.map(|s| s.to_string()),
        last_updated,
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

const STATES: &[&str] = &[
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN",
    "IA", "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV",
    "NH", "NJ", "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN",
    "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY",
];

async fn fetch_page(client: &reqwest::Client, url: &str) -> SeedResult<RidbPayload> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(format!("RIDB API returned {}", resp.status()).into());
    }
    Ok(resp.json().await?)
}

/// Full historical fetch — iterates all 50 states with pagination.
/// Used when the DB is empty (no high-water mark).
async fn fetch_full(api_key: &str) -> SeedResult<Vec<FacilityRecord>> {
    let client = reqwest::Client::new();
    let mut all: Vec<FacilityRecord> = Vec::new();
    let limit = 50usize;

    for &state in STATES {
        let mut offset = 0usize;
        let mut state_count = 0usize;

        loop {
            let url = format!(
                "https://ridb.recreation.gov/api/v1/facilities\
                 ?state={}&limit={}&offset={}&apikey={}",
                state, limit, offset, api_key
            );
            let payload = fetch_page(&client, &url).await?;
            let count = payload.records.len();
            state_count += count;

            all.extend(payload.records.into_iter().map(|f| to_record(f, Some(state))));

            if count < limit {
                break;
            }
            offset += limit;
        }

        println!(
            "Seeder: [full] {} — {} facilities ({} total)",
            state,
            state_count,
            all.len()
        );
    }

    Ok(all)
}

/// Delta fetch — queries only records updated on or after `since`, across all states.
/// Used when we have an existing high-water mark in the DB.
async fn fetch_delta(api_key: &str, since: NaiveDate) -> SeedResult<Vec<FacilityRecord>> {
    let client = reqwest::Client::new();
    let mut all: Vec<FacilityRecord> = Vec::new();
    let mut offset = 0usize;
    let limit = 50usize;
    let since_str = since.format("%Y-%m-%d").to_string();

    loop {
        let url = format!(
            "https://ridb.recreation.gov/api/v1/facilities\
             ?lastupdated={}&limit={}&offset={}&apikey={}",
            since_str, limit, offset, api_key
        );
        let payload = fetch_page(&client, &url).await?;
        let count = payload.records.len();

        // State is unknown in delta context; the upsert preserves the stored value.
        all.extend(payload.records.into_iter().map(|f| to_record(f, None)));

        if count < limit {
            break;
        }
        offset += limit;
    }

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
            state         TEXT,
            last_updated  DATE
        )",
    )
    .execute(pool)
    .await?;

    // Migrations for columns added after the initial schema.
    for sql in [
        "ALTER TABLE facilities ADD COLUMN IF NOT EXISTS state TEXT",
        "ALTER TABLE facilities ADD COLUMN IF NOT EXISTS last_updated DATE",
    ] {
        sqlx::query(sql).execute(pool).await?;
    }

    Ok(())
}

/// Returns the most recent `last_updated` date in the DB, or None if the
/// table is empty. This is the high-water mark that drives delta vs full sync.
async fn get_high_water_mark(pool: &PgPool) -> SeedResult<Option<NaiveDate>> {
    let row = sqlx::query("SELECT MAX(last_updated) AS hwm FROM facilities")
        .fetch_one(pool)
        .await?;
    Ok(row.get("hwm"))
}

/// Embeds each record and upserts it into the DB.
/// Commits in batches of BATCH_SIZE to bound transaction memory and give
/// progress feedback during long initial loads.
async fn upsert(
    pool: &PgPool,
    records: &[FacilityRecord],
    model: &mut EmbeddingModel,
) -> SeedResult<usize> {
    const BATCH_SIZE: usize = 500;
    let mut total_upserted = 0usize;

    for (batch_idx, chunk) in records.chunks(BATCH_SIZE).enumerate() {
        let mut tx = pool.begin().await?;

        for r in chunk {
            let embedding = Vector::from(model.embed(&r.embedding_text)?);

            sqlx::query(
                "INSERT INTO facilities
                    (id, name, description, embedding, lat, lon, is_reservable, type, state, last_updated)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (id) DO UPDATE SET
                    name         = EXCLUDED.name,
                    description  = EXCLUDED.description,
                    embedding    = EXCLUDED.embedding,
                    last_updated = EXCLUDED.last_updated",
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
            .bind(r.last_updated)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        total_upserted += chunk.len();

        println!(
            "Seeder: batch {} committed — {}/{} records processed",
            batch_idx + 1,
            total_upserted,
            records.len()
        );
    }

    Ok(total_upserted)
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

    // --- parse_date ---

    #[test]
    fn test_parse_date_iso_date() {
        let d = parse_date("2024-03-15");
        assert_eq!(d, NaiveDate::from_ymd_opt(2024, 3, 15).unwrap());
    }

    #[test]
    fn test_parse_date_iso_datetime() {
        let d = parse_date("2024-03-15T00:00:00Z");
        assert_eq!(d, NaiveDate::from_ymd_opt(2024, 3, 15).unwrap());
    }

    #[test]
    fn test_parse_date_invalid_falls_back_to_today() {
        let d = parse_date("not-a-date");
        assert_eq!(d, chrono::Local::now().date_naive());
    }

    #[test]
    fn test_parse_date_trims_whitespace() {
        let d = parse_date("  2024-06-01  ");
        assert_eq!(d, NaiveDate::from_ymd_opt(2024, 6, 1).unwrap());
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
            "FacilityTypeDescription": "Campground",
            "LastUpdatedDate": "2024-03-15"
        });
        let f: RidbFacility = serde_json::from_value(json).unwrap();
        assert_eq!(f.id, "233115");
        assert_eq!(f.name, "Rampart Range Campground");
        assert!((f.lat - 39.1834).abs() < 1e-6);
        assert!((f.lon - -104.8612).abs() < 1e-6);
        assert!(f.is_reservable);
        assert_eq!(f.facility_type, "Campground");
        assert_eq!(f.last_updated_raw, "2024-03-15");
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
        assert_eq!(f.last_updated_raw, "");
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

    // --- to_record ---

    #[test]
    fn test_to_record_strips_html_and_builds_embedding_text() {
        let f = RidbFacility {
            id: "1".into(),
            name: "Test Camp".into(),
            description: "<p>A nice campground</p>".into(),
            keywords: "camping hiking".into(),
            lat: 0.0,
            lon: 0.0,
            is_reservable: false,
            facility_type: "Campground".into(),
            last_updated_raw: "2024-01-01".into(),
        };
        let r = to_record(f, Some("CO"));
        assert_eq!(r.description, "A nice campground");
        assert_eq!(r.embedding_text, "Test Camp A nice campground camping hiking");
        assert_eq!(r.state.as_deref(), Some("CO"));
        assert_eq!(r.last_updated, NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
    }

    #[test]
    fn test_to_record_none_state_for_delta() {
        let f = RidbFacility {
            id: "2".into(),
            name: "Delta Camp".into(),
            description: "".into(),
            keywords: "".into(),
            lat: 0.0,
            lon: 0.0,
            is_reservable: false,
            facility_type: "".into(),
            last_updated_raw: "2024-06-01".into(),
        };
        let r = to_record(f, None);
        assert!(r.state.is_none());
    }

    #[test]
    fn test_to_record_missing_date_falls_back_to_today() {
        let f = RidbFacility {
            id: "3".into(),
            name: "Camp".into(),
            description: "".into(),
            keywords: "".into(),
            lat: 0.0,
            lon: 0.0,
            is_reservable: false,
            facility_type: "".into(),
            last_updated_raw: "".into(),
        };
        let r = to_record(f, None);
        assert_eq!(r.last_updated, chrono::Local::now().date_naive());
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

    // DB must be ready before anything else — schema setup and high-water mark
    // query both happen before we touch the API or load the model.
    let pool = connect().await?;
    println!("Seeder: connected to database");

    run_schema(&pool).await?;
    println!("Seeder: schema ready");

    // --- High-water mark ---
    let hwm = get_high_water_mark(&pool).await?;
    match hwm {
        Some(d) => println!("Seeder: high-water mark = {d} — running delta sync"),
        None => println!("Seeder: no existing data — running full historical sync"),
    }

    // --- Fetch ---
    let records = match hwm {
        Some(since) => {
            println!("Seeder: fetching records updated since {since}");
            let r = fetch_delta(&api_key, since).await?;
            println!("Seeder: {} records returned by delta query", r.len());
            r
        }
        None => {
            println!("Seeder: fetching all facilities across all states");
            let r = fetch_full(&api_key).await?;
            println!("Seeder: {} total facilities fetched", r.len());
            r
        }
    };

    if records.is_empty() {
        println!("Seeder: nothing to do — database is already up to date");
        return Ok(());
    }

    // --- Embed & upsert ---
    println!("Seeder: loading embedding model");
    let mut model = EmbeddingModel::load()?;
    println!("Seeder: model ready — beginning embed + upsert");

    let upserted = upsert(&pool, &records, &mut model).await?;
    println!("Seeder: done — {upserted} rows upserted");

    Ok(())
}
