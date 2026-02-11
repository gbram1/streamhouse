//! Role-Based Access Control (RBAC) & Governance for StreamHouse
//!
//! Provides role-based access control built on top of the existing ACL system.
//! Includes role hierarchy, built-in roles, and data masking policies for governance.
//!
//! ## Architecture
//!
//! RBAC builds on top of the ACL primitives (`AclResource`, `AclAction`) to provide:
//! 1. **Roles**: Named collections of permissions (resource + action pairs)
//! 2. **Role Hierarchy**: Roles can inherit permissions from parent roles
//! 3. **Assignments**: Principals are assigned roles, gaining all associated permissions
//! 4. **Data Masking**: Policies for redacting or masking sensitive data based on roles
//! 5. **Middleware**: Tower layer for enforcing RBAC on HTTP routes
//!
//! ## Built-in Roles
//!
//! - `admin`: Full access to all resources and actions
//! - `operator`: Read, write, describe, and alter permissions on all resources
//! - `developer`: Read, write, and describe permissions on all resources
//! - `viewer`: Read and describe permissions on all resources
//!
//! ## Usage
//!
//! ```rust,ignore
//! use streamhouse_api::rbac::{RbacManager, Role, Permission};
//! use streamhouse_api::acl::{AclResource, AclAction};
//!
//! let manager = RbacManager::new();
//!
//! // Assign the built-in developer role to a user
//! manager.assign_role("user:alice", "developer", "admin:root").await?;
//!
//! // Check if the user is allowed to read from a topic
//! let allowed = manager.is_allowed(
//!     "user:alice",
//!     &AclResource::Topic("orders".to_string()),
//!     AclAction::Read,
//! ).await;
//! ```

use crate::acl::{AclAction, AclResource};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during RBAC operations
#[derive(Debug, Error)]
pub enum RbacError {
    #[error("Role not found: {0}")]
    RoleNotFound(String),

    #[error("Role already exists: {0}")]
    RoleAlreadyExists(String),

    #[error("Assignment not found: {0}")]
    AssignmentNotFound(String),

    #[error("Cannot delete built-in role: {0}")]
    BuiltInRole(String),

    #[error("Circular role hierarchy detected: {0}")]
    CircularHierarchy(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, RbacError>;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A permission grants one or more actions on a specific resource type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// The resource this permission applies to
    pub resource: AclResource,
    /// The actions allowed on this resource
    pub actions: Vec<AclAction>,
}

impl Permission {
    /// Create a new permission
    pub fn new(resource: AclResource, actions: Vec<AclAction>) -> Self {
        Self { resource, actions }
    }

    /// Check if this permission allows a given action on a resource
    pub fn allows(&self, resource: &AclResource, action: AclAction) -> bool {
        self.resource.matches(resource) && self.actions.iter().any(|a| a.matches(action))
    }
}

/// A role is a named collection of permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Unique role identifier
    pub id: String,
    /// Human-readable role name (e.g., "admin", "developer")
    pub name: String,
    /// Description of the role
    pub description: String,
    /// Permissions granted by this role
    pub permissions: Vec<Permission>,
    /// Optional parent role ID for hierarchy (inherits parent permissions)
    pub parent_role: Option<String>,
    /// When this role was created
    pub created_at: i64,
    /// Whether this is a built-in role (cannot be deleted)
    pub built_in: bool,
}

impl Role {
    /// Create a new custom role
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        permissions: Vec<Permission>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            permissions,
            parent_role: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            built_in: false,
        }
    }

    /// Create a new custom role with a parent
    pub fn with_parent(
        name: impl Into<String>,
        description: impl Into<String>,
        permissions: Vec<Permission>,
        parent_role_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            permissions,
            parent_role: Some(parent_role_id.into()),
            created_at: chrono::Utc::now().timestamp_millis(),
            built_in: false,
        }
    }
}

/// A role assignment links a principal to a role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// Unique assignment identifier
    pub id: String,
    /// The principal being assigned the role (e.g., "user:alice", "service:payment")
    pub principal: String,
    /// The role ID being assigned
    pub role_id: String,
    /// Who assigned this role
    pub assigned_by: String,
    /// When the role was assigned
    pub assigned_at: i64,
}

