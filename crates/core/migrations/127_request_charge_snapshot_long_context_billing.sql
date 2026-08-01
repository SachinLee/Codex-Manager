ALTER TABLE request_charge_snapshots
  ADD COLUMN long_context_billing_enabled INTEGER NOT NULL DEFAULT 1
  CHECK (long_context_billing_enabled IN (0, 1));
