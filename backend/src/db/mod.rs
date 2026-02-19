use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::*;

// ── Products ──────────────────────────────────────────────────────────────────

pub async fn fetch_all_products(pool: &PgPool, filters: &ProductFilters) -> AppResult<Vec<Product>> {
    let limit = filters.limit.unwrap_or(1000).min(10_000);
    let offset = filters.offset.unwrap_or(0);

    let products = sqlx::query_as::<_, Product>(
        r#"
        SELECT id, name, description, price_cents, quantity, category, created_at, updated_at
        FROM products
        WHERE ($1::text IS NULL OR category = $1)
          AND ($2::bigint IS NULL OR price_cents >= $2)
          AND ($3::bigint IS NULL OR price_cents <= $3)
        ORDER BY created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(filters.category.as_deref())
    .bind(filters.min_price_cents)
    .bind(filters.max_price_cents)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(products)
}

pub async fn fetch_product_by_id(pool: &PgPool, id: Uuid) -> AppResult<Product> {
    sqlx::query_as::<_, Product>(
        "SELECT id, name, description, price_cents, quantity, category, created_at, updated_at
         FROM products WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Product {} not found", id)))
}

pub async fn insert_product(pool: &PgPool, payload: &CreateProduct) -> AppResult<Product> {
    let product = sqlx::query_as::<_, Product>(
        r#"
        INSERT INTO products (name, description, price_cents, quantity, category)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, description, price_cents, quantity, category, created_at, updated_at
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.price_cents)
    .bind(payload.quantity)
    .bind(&payload.category)
    .fetch_one(pool)
    .await?;

    Ok(product)
}

pub async fn update_product(pool: &PgPool, id: Uuid, payload: &UpdateProduct) -> AppResult<Product> {
    // Fetch existing to merge optional fields
    let existing = fetch_product_by_id(pool, id).await?;

    let product = sqlx::query_as::<_, Product>(
        r#"
        UPDATE products
        SET name        = $1,
            description = $2,
            price_cents = $3,
            quantity    = $4,
            category    = $5,
            updated_at  = $6
        WHERE id = $7
        RETURNING id, name, description, price_cents, quantity, category, created_at, updated_at
        "#,
    )
    .bind(payload.name.as_deref().unwrap_or(&existing.name))
    .bind(payload.description.as_deref().or(existing.description.as_deref()))
    .bind(payload.price_cents.unwrap_or(existing.price_cents))
    .bind(payload.quantity.unwrap_or(existing.quantity))
    .bind(payload.category.as_deref().unwrap_or(&existing.category))
    .bind(Utc::now())
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Product {} not found", id)))?;

    Ok(product)
}

pub async fn delete_product(pool: &PgPool, id: Uuid) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Product {} not found", id)));
    }
    Ok(())
}

pub async fn count_products(pool: &PgPool) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM products")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

// ── Devolutions ───────────────────────────────────────────────────────────────

pub async fn fetch_all_devolutions(pool: &PgPool) -> AppResult<Vec<DevolutionWithProduct>> {
    let devolutions = sqlx::query_as::<_, DevolutionWithProduct>(
        r#"
        SELECT d.id, d.product_id, p.name AS product_name, p.category AS product_category,
               d.quantity, d.reason, d.returned_at, d.created_at
        FROM product_devolutions d
        JOIN products p ON p.id = d.product_id
        ORDER BY d.returned_at DESC
        LIMIT 1000
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(devolutions)
}

pub async fn fetch_devolution_by_id(pool: &PgPool, id: Uuid) -> AppResult<DevolutionWithProduct> {
    sqlx::query_as::<_, DevolutionWithProduct>(
        r#"
        SELECT d.id, d.product_id, p.name AS product_name, p.category AS product_category,
               d.quantity, d.reason, d.returned_at, d.created_at
        FROM product_devolutions d
        JOIN products p ON p.id = d.product_id
        WHERE d.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Devolution {} not found", id)))
}

pub async fn insert_devolution(pool: &PgPool, payload: &CreateDevolution) -> AppResult<DevolutionWithProduct> {
    if payload.quantity <= 0 {
        return Err(AppError::BadRequest("quantity must be > 0".to_string()));
    }

    // Verify product exists
    fetch_product_by_id(pool, payload.product_id).await?;

    let returned_at = payload.returned_at.unwrap_or_else(Utc::now);

    let dev = sqlx::query_as::<_, ProductDevolution>(
        r#"
        INSERT INTO product_devolutions (product_id, quantity, reason, returned_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id, product_id, quantity, reason, returned_at, created_at
        "#,
    )
    .bind(payload.product_id)
    .bind(payload.quantity)
    .bind(&payload.reason)
    .bind(returned_at)
    .fetch_one(pool)
    .await?;

    fetch_devolution_by_id(pool, dev.id).await
}

/// Fetch all products without filters (used for seeding sets in benchmarks).
pub async fn fetch_all_products_unbounded(pool: &PgPool) -> AppResult<Vec<Product>> {
    let products = sqlx::query_as::<_, Product>(
        "SELECT id, name, description, price_cents, quantity, category, created_at, updated_at
         FROM products ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(products)
}