impl RoleAssignment {
    /// Create a new role assignment
    pub fn new(
        principal: impl Into<String>,
        role_id: impl Into<String>,
        assigned_by: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            principal: principal.into(),
            role_id: role_id.into(),
            assigned_by: assigned_by.into(),
            assigned_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in roles
// ---------------------------------------------------------------------------

/// Built-in role ID for the admin role
pub const ADMIN_ROLE_ID: &str = "builtin:admin";
/// Built-in role ID for the operator role
pub const OPERATOR_ROLE_ID: &str = "builtin:operator";
/// Built-in role ID for the developer role
pub const DEVELOPER_ROLE_ID: &str = "builtin:developer";
/// Built-in role ID for the viewer role
pub const VIEWER_ROLE_ID: &str = "builtin:viewer";

/// Create the built-in admin role (all permissions on all resources)
fn create_admin_role() -> Role {
    Role {
        id: ADMIN_ROLE_ID.to_string(),
        name: "admin".to_string(),
        description: "Full administrative access to all resources".to_string(),
        permissions: vec![Permission::new(AclResource::All, vec![AclAction::All])],
        parent_role: None,
        created_at: 0,
        built_in: true,
    }
}

/// Create the built-in operator role (read, write, describe, alter)
fn create_operator_role() -> Role {
    Role {
        id: OPERATOR_ROLE_ID.to_string(),
        name: "operator".to_string(),
        description: "Operational access: read, write, describe, and alter resources".to_string(),
        permissions: vec![Permission::new(
            AclResource::All,
            vec![
                AclAction::Read,
                AclAction::Write,
                AclAction::Describe,
                AclAction::Alter,
            ],
        )],
        parent_role: None,
        created_at: 0,
        built_in: true,
    }
}

/// Create the built-in developer role (read, write, describe)
fn create_developer_role() -> Role {
    Role {
        id: DEVELOPER_ROLE_ID.to_string(),
        name: "developer".to_string(),
        description: "Developer access: read, write, and describe resources".to_string(),
        permissions: vec![Permission::new(
            AclResource::All,
            vec![AclAction::Read, AclAction::Write, AclAction::Describe],
        )],
        parent_role: None,
        created_at: 0,
        built_in: true,
    }
}

/// Create the built-in viewer role (read, describe only)
fn create_viewer_role() -> Role {
    Role {
        id: VIEWER_ROLE_ID.to_string(),
        name: "viewer".to_string(),
        description: "Read-only access: read and describe resources".to_string(),
        permissions: vec![Permission::new(
            AclResource::All,
            vec![AclAction::Read, AclAction::Describe],
        )],
        parent_role: None,
        created_at: 0,
        built_in: true,
    }
}

// ---------------------------------------------------------------------------
// Data Masking
// ---------------------------------------------------------------------------

/// Type of data masking to apply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MaskType {
    /// Completely redact the data (replace with "***REDACTED***")
    Redact,
    /// Hash the data using SHA-256
    Hash,
    /// Partially mask the data (show first and last characters)
    Partial,
}

impl std::fmt::Display for MaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaskType::Redact => write!(f, "redact"),
            MaskType::Hash => write!(f, "hash"),
            MaskType::Partial => write!(f, "partial"),
        }
    }
}

/// A data masking policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMaskingPolicy {
    /// Unique policy identifier
    pub id: String,
    /// Policy name
    pub name: String,
    /// Field name patterns to mask (supports glob-like patterns)
    pub field_patterns: Vec<String>,
    /// Type of masking to apply
    pub mask_type: MaskType,
    /// Role IDs that this masking applies to (empty = applies to all non-admin roles)
    pub applies_to_roles: Vec<String>,
}

impl DataMaskingPolicy {
    /// Create a new data masking policy
    pub fn new(
        name: impl Into<String>,
        field_patterns: Vec<String>,
        mask_type: MaskType,
        applies_to_roles: Vec<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            field_patterns,
            mask_type,
            applies_to_roles,
        }
    }

    /// Check if a field name matches any of the policy patterns
    pub fn matches_field(&self, field_name: &str) -> bool {
        self.field_patterns
            .iter()
            .any(|pattern| field_pattern_matches(pattern, field_name))
    }
}

/// Check if a field pattern matches a field name (supports * and ** wildcards)
fn field_pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }
    pattern == value
}

/// Data masker applies masking policies to data values
pub struct DataMasker {
    policies: Arc<RwLock<Vec<DataMaskingPolicy>>>,
}

impl DataMasker {
    /// Create a new data masker
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a masking policy
    pub async fn add_policy(&self, policy: DataMaskingPolicy) {
        info!(
            policy_id = %policy.id,
            name = %policy.name,
            mask_type = %policy.mask_type,
            "Adding data masking policy"
        );
        let mut policies = self.policies.write().await;
        policies.push(policy);
    }

    /// Remove a masking policy by ID
    pub async fn remove_policy(&self, policy_id: &str) -> bool {
        let mut policies = self.policies.write().await;
        let len_before = policies.len();
        policies.retain(|p| p.id != policy_id);
        policies.len() < len_before
    }

    /// List all policies
    pub async fn list_policies(&self) -> Vec<DataMaskingPolicy> {
        self.policies.read().await.clone()
    }

    /// Check if a field should be masked for a given set of role IDs
    pub async fn should_mask(&self, field_name: &str, role_ids: &[String]) -> Option<MaskType> {
        let policies = self.policies.read().await;

        for policy in policies.iter() {
            // Check if the field matches
            if !policy.matches_field(field_name) {
                continue;
            }

            // Check if the policy applies to these roles
            if policy.applies_to_roles.is_empty() {
                // Empty means it applies to all non-admin roles
                if !role_ids.contains(&ADMIN_ROLE_ID.to_string()) {
                    return Some(policy.mask_type.clone());
                }
            } else if role_ids
                .iter()
                .any(|r| policy.applies_to_roles.contains(r))
            {
                return Some(policy.mask_type.clone());
            }
        }

        None
    }

    /// Apply masking to a string value
    pub fn apply_mask(value: &str, mask_type: &MaskType) -> String {
        match mask_type {
            MaskType::Redact => "***REDACTED***".to_string(),
            MaskType::Hash => {
                let mut hasher = Sha256::new();
                hasher.update(value.as_bytes());
                format!("{:x}", hasher.finalize())
            }
            MaskType::Partial => {
                if value.len() <= 4 {
                    "****".to_string()
                } else {
                    let first = &value[..2];
                    let last = &value[value.len() - 2..];
                    let mask_len = value.len() - 4;
                    format!("{}{}{}",first, "*".repeat(mask_len), last)
                }
            }
        }
    }
}

impl Default for DataMasker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// RBAC Manager
// ---------------------------------------------------------------------------

