CREATE TABLE IF NOT EXISTS product_devolutions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id  UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity    INTEGER NOT NULL CHECK (quantity > 0),
    reason      TEXT NOT NULL,
    returned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_devolutions_product_id ON product_devolutions(product_id);
CREATE INDEX IF NOT EXISTS idx_devolutions_returned_at ON product_devolutions(returned_at);
