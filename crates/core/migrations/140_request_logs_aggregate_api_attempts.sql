-- Migration: Add aggregate_api_attempts column to request_logs
-- Date: 2026-09-03
-- Purpose: Store JSON array of aggregate API attempt records for cache affinity analysis

ALTER TABLE request_logs ADD COLUMN aggregate_api_attempts TEXT;