/// RBAC manager for role-based access control
///
/// Manages roles, role assignments, and permission checking. Uses RwLock
/// for thread-safe concurrent access.
pub struct RbacManager {
    /// Roles indexed by role ID
    roles: Arc<RwLock<HashMap<String, Role>>>,
    /// Assignments indexed by assignment ID
    assignments: Arc<RwLock<HashMap<String, RoleAssignment>>>,
    /// Data masker for governance
    masker: DataMasker,
}

impl RbacManager {
    /// Create a new RBAC manager with built-in roles initialized
    pub fn new() -> Self {
        let mut roles = HashMap::new();

        // Initialize built-in roles
        let admin = create_admin_role();
        let operator = create_operator_role();
        let developer = create_developer_role();
        let viewer = create_viewer_role();

        roles.insert(admin.id.clone(), admin);
        roles.insert(operator.id.clone(), operator);
        roles.insert(developer.id.clone(), developer);
        roles.insert(viewer.id.clone(), viewer);

        info!("RBAC manager initialized with {} built-in roles", roles.len());

        Self {
            roles: Arc::new(RwLock::new(roles)),
            assignments: Arc::new(RwLock::new(HashMap::new())),
            masker: DataMasker::new(),
        }
    }

    /// Get a reference to the data masker
    pub fn masker(&self) -> &DataMasker {
        &self.masker
    }

    // -----------------------------------------------------------------------
    // Role management
    // -----------------------------------------------------------------------

    /// Create a new custom role
    pub async fn create_role(&self, role: Role) -> Result<Role> {
        let mut roles = self.roles.write().await;

        // Check for duplicate name
        if roles.values().any(|r| r.name == role.name) {
            return Err(RbacError::RoleAlreadyExists(role.name));
        }

        // Validate parent role exists if specified
        if let Some(ref parent_id) = role.parent_role {
            if !roles.contains_key(parent_id) {
                return Err(RbacError::RoleNotFound(parent_id.clone()));
            }
        }

        info!(
            role_id = %role.id,
            name = %role.name,
            permissions_count = role.permissions.len(),
            "Creating custom role"
        );

        let created = role.clone();
        roles.insert(role.id.clone(), role);
        Ok(created)
    }

    /// Delete a custom role (built-in roles cannot be deleted)
    pub async fn delete_role(&self, role_id: &str) -> Result<Role> {
        let mut roles = self.roles.write().await;

        let role = roles
            .get(role_id)
            .ok_or_else(|| RbacError::RoleNotFound(role_id.to_string()))?;

        if role.built_in {
            return Err(RbacError::BuiltInRole(role.name.clone()));
        }

        // Check that no roles inherit from this role
        let has_children = roles.values().any(|r| {
            r.parent_role
                .as_ref()
                .map(|p| p == role_id)
                .unwrap_or(false)
        });

        if has_children {
            return Err(RbacError::Config(format!(
                "Cannot delete role '{}': other roles inherit from it",
                role_id
            )));
        }

        let removed = roles.remove(role_id).unwrap();

        // Also remove any assignments for this role
        let mut assignments = self.assignments.write().await;
        assignments.retain(|_, a| a.role_id != role_id);

        warn!(
            role_id = %removed.id,
            name = %removed.name,
            "Deleted custom role"
        );

        Ok(removed)
    }

    /// Get a role by ID
    pub async fn get_role(&self, role_id: &str) -> Option<Role> {
        self.roles.read().await.get(role_id).cloned()
    }

    /// Get a role by name
    pub async fn get_role_by_name(&self, name: &str) -> Option<Role> {
        self.roles
            .read()
            .await
            .values()
            .find(|r| r.name == name)
            .cloned()
    }

    /// List all roles
    pub async fn list_roles(&self) -> Vec<Role> {
        self.roles.read().await.values().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Role assignment
    // -----------------------------------------------------------------------

    /// Assign a role to a principal
    pub async fn assign_role(
        &self,
        principal: &str,
        role_id: &str,
        assigned_by: &str,
    ) -> Result<RoleAssignment> {
        // Verify role exists
        let roles = self.roles.read().await;
        if !roles.contains_key(role_id) {
            return Err(RbacError::RoleNotFound(role_id.to_string()));
        }
        drop(roles);

        let assignment = RoleAssignment::new(principal, role_id, assigned_by);

        info!(
            assignment_id = %assignment.id,
            principal = principal,
            role_id = role_id,
            assigned_by = assigned_by,
            "Assigning role to principal"
        );

        let mut assignments = self.assignments.write().await;
        let created = assignment.clone();
        assignments.insert(assignment.id.clone(), assignment);
        Ok(created)
    }

    /// Revoke a role assignment by assignment ID
    pub async fn revoke_role(&self, assignment_id: &str) -> Result<RoleAssignment> {
        let mut assignments = self.assignments.write().await;
        let assignment = assignments
            .remove(assignment_id)
            .ok_or_else(|| RbacError::AssignmentNotFound(assignment_id.to_string()))?;

        warn!(
            assignment_id = %assignment.id,
            principal = %assignment.principal,
            role_id = %assignment.role_id,
            "Revoked role assignment"
        );

        Ok(assignment)
    }

    /// Revoke all role assignments for a principal and role combination
    pub async fn revoke_role_from_principal(
        &self,
        principal: &str,
        role_id: &str,
    ) -> Vec<RoleAssignment> {
        let mut assignments = self.assignments.write().await;
        let mut revoked = Vec::new();

        let to_remove: Vec<String> = assignments
            .iter()
            .filter(|(_, a)| a.principal == principal && a.role_id == role_id)
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            if let Some(assignment) = assignments.remove(&id) {
                revoked.push(assignment);
            }
        }

        if !revoked.is_empty() {
            warn!(
                principal = principal,
                role_id = role_id,
                count = revoked.len(),
                "Revoked role assignments from principal"
            );
        }

        revoked
    }

