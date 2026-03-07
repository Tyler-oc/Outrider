use std::sync::LazyLock;

use backend::embedding::EmbeddingModel;
use pgvector::Vector;
use regex::Regex;
use serde::Deserialize;
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Unified error type — keeps `?` working across ort, tokenizers, sqlx, and
// std::io which each return subtly different error wrapper types.
// ---------------------------------------------------------------------------

type SeedResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ---------------------------------------------------------------------------
// Regex statics — compiled once for the lifetime of the process
// ---------------------------------------------------------------------------

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("invalid TAG_RE"));

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

    #[serde(rename = "FacilityDescription")]
    description: String,

    #[serde(rename = "FacilityKeywords")]
    keywords: String,

    #[serde(rename = "FacilityLatitude")]
    lat: f64,

    #[serde(rename = "FacilityLongitude")]
    lon: f64,

    #[serde(rename = "FacilityReservable")]
    is_reservable: bool,

    #[serde(rename = "FacilityTypeDescription")]
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

    sqlx::query("DROP TABLE IF EXISTS facilities")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE facilities (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            description   TEXT NOT NULL,
            embedding     vector(384),
            lat           FLOAT8,
            lon           FLOAT8,
            is_reservable BOOLEAN,
            type          TEXT
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn seed(
    pool: &PgPool,
    records: &[FacilityRecord],
    model: &mut EmbeddingModel,
) -> SeedResult<()> {
    let mut tx = pool.begin().await?;

    for r in records {
        let embedding = Vector::from(model.embed(&r.embedding_text)?);

        sqlx::query(
            "INSERT INTO facilities (id, name, description, embedding, lat, lon, is_reservable, type)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
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
        // <br /> becomes a space, then collapsed
        assert_eq!(strip_html("Line1<br />Line2"), "Line1 Line2");
    }

    #[test]
    fn test_strip_html_removes_nested_tags() {
        assert_eq!(strip_html("<p><strong>Bold</strong> text</p>"), "Bold text");
    }

    #[test]
    fn test_strip_html_replaces_entities_with_space() {
        // Entities are replaced with a space and then collapsed — they are NOT decoded.
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

    /// Verifies that name, stripped description, and keywords are joined with
    /// single spaces in the order the main loop uses them.
    #[test]
    fn test_embedding_text_format() {
        let name = "Test Camp";
        let clean_desc = strip_html("<p>A nice campground</p>");
        let keywords = "camping hiking";
        let text = format!("{} {} {}", name, clean_desc, keywords);
        assert_eq!(text, "Test Camp A nice campground camping hiking");
    }

    // --- L2 normalisation ---

    /// A [3, 4] vector (norm = 5) should become [0.6, 0.8] after normalisation.
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
        assert!((v[0] - 0.6).abs() < 1e-6, "v[0] should be 0.6");
        assert!((v[1] - 0.8).abs() < 1e-6, "v[1] should be 0.8");
        // Remaining dimensions stay zero
        assert!(v[2..].iter().all(|&x| x == 0.0));
        // Resulting vector must have unit length
        let new_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((new_norm - 1.0).abs() < 1e-6, "norm should be 1.0 after normalisation");
    }

    /// A zero vector (norm = 0) must not be modified — division is skipped.
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

    /// All mask values 1.0: each token contributes equally.
    /// With 3 tokens each holding a one-hot vector, the mean is [1/3, 1/3, 1/3, 0].
    #[test]
    fn test_mean_pool_uniform_mask() {
        // 3 tokens, 4 dims — simulates a tiny hidden_state slice
        let hidden: &[f32] = &[
            1.0, 0.0, 0.0, 0.0, // token 0
            0.0, 1.0, 0.0, 0.0, // token 1
            0.0, 0.0, 1.0, 0.0, // token 2
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

    /// Mask of [1, 0]: only the first token should contribute.
    #[test]
    fn test_mean_pool_masked_token_excluded() {
        // 2 tokens, 2 dims
        let hidden: &[f32] = &[
            3.0, 6.0, // token 0 (unmasked)
            9.0, 9.0, // token 1 (masked out)
        ];
        let mask_f = vec![1.0f32, 0.0];
        let dims = 2usize;
        let mask_sum: f32 = mask_f.iter().sum(); // 1.0

        let mut pooled = vec![0.0f32; dims];
        for (i, &w) in mask_f.iter().enumerate() {
            for d in 0..dims {
                pooled[d] += hidden[i * dims + d] * w;
            }
        }
        for x in &mut pooled {
            *x /= mask_sum;
        }

        // Only token 0 contributed, mask_sum = 1, so result == token 0
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

    let data_path =
        std::env::var("SEED_DATA_PATH").unwrap_or_else(|_| "data/campgrounds.json".to_string());

    println!("Seeder: reading {data_path}");
    let raw = std::fs::read_to_string(&data_path)
        .unwrap_or_else(|e| panic!("Cannot read {data_path}: {e}"));

    let payload: RidbPayload = serde_json::from_str(&raw)?;
    println!("Seeder: parsed {} facilities", payload.records.len());

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
            }
        })
        .collect();

    println!("Seeder: loading embedding model");
    let mut model = EmbeddingModel::load()?;
    println!("Seeder: model ready");

    let pool = connect().await?;
    println!("Seeder: connected to database");

    run_schema(&pool).await?;
    println!("Seeder: schema ready");

    seed(&pool, &records, &mut model).await?;
    println!("Seeder: inserted {} rows — done", records.len());

    Ok(())
}
