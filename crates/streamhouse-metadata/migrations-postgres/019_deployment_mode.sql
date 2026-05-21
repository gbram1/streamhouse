-- Add deployment_mode column to organizations
ALTER TABLE organizations ADD COLUMN deployment_mode VARCHAR(20) NOT NULL DEFAULT 'self_hosted';
ALTER TABLE organizations ADD CONSTRAINT valid_deployment_mode
    CHECK (deployment_mode IN ('self_hosted', 'byoc', 'managed'));
