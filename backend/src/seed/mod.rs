use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use sqlx::PgPool;
use tracing::info;

use crate::error::AppResult;
use crate::models::Product;

static CATEGORIES: &[&str] = &[
    "Electronics",
    "Clothing",
    "Food & Beverage",
    "Home & Garden",
    "Toys & Games",
    "Sports & Outdoors",
    "Books",
    "Automotive",
    "Health & Beauty",
    "Office Supplies",
    "Musical Instruments",
    "Pet Supplies",
    "Jewelry",
    "Tools & Hardware",
    "Baby Products",
];

static ADJECTIVES: &[&str] = &[
    "Premium", "Deluxe", "Ultra", "Pro", "Classic", "Elite", "Smart", "Eco",
    "Compact", "Portable", "Heavy-Duty", "Lightweight", "Advanced", "Basic",
    "Professional", "Essential", "Signature", "Exclusive", "Standard", "Plus",
    "Mini", "Maxi", "Turbo", "Super", "Mega", "Nano", "Micro", "Flex",
    "Rapid", "Silent",
];

static NOUNS: &[&str] = &[
    "Widget", "Gadget", "Device", "Module", "Unit", "Component", "System",
    "Kit", "Set", "Pack", "Bundle", "Assembly", "Console", "Panel", "Sensor",
    "Controller", "Adapter", "Monitor", "Processor", "Scanner", "Encoder",
    "Decoder", "Emitter", "Receiver", "Transmitter", "Amplifier", "Filter",
    "Converter", "Regulator", "Indicator",
];

#[allow(dead_code)]
static REASONS: &[&str] = &[
    "Defective on arrival",
    "Wrong item received",
    "Changed mind",
    "Product not as described",
    "Damaged during shipping",
    "Missing parts",
    "Quality below expectations",
    "Found better price elsewhere",
    "No longer needed",
    "Gift recipient already has it",
];

/// Generate a random product name using adjective + noun + serial suffix.
fn random_product_name(rng: &mut impl Rng, serial: usize) -> String {
    let adj = ADJECTIVES.choose(rng).unwrap_or(&"Standard");
    let noun = NOUNS.choose(rng).unwrap_or(&"Widget");
    format!("{} {} #{:05}", adj, noun, serial)
}

/// Seed the database with `count` random products in batches.
pub async fn seed_products(pool: &PgPool, count: usize) -> AppResult<Vec<Product>> {
    info!("Seeding {} products...", count);

    // StdRng is Send + Sync — safe to hold across async await points
    let mut rng = StdRng::from_entropy();
    let batch_size = 500_usize;
    let mut all_products: Vec<Product> = Vec::with_capacity(count);

    let chunks = (count + batch_size - 1) / batch_size;

    for chunk in 0..chunks {
        let start = chunk * batch_size;
        let end = (start + batch_size).min(count);
        let this_batch = end - start;

        // Build batch insert
        let mut names: Vec<String> = Vec::with_capacity(this_batch);
        let mut descriptions: Vec<Option<String>> = Vec::with_capacity(this_batch);
        let mut prices: Vec<i64> = Vec::with_capacity(this_batch);
        let mut quantities: Vec<i32> = Vec::with_capacity(this_batch);
        let mut categories: Vec<String> = Vec::with_capacity(this_batch);

        for i in 0..this_batch {
            names.push(random_product_name(&mut rng, start + i));
            descriptions.push(if rng.gen_bool(0.7) {
                Some(format!(
                    "High-quality {} for professional use. Serial: {}",
                    names.last().unwrap(),
                    start + i
                ))
            } else {
                None
            });
            prices.push(rng.gen_range(99..=999_99)); // $0.99 – $999.99
            quantities.push(rng.gen_range(0..=500));
            categories.push(CATEGORIES.choose(&mut rng).unwrap().to_string());
        }

        // Use unnest for bulk insert (much faster than individual INSERTs)
        let products = sqlx::query_as::<_, Product>(
            r#"
            INSERT INTO products (name, description, price_cents, quantity, category)
            SELECT * FROM UNNEST($1::text[], $2::text[], $3::bigint[], $4::int[], $5::text[])
            ON CONFLICT DO NOTHING
            RETURNING id, name, description, price_cents, quantity, category, created_at, updated_at
            "#,
        )
        .bind(&names)
        .bind(&descriptions)
        .bind(&prices)
        .bind(&quantities)
        .bind(&categories)
        .fetch_all(pool)
        .await?;

        all_products.extend(products);

        info!("  Seeded batch {}/{} ({} products so far)", chunk + 1, chunks, all_products.len());
    }

    info!("Seeding complete. Total: {} products", all_products.len());
    Ok(all_products)
}

/// Generate a random devolution reason.
#[allow(dead_code)]
pub fn random_reason(rng: &mut StdRng) -> String {
    REASONS.choose(rng).unwrap_or(&"Other").to_string()
}
