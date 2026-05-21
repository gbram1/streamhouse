-- Add deployment_mode column to organizations
ALTER TABLE organizations ADD COLUMN deployment_mode TEXT NOT NULL DEFAULT 'self_hosted';