    /// Get all role assignments for a principal
    pub async fn get_roles_for_principal(&self, principal: &str) -> Vec<RoleAssignment> {
        self.assignments
            .read()
            .await
            .values()
            .filter(|a| a.principal == principal)
            .cloned()
            .collect()
    }

    /// Get all assignments for a specific role
    pub async fn get_assignments_for_role(&self, role_id: &str) -> Vec<RoleAssignment> {
        self.assignments
            .read()
            .await
            .values()
            .filter(|a| a.role_id == role_id)
            .cloned()
            .collect()
    }

    /// List all assignments
    pub async fn list_assignments(&self) -> Vec<RoleAssignment> {
        self.assignments.read().await.values().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Permission checking
    // -----------------------------------------------------------------------

    /// Check if a principal is allowed to perform an action on a resource
    ///
    /// Resolves all assigned roles (including parent hierarchy) and checks if
    /// any of them grant the requested permission.
    pub async fn is_allowed(
        &self,
        principal: &str,
        resource: &AclResource,
        action: AclAction,
    ) -> bool {
        let assignments = self.assignments.read().await;
        let roles = self.roles.read().await;

        // Find all role IDs assigned to this principal
        let assigned_role_ids: Vec<String> = assignments
            .values()
            .filter(|a| a.principal == principal)
            .map(|a| a.role_id.clone())
            .collect();

        if assigned_role_ids.is_empty() {
            debug!(
                principal = principal,
                resource = %resource,
                action = %action,
                "No roles assigned, denying access"
            );
            return false;
        }

        // Resolve all effective role IDs (including parents in the hierarchy)
        let effective_role_ids = resolve_role_hierarchy(&assigned_role_ids, &roles);

        // Check each effective role for the permission
        for role_id in &effective_role_ids {
            if let Some(role) = roles.get(role_id) {
                for perm in &role.permissions {
                    if perm.allows(resource, action) {
                        debug!(
                            principal = principal,
                            role = %role.name,
                            resource = %resource,
                            action = %action,
                            "Access granted by role"
                        );
                        return true;
                    }
                }
            }
        }

        debug!(
            principal = principal,
            resource = %resource,
            action = %action,
            roles = ?assigned_role_ids,
            "Access denied - no matching permission in any role"
        );
        false
    }

    /// Get all effective permissions for a principal (resolving hierarchy)
    pub async fn get_effective_permissions(&self, principal: &str) -> Vec<Permission> {
        let assignments = self.assignments.read().await;
        let roles = self.roles.read().await;

        let assigned_role_ids: Vec<String> = assignments
            .values()
            .filter(|a| a.principal == principal)
            .map(|a| a.role_id.clone())
            .collect();

        let effective_role_ids = resolve_role_hierarchy(&assigned_role_ids, &roles);

        let mut permissions = Vec::new();
        for role_id in &effective_role_ids {
            if let Some(role) = roles.get(role_id) {
                permissions.extend(role.permissions.clone());
            }
        }

        permissions
    }

    /// Get all role IDs (including inherited) for a principal
    pub async fn get_effective_role_ids(&self, principal: &str) -> Vec<String> {
        let assignments = self.assignments.read().await;
        let roles = self.roles.read().await;

        let assigned_role_ids: Vec<String> = assignments
            .values()
            .filter(|a| a.principal == principal)
            .map(|a| a.role_id.clone())
            .collect();

        resolve_role_hierarchy(&assigned_role_ids, &roles)
    }
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve role hierarchy by collecting all role IDs including parents
fn resolve_role_hierarchy(role_ids: &[String], roles: &HashMap<String, Role>) -> Vec<String> {
    let mut effective = Vec::new();
    let mut visited = std::collections::HashSet::new();

    for role_id in role_ids {
        collect_hierarchy(role_id, roles, &mut effective, &mut visited);
    }

    effective
}

/// Recursively collect role IDs through the parent hierarchy
fn collect_hierarchy(
    role_id: &str,
    roles: &HashMap<String, Role>,
    effective: &mut Vec<String>,
    visited: &mut std::collections::HashSet<String>,
) {
    if visited.contains(role_id) {
        return; // Avoid infinite loops in circular hierarchies
    }
    visited.insert(role_id.to_string());
    effective.push(role_id.to_string());

    if let Some(role) = roles.get(role_id) {
        if let Some(ref parent_id) = role.parent_role {
            collect_hierarchy(parent_id, roles, effective, visited);
        }
    }
}

// ---------------------------------------------------------------------------
// Tower middleware (RbacLayer)
// ---------------------------------------------------------------------------

/// RBAC authorization layer for Axum routes
///
/// Similar to `AclLayer`, this middleware checks RBAC permissions before
/// allowing a request to proceed. It extracts the principal from the
/// `AuthenticatedKey` request extension and checks against the RBAC manager.
#[derive(Clone)]
pub struct RbacLayer {
    manager: Arc<RbacManager>,
    resource_extractor: RbacResourceExtractor,
}

/// Function type for extracting resources from requests
type RbacResourceExtractor =
    Arc<dyn Fn(&str, &str) -> Option<(AclResource, AclAction)> + Send + Sync>;

impl RbacLayer {
    /// Create a new RBAC layer
    pub fn new(manager: Arc<RbacManager>) -> Self {
        Self {
            manager,
            resource_extractor: Arc::new(default_rbac_resource_extractor),
        }
    }

    /// Create with a custom resource extractor
    pub fn with_extractor<F>(manager: Arc<RbacManager>, extractor: F) -> Self
    where
        F: Fn(&str, &str) -> Option<(AclResource, AclAction)> + Send + Sync + 'static,
    {
        Self {
            manager,
            resource_extractor: Arc::new(extractor),
        }
    }
}

/// Default resource extractor for RBAC (same logic as ACL)
fn default_rbac_resource_extractor(
    method: &str,
    path: &str,
) -> Option<(AclResource, AclAction)> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match parts.as_slice() {
        ["api", "v1", "topics"] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "POST" => AclAction::Create,
                _ => return None,
            };
            Some((AclResource::All, action))
        }
        ["api", "v1", "topics", name] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "DELETE" => AclAction::Delete,
                _ => return None,
            };
            Some((AclResource::Topic(name.to_string()), action))
        }
        ["api", "v1", "topics", name, "messages"] => {
            Some((AclResource::Topic(name.to_string()), AclAction::Read))
        }
        ["api", "v1", "produce"] | ["api", "v1", "produce", "batch"] => {
            Some((AclResource::All, AclAction::Write))
        }
        ["api", "v1", "consume"] => Some((AclResource::All, AclAction::Read)),
        ["api", "v1", "consumer-groups"] => Some((AclResource::All, AclAction::Describe)),
        ["api", "v1", "consumer-groups", group_id] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "DELETE" => AclAction::Delete,
                _ => return None,
            };
            Some((AclResource::ConsumerGroup(group_id.to_string()), action))
        }
        ["api", "v1", "organizations", ..] => Some((AclResource::Cluster, AclAction::Admin)),
        ["api", "v1", "api-keys", ..] => Some((AclResource::Cluster, AclAction::Admin)),
        ["health"] | ["live"] | ["ready"] | ["metrics"] => None,
        _ => None,
    }
}

impl<S> tower::Layer<S> for RbacLayer {
    type Service = RbacMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RbacMiddleware {
            inner,
            manager: self.manager.clone(),
            resource_extractor: self.resource_extractor.clone(),
        }
    }
}

/// RBAC middleware service
#[derive(Clone)]
pub struct RbacMiddleware<S> {
    inner: S,
    manager: Arc<RbacManager>,
    resource_extractor: RbacResourceExtractor,
}

impl<S> tower::Service<axum::extract::Request> for RbacMiddleware<S>
where
    S: tower::Service<axum::extract::Request, Response = axum::response::Response>
        + Send
        + Clone
        + 'static,
    S::Future: Send + 'static,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = futures::future::BoxFuture<'static, std::result::Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: axum::extract::Request) -> Self::Future {
        let manager = self.manager.clone();
        let extractor = self.resource_extractor.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let method = request.method().as_str().to_string();
            let path = request.uri().path().to_string();

            // Extract resource and action from request
            let Some((resource, action)) = extractor(&method, &path) else {
                // No RBAC needed for this path
                return inner.call(request).await;
            };

            // Get principal from authenticated key extension
            let principal = request
                .extensions()
                .get::<crate::auth::AuthenticatedKey>()
                .map(|key| format!("apikey:{}", key.key_id))
                .unwrap_or_else(|| "anonymous".to_string());

            // Check RBAC permission
            let allowed = manager.is_allowed(&principal, &resource, action).await;

            if !allowed {
                warn!(
                    principal = %principal,
                    resource = %resource,
                    action = %action,
                    "RBAC denied access"
                );

                let body = serde_json::json!({
                    "error": format!("Access denied: {} on {} for {}", action, resource, principal),
                    "code": 403
                });

                return Ok(axum::response::Response::builder()
                    .status(axum::http::StatusCode::FORBIDDEN)
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap());
            }

            inner.call(request).await
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_built_in_roles_initialized() {
        let manager = RbacManager::new();
        let roles = manager.list_roles().await;

        assert_eq!(roles.len(), 4);

        // Verify each built-in role exists
        assert!(manager.get_role(ADMIN_ROLE_ID).await.is_some());
        assert!(manager.get_role(OPERATOR_ROLE_ID).await.is_some());
        assert!(manager.get_role(DEVELOPER_ROLE_ID).await.is_some());
        assert!(manager.get_role(VIEWER_ROLE_ID).await.is_some());
    }

    #[tokio::test]
    async fn test_admin_role_has_all_permissions() {
        let manager = RbacManager::new();
        let admin = manager.get_role(ADMIN_ROLE_ID).await.unwrap();

        assert!(admin.built_in);
        assert_eq!(admin.name, "admin");
        assert_eq!(admin.permissions.len(), 1);

        let perm = &admin.permissions[0];
        assert!(perm.allows(&AclResource::Topic("any".to_string()), AclAction::Read));
        assert!(perm.allows(&AclResource::Topic("any".to_string()), AclAction::Write));
        assert!(perm.allows(&AclResource::Topic("any".to_string()), AclAction::Admin));
        assert!(perm.allows(&AclResource::Cluster, AclAction::Admin));
    }

    #[tokio::test]
    async fn test_viewer_role_limited_permissions() {
        let manager = RbacManager::new();
        let viewer = manager.get_role(VIEWER_ROLE_ID).await.unwrap();

        let perm = &viewer.permissions[0];
        assert!(perm.allows(&AclResource::Topic("any".to_string()), AclAction::Read));
        assert!(perm.allows(&AclResource::Topic("any".to_string()), AclAction::Describe));
        assert!(!perm.allows(&AclResource::Topic("any".to_string()), AclAction::Write));
        assert!(!perm.allows(&AclResource::Topic("any".to_string()), AclAction::Admin));
        assert!(!perm.allows(&AclResource::Topic("any".to_string()), AclAction::Delete));
    }

    #[tokio::test]
    async fn test_create_custom_role() {
        let manager = RbacManager::new();

        let role = Role::new(
            "custom-read-orders",
            "Read-only access to orders topics",
            vec![Permission::new(
                AclResource::Topic("orders-*".to_string()),
                vec![AclAction::Read, AclAction::Describe],
            )],
        );

        let created = manager.create_role(role).await.unwrap();
        assert_eq!(created.name, "custom-read-orders");
        assert!(!created.built_in);

        // Verify it can be retrieved
        let fetched = manager.get_role(&created.id).await.unwrap();
        assert_eq!(fetched.name, "custom-read-orders");
    }

    #[tokio::test]
    async fn test_create_duplicate_role_name_fails() {
        let manager = RbacManager::new();

        let role = Role::new("admin", "Duplicate admin", vec![]);
        let result = manager.create_role(role).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_custom_role() {
        let manager = RbacManager::new();

        let role = Role::new("temp-role", "Temporary role", vec![]);
        let created = manager.create_role(role).await.unwrap();

        let deleted = manager.delete_role(&created.id).await.unwrap();
        assert_eq!(deleted.name, "temp-role");

        // Verify it is gone
        assert!(manager.get_role(&created.id).await.is_none());
    }

    #[tokio::test]
    async fn test_cannot_delete_built_in_role() {
        let manager = RbacManager::new();

        let result = manager.delete_role(ADMIN_ROLE_ID).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RbacError::BuiltInRole(_)));
    }

    #[tokio::test]
    async fn test_assign_and_check_role() {
        let manager = RbacManager::new();

        // Assign developer role to alice
        manager
            .assign_role("user:alice", DEVELOPER_ROLE_ID, "admin:root")
            .await
            .unwrap();

        // Developer can read
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );

        // Developer can write
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Write,
                )
                .await
        );

        // Developer can describe
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Describe,
                )
                .await
        );

        // Developer cannot admin
        assert!(
            !manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Admin,
                )
                .await
        );

        // Developer cannot alter
        assert!(
            !manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Alter,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_viewer_permissions() {
        let manager = RbacManager::new();

        manager
            .assign_role("user:bob", VIEWER_ROLE_ID, "admin:root")
            .await
            .unwrap();

        // Viewer can read
        assert!(
            manager
                .is_allowed(
                    "user:bob",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );

        // Viewer cannot write
        assert!(
            !manager
                .is_allowed(
                    "user:bob",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Write,
                )
                .await
        );

        // Viewer cannot delete
        assert!(
            !manager
                .is_allowed(
                    "user:bob",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Delete,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_admin_has_all_access() {
        let manager = RbacManager::new();

        manager
            .assign_role("user:admin", ADMIN_ROLE_ID, "admin:root")
            .await
            .unwrap();

        // Admin can do everything
        assert!(
            manager
                .is_allowed(
                    "user:admin",
                    &AclResource::Topic("anything".to_string()),
                    AclAction::Read,
                )
                .await
        );
        assert!(
            manager
                .is_allowed(
                    "user:admin",
                    &AclResource::Topic("anything".to_string()),
                    AclAction::Write,
                )
                .await
        );
        assert!(
            manager
                .is_allowed(
                    "user:admin",
                    &AclResource::Topic("anything".to_string()),
                    AclAction::Admin,
                )
                .await
        );
        assert!(
            manager
                .is_allowed("user:admin", &AclResource::Cluster, AclAction::Admin)
                .await
        );
    }

    #[tokio::test]
    async fn test_no_role_means_denied() {
        let manager = RbacManager::new();

        // User with no roles should be denied
        assert!(
            !manager
                .is_allowed(
                    "user:nobody",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_revoke_role() {
        let manager = RbacManager::new();

        let assignment = manager
            .assign_role("user:alice", DEVELOPER_ROLE_ID, "admin:root")
            .await
            .unwrap();

        // Alice should have access
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );

        // Revoke the assignment
        manager.revoke_role(&assignment.id).await.unwrap();

        // Alice should no longer have access
        assert!(
            !manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_revoke_role_from_principal() {
        let manager = RbacManager::new();

        // Assign multiple roles
        manager
            .assign_role("user:alice", DEVELOPER_ROLE_ID, "admin:root")
            .await
            .unwrap();
        manager
            .assign_role("user:alice", VIEWER_ROLE_ID, "admin:root")
            .await
            .unwrap();

        // Revoke developer role specifically
        let revoked = manager
            .revoke_role_from_principal("user:alice", DEVELOPER_ROLE_ID)
            .await;
        assert_eq!(revoked.len(), 1);

        // Alice should still have viewer access
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );

        // But not write access (viewer does not have write)
        assert!(
            !manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Write,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_get_roles_for_principal() {
        let manager = RbacManager::new();

        manager
            .assign_role("user:alice", DEVELOPER_ROLE_ID, "admin:root")
            .await
            .unwrap();
        manager
            .assign_role("user:alice", VIEWER_ROLE_ID, "admin:root")
            .await
            .unwrap();
        manager
            .assign_role("user:bob", ADMIN_ROLE_ID, "admin:root")
            .await
            .unwrap();

        let alice_roles = manager.get_roles_for_principal("user:alice").await;
        assert_eq!(alice_roles.len(), 2);

        let bob_roles = manager.get_roles_for_principal("user:bob").await;
        assert_eq!(bob_roles.len(), 1);

        let nobody_roles = manager.get_roles_for_principal("user:nobody").await;
        assert!(nobody_roles.is_empty());
    }

    #[tokio::test]
    async fn test_role_hierarchy() {
        let manager = RbacManager::new();

        // Create a child role that inherits from developer
        let child = Role::with_parent(
            "senior-dev",
            "Senior developer with alter permission",
            vec![Permission::new(
                AclResource::All,
                vec![AclAction::Alter],
            )],
            DEVELOPER_ROLE_ID,
        );
        let child = manager.create_role(child).await.unwrap();

        // Assign the child role
        manager
            .assign_role("user:senior", &child.id, "admin:root")
            .await
            .unwrap();

        // Should have own permissions (alter)
        assert!(
            manager
                .is_allowed(
                    "user:senior",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Alter,
                )
                .await
        );

        // Should inherit parent permissions (read, write, describe from developer)
        assert!(
            manager
                .is_allowed(
                    "user:senior",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Read,
                )
                .await
        );
        assert!(
            manager
                .is_allowed(
                    "user:senior",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Write,
                )
                .await
        );

        // Should NOT have admin (neither child nor parent grants admin)
        assert!(
            !manager
                .is_allowed(
                    "user:senior",
                    &AclResource::Topic("orders".to_string()),
                    AclAction::Admin,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_get_effective_permissions() {
        let manager = RbacManager::new();

        manager
            .assign_role("user:alice", VIEWER_ROLE_ID, "admin:root")
            .await
            .unwrap();

        let perms = manager.get_effective_permissions("user:alice").await;
        assert_eq!(perms.len(), 1); // viewer has 1 permission entry
    }

    #[tokio::test]
    async fn test_get_effective_role_ids() {
        let manager = RbacManager::new();

        // Create hierarchy: child -> developer
        let child = Role::with_parent(
            "custom-child",
            "Child role",
            vec![],
            DEVELOPER_ROLE_ID,
        );
        let child = manager.create_role(child).await.unwrap();

        manager
            .assign_role("user:alice", &child.id, "admin:root")
            .await
            .unwrap();

        let effective = manager.get_effective_role_ids("user:alice").await;
        assert_eq!(effective.len(), 2); // child + developer (parent)
        assert!(effective.contains(&child.id));
        assert!(effective.contains(&DEVELOPER_ROLE_ID.to_string()));
    }

    #[tokio::test]
    async fn test_get_role_by_name() {
        let manager = RbacManager::new();

        let admin = manager.get_role_by_name("admin").await;
        assert!(admin.is_some());
        assert_eq!(admin.unwrap().id, ADMIN_ROLE_ID);

        let nonexistent = manager.get_role_by_name("nonexistent").await;
        assert!(nonexistent.is_none());
    }

    #[tokio::test]
    async fn test_cannot_create_role_with_invalid_parent() {
        let manager = RbacManager::new();

        let role = Role::with_parent(
            "orphan",
            "Role with invalid parent",
            vec![],
            "nonexistent-parent",
        );
        let result = manager.create_role(role).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RbacError::RoleNotFound(_)));
    }

    #[tokio::test]
    async fn test_cannot_delete_role_with_children() {
        let manager = RbacManager::new();

        let parent = Role::new("parent-role", "Parent", vec![]);
        let parent = manager.create_role(parent).await.unwrap();

        let child = Role::with_parent("child-role", "Child", vec![], &parent.id);
        manager.create_role(child).await.unwrap();

        // Cannot delete parent while child exists
        let result = manager.delete_role(&parent.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_assign_nonexistent_role_fails() {
        let manager = RbacManager::new();

        let result = manager
            .assign_role("user:alice", "nonexistent-role", "admin:root")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RbacError::RoleNotFound(_)));
    }

    #[tokio::test]
    async fn test_revoke_nonexistent_assignment_fails() {
        let manager = RbacManager::new();

        let result = manager.revoke_role("nonexistent-assignment").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RbacError::AssignmentNotFound(_)
        ));
    }

    // -----------------------------------------------------------------------
    // Data Masking tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_data_masker_redact() {
        let result = DataMasker::apply_mask("sensitive-value", &MaskType::Redact);
        assert_eq!(result, "***REDACTED***");
    }

    #[tokio::test]
    async fn test_data_masker_hash() {
        let result = DataMasker::apply_mask("sensitive-value", &MaskType::Hash);
        // Should be a SHA-256 hex string
        assert_eq!(result.len(), 64);
        // Should be deterministic
        let result2 = DataMasker::apply_mask("sensitive-value", &MaskType::Hash);
        assert_eq!(result, result2);
    }

    #[tokio::test]
    async fn test_data_masker_partial() {
        let result = DataMasker::apply_mask("1234567890", &MaskType::Partial);
        assert_eq!(result, "12******90");

        // Short values should be fully masked
        let result = DataMasker::apply_mask("abc", &MaskType::Partial);
        assert_eq!(result, "****");
    }

    #[tokio::test]
    async fn test_data_masking_policy() {
        let masker = DataMasker::new();

        let policy = DataMaskingPolicy::new(
            "mask-emails",
            vec!["email".to_string(), "*_email".to_string()],
            MaskType::Redact,
            vec![VIEWER_ROLE_ID.to_string()],
        );
        masker.add_policy(policy).await;

        // Viewer should be masked
        let mask_type = masker
            .should_mask("email", &[VIEWER_ROLE_ID.to_string()])
            .await;
        assert_eq!(mask_type, Some(MaskType::Redact));

        // Pattern match
        let mask_type = masker
            .should_mask("user_email", &[VIEWER_ROLE_ID.to_string()])
            .await;
        assert_eq!(mask_type, Some(MaskType::Redact));

        // Admin should not be masked (not in applies_to_roles)
        let mask_type = masker
            .should_mask("email", &[ADMIN_ROLE_ID.to_string()])
            .await;
        assert!(mask_type.is_none());

        // Non-matching field should not be masked
        let mask_type = masker
            .should_mask("name", &[VIEWER_ROLE_ID.to_string()])
            .await;
        assert!(mask_type.is_none());
    }

    #[tokio::test]
    async fn test_data_masking_policy_applies_to_all_non_admin() {
        let masker = DataMasker::new();

        // Empty applies_to_roles = applies to all non-admin roles
        let policy = DataMaskingPolicy::new(
            "mask-ssn",
            vec!["ssn".to_string()],
            MaskType::Hash,
            vec![], // Empty = all non-admin
        );
        masker.add_policy(policy).await;

        // Non-admin should be masked
        let mask_type = masker
            .should_mask("ssn", &[VIEWER_ROLE_ID.to_string()])
            .await;
        assert_eq!(mask_type, Some(MaskType::Hash));

        // Admin should NOT be masked
        let mask_type = masker
            .should_mask("ssn", &[ADMIN_ROLE_ID.to_string()])
            .await;
        assert!(mask_type.is_none());
    }

    #[tokio::test]
    async fn test_remove_masking_policy() {
        let masker = DataMasker::new();

        let policy = DataMaskingPolicy::new(
            "temp-policy",
            vec!["field".to_string()],
            MaskType::Redact,
            vec![],
        );
        let policy_id = policy.id.clone();
        masker.add_policy(policy).await;

        assert_eq!(masker.list_policies().await.len(), 1);

        let removed = masker.remove_policy(&policy_id).await;
        assert!(removed);
        assert!(masker.list_policies().await.is_empty());

        // Removing non-existent should return false
        let removed = masker.remove_policy("nonexistent").await;
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_field_pattern_matching() {
        assert!(field_pattern_matches("*", "anything"));
        assert!(field_pattern_matches("**", "anything"));
        assert!(field_pattern_matches("email", "email"));
        assert!(field_pattern_matches("user_*", "user_email"));
        assert!(field_pattern_matches("*_email", "user_email"));

        assert!(!field_pattern_matches("email", "name"));
        assert!(!field_pattern_matches("user_*", "admin_email"));
    }

    #[tokio::test]
    async fn test_permission_allows() {
        let perm = Permission::new(
            AclResource::Topic("orders-*".to_string()),
            vec![AclAction::Read, AclAction::Write],
        );

        assert!(perm.allows(&AclResource::Topic("orders-eu".to_string()), AclAction::Read));
        assert!(perm.allows(&AclResource::Topic("orders-us".to_string()), AclAction::Write));
        assert!(!perm.allows(&AclResource::Topic("orders-eu".to_string()), AclAction::Admin));
        assert!(!perm.allows(&AclResource::Topic("payments".to_string()), AclAction::Read));
    }

    #[tokio::test]
    async fn test_mask_type_display() {
        assert_eq!(format!("{}", MaskType::Redact), "redact");
        assert_eq!(format!("{}", MaskType::Hash), "hash");
        assert_eq!(format!("{}", MaskType::Partial), "partial");
    }

    #[tokio::test]
    async fn test_multiple_roles_combined() {
        let manager = RbacManager::new();

        // Create a custom role with specific topic access
        let custom_role = Role::new(
            "orders-writer",
            "Can only write to orders topics",
            vec![Permission::new(
                AclResource::Topic("orders-*".to_string()),
                vec![AclAction::Write],
            )],
        );
        let custom_role = manager.create_role(custom_role).await.unwrap();

        // Assign both viewer (global read) and custom role (orders write)
        manager
            .assign_role("user:alice", VIEWER_ROLE_ID, "admin:root")
            .await
            .unwrap();
        manager
            .assign_role("user:alice", &custom_role.id, "admin:root")
            .await
            .unwrap();

        // Can read anything (from viewer)
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("payments".to_string()),
                    AclAction::Read,
                )
                .await
        );

        // Can write to orders (from custom role)
        assert!(
            manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("orders-eu".to_string()),
                    AclAction::Write,
                )
                .await
        );

        // Cannot write to payments (no role grants this)
        assert!(
            !manager
                .is_allowed(
                    "user:alice",
                    &AclResource::Topic("payments".to_string()),
                    AclAction::Write,
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_delete_role_removes_assignments() {
        let manager = RbacManager::new();

        let role = Role::new("temp-role", "Temporary", vec![]);
        let role = manager.create_role(role).await.unwrap();

        manager
            .assign_role("user:alice", &role.id, "admin:root")
            .await
            .unwrap();

        assert_eq!(manager.get_roles_for_principal("user:alice").await.len(), 1);

        // Delete the role
        manager.delete_role(&role.id).await.unwrap();

        // Assignment should be gone
        assert!(manager.get_roles_for_principal("user:alice").await.is_empty());
    }

    #[tokio::test]
    async fn test_default_rbac_resource_extractor() {
        let result = default_rbac_resource_extractor("GET", "/api/v1/topics");
        assert!(matches!(
            result,
            Some((AclResource::All, AclAction::Describe))
        ));

        let result = default_rbac_resource_extractor("GET", "/api/v1/topics/orders");
        assert!(
            matches!(result, Some((AclResource::Topic(name), AclAction::Describe)) if name == "orders")
        );

        let result = default_rbac_resource_extractor("GET", "/health");
        assert!(result.is_none());
    }
}
