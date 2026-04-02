//! Daimon edge fleet scheduling — fleet-level container placement and migration.
//!
//! Provides types and scheduling functions for distributing containers across
//! a fleet of nodes managed by daimon.

use crate::container::ContainerConfig;
use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// A fleet deployment request — schedule a container across nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetDeployment {
    /// Deployment ID.
    pub id: String,
    /// Image reference.
    pub image: String,
    /// Container configuration.
    pub config: ContainerConfig,
    /// Target node constraints.
    pub constraints: DeploymentConstraints,
    /// Number of replicas across the fleet.
    pub replicas: u32,
    /// Deployment strategy.
    pub strategy: DeploymentStrategy,
    /// When this deployment was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Constraints for node selection during scheduling.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeploymentConstraints {
    /// Required node labels (all must match).
    #[serde(default)]
    pub node_labels: HashMap<String, String>,
    /// Required minimum memory in MB.
    pub min_memory_mb: Option<u64>,
    /// Required minimum CPU cores.
    pub min_cpus: Option<u32>,
    /// Preferred node IDs (soft constraint).
    #[serde(default)]
    pub preferred_nodes: Vec<String>,
}

/// Strategy for distributing replicas across fleet nodes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeploymentStrategy {
    /// Deploy to any available node (default).
    #[default]
    Spread,
    /// Pack onto fewest nodes possible.
    BinPack,
    /// Deploy to specific node only.
    Pinned {
        /// Target node ID.
        node_id: String,
    },
}

/// A fleet node — represents a machine in the daimon fleet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetNode {
    /// Unique node identifier.
    pub id: String,
    /// Node network address.
    pub address: String,
    /// Node labels for constraint matching.
    pub labels: HashMap<String, String>,
    /// Node resource capacity.
    pub capacity: NodeCapacity,
    /// Current node status.
    pub status: NodeStatus,
    /// Last heartbeat timestamp.
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// Resource capacity of a fleet node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapacity {
    /// Total memory in MB.
    pub memory_mb: u64,
    /// Total CPU cores.
    pub cpus: u32,
    /// Maximum containers this node can run.
    pub max_containers: u32,
    /// Currently running containers.
    pub running_containers: u32,
}

/// Status of a fleet node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NodeStatus {
    /// Node is ready to accept containers.
    Ready,
    /// Node is not responding or unhealthy.
    NotReady,
    /// Node is draining — no new containers, existing ones finishing.
    Draining,
    /// Node is cordoned — no new containers allowed.
    Cordoned,
}

/// Result of a fleet scheduling decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleResult {
    /// Node assignments: node_id -> number of replicas.
    pub assignments: HashMap<String, u32>,
    /// Reason for each assignment.
    pub reasons: Vec<String>,
}

/// Check if a node matches the given deployment constraints.
#[must_use]
#[inline]
fn node_matches_constraints(node: &FleetNode, constraints: &DeploymentConstraints) -> bool {
    // Check label constraints (all must match).
    for (key, value) in &constraints.node_labels {
        match node.labels.get(key) {
            Some(v) if v == value => {}
            _ => return false,
        }
    }

    // Check memory constraint.
    if let Some(min_mem) = constraints.min_memory_mb
        && node.capacity.memory_mb < min_mem
    {
        return false;
    }

    // Check CPU constraint.
    if let Some(min_cpus) = constraints.min_cpus
        && node.capacity.cpus < min_cpus
    {
        return false;
    }

    true
}

/// Remaining container slots on a node.
#[must_use]
#[inline]
fn free_slots(node: &FleetNode) -> u32 {
    node.capacity
        .max_containers
        .saturating_sub(node.capacity.running_containers)
}

/// Schedule a deployment across fleet nodes.
///
/// Selects nodes based on constraints, capacity, and strategy.
pub fn schedule(
    deployment: &FleetDeployment,
    nodes: &[FleetNode],
) -> Result<ScheduleResult, StivaError> {
    info!(
        deployment = %deployment.id,
        replicas = deployment.replicas,
        strategy = ?deployment.strategy,
        "scheduling fleet deployment"
    );

    // Filter to ready nodes that match constraints.
    let mut candidates: Vec<&FleetNode> = nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Ready)
        .filter(|n| node_matches_constraints(n, &deployment.constraints))
        .filter(|n| free_slots(n) > 0)
        .collect();

    if candidates.is_empty() {
        return Err(StivaError::Fleet(
            "no ready nodes matching constraints".to_string(),
        ));
    }

    let mut assignments: HashMap<String, u32> = HashMap::new();
    let mut reasons: Vec<String> = Vec::new();
    let mut remaining = deployment.replicas;

    match &deployment.strategy {
        DeploymentStrategy::Spread => {
            // Sort by fewest running containers (most headroom first).
            candidates.sort_by_key(|n| n.capacity.running_containers);

            // Round-robin across candidates.
            let mut idx = 0;
            while remaining > 0 {
                let node = candidates[idx % candidates.len()];
                let current = assignments.get(&node.id).copied().unwrap_or(0);
                let slots = free_slots(node).saturating_sub(current);
                if slots > 0 {
                    *assignments.entry(node.id.clone()).or_insert(0) += 1;
                    remaining -= 1;
                    reasons.push(format!("spread: assigned 1 replica to node {}", node.id));
                }
                idx += 1;
                // Safety: prevent infinite loop if total capacity is insufficient.
                if idx >= candidates.len() * (deployment.replicas as usize + 1) {
                    return Err(StivaError::Fleet(format!(
                        "insufficient capacity: need {} more replicas but no slots available",
                        remaining
                    )));
                }
            }
        }
        DeploymentStrategy::BinPack => {
            // Sort by most running containers (most packed first).
            candidates.sort_by(|a, b| {
                b.capacity
                    .running_containers
                    .cmp(&a.capacity.running_containers)
            });

            for node in &candidates {
                if remaining == 0 {
                    break;
                }
                let slots = free_slots(node);
                let assign = remaining.min(slots);
                if assign > 0 {
                    assignments.insert(node.id.clone(), assign);
                    reasons.push(format!(
                        "binpack: assigned {assign} replicas to node {}",
                        node.id
                    ));
                    remaining -= assign;
                }
            }
            if remaining > 0 {
                return Err(StivaError::Fleet(format!(
                    "insufficient capacity: need {remaining} more replicas"
                )));
            }
        }
        DeploymentStrategy::Pinned { node_id } => {
            let node = candidates
                .iter()
                .find(|n| n.id == *node_id)
                .ok_or_else(|| {
                    StivaError::Fleet(format!("pinned node '{node_id}' not found or not ready"))
                })?;
            let slots = free_slots(node);
            if slots < remaining {
                return Err(StivaError::Fleet(format!(
                    "pinned node '{node_id}' has only {slots} slots, need {remaining}"
                )));
            }
            assignments.insert(node_id.clone(), remaining);
            reasons.push(format!(
                "pinned: assigned {remaining} replicas to node {node_id}"
            ));
        }
    }

    info!(
        deployment = %deployment.id,
        assignments = ?assignments,
        "scheduling complete"
    );

    Ok(ScheduleResult {
        assignments,
        reasons,
    })
}

/// Select the best node for a single container migration.
///
/// Picks the ready node with the most free capacity that matches constraints.
pub fn select_migration_target(
    nodes: &[FleetNode],
    constraints: &DeploymentConstraints,
) -> Result<String, StivaError> {
    info!("selecting migration target node");

    let best = nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Ready)
        .filter(|n| node_matches_constraints(n, constraints))
        .filter(|n| free_slots(n) > 0)
        .max_by_key(|n| free_slots(n))
        .ok_or_else(|| StivaError::Fleet("no suitable migration target node found".to_string()))?;

    info!(target = %best.id, free_slots = free_slots(best), "migration target selected");
    Ok(best.id.clone())
}

// ---------------------------------------------------------------------------
// Fleet health monitoring
// ---------------------------------------------------------------------------

/// Check fleet node health based on heartbeat timestamps.
///
/// Nodes that haven't been seen within `timeout` are marked NotReady.
/// Returns the IDs of nodes whose status changed.
pub fn check_fleet_health(nodes: &mut [FleetNode], timeout: chrono::Duration) -> Vec<String> {
    let cutoff = chrono::Utc::now() - timeout;
    let mut changed = Vec::new();

    for node in nodes.iter_mut() {
        if node.status == NodeStatus::Ready && node.last_seen < cutoff {
            info!(
                node = %node.id,
                last_seen = %node.last_seen,
                "node heartbeat expired, marking NotReady"
            );
            node.status = NodeStatus::NotReady;
            changed.push(node.id.clone());
        }
    }

    changed
}

/// Plan a rollback: reschedule containers from failed nodes to healthy ones.
///
/// Returns a list of `(source_node, target_node)` pairs for container migration.
pub fn plan_rollback(
    nodes: &[FleetNode],
    constraints: &DeploymentConstraints,
) -> Vec<(String, String)> {
    let failed: Vec<&FleetNode> = nodes
        .iter()
        .filter(|n| n.status == NodeStatus::NotReady && n.capacity.running_containers > 0)
        .collect();

    let mut migrations = Vec::new();

    for failed_node in &failed {
        // Find a healthy target for each container on the failed node.
        for _ in 0..failed_node.capacity.running_containers {
            if let Ok(target) = select_migration_target(nodes, constraints)
                && target != failed_node.id
            {
                migrations.push((failed_node.id.clone(), target));
            }
        }
    }

    migrations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, status: NodeStatus, max: u32, running: u32) -> FleetNode {
        FleetNode {
            id: id.to_string(),
            address: format!("10.0.0.{}:8080", id.len()),
            labels: HashMap::new(),
            capacity: NodeCapacity {
                memory_mb: 4096,
                cpus: 4,
                max_containers: max,
                running_containers: running,
            },
            status,
            last_seen: chrono::Utc::now(),
        }
    }

    fn make_node_with_labels(
        id: &str,
        status: NodeStatus,
        max: u32,
        running: u32,
        labels: HashMap<String, String>,
    ) -> FleetNode {
        FleetNode {
            id: id.to_string(),
            address: format!("10.0.0.{}:8080", id.len()),
            labels,
            capacity: NodeCapacity {
                memory_mb: 4096,
                cpus: 4,
                max_containers: max,
                running_containers: running,
            },
            status,
            last_seen: chrono::Utc::now(),
        }
    }

    fn make_node_with_resources(
        id: &str,
        memory_mb: u64,
        cpus: u32,
        max: u32,
        running: u32,
    ) -> FleetNode {
        FleetNode {
            id: id.to_string(),
            address: "10.0.0.1:8080".to_string(),
            labels: HashMap::new(),
            capacity: NodeCapacity {
                memory_mb,
                cpus,
                max_containers: max,
                running_containers: running,
            },
            status: NodeStatus::Ready,
            last_seen: chrono::Utc::now(),
        }
    }

    fn make_deployment(replicas: u32, strategy: DeploymentStrategy) -> FleetDeployment {
        FleetDeployment {
            id: "deploy-1".to_string(),
            image: "docker.io/library/nginx:latest".to_string(),
            config: ContainerConfig::default(),
            constraints: DeploymentConstraints::default(),
            replicas,
            strategy,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn parse_deployment_serde() {
        let deployment = make_deployment(3, DeploymentStrategy::Spread);
        let json = serde_json::to_string(&deployment).unwrap();
        let back: FleetDeployment = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "deploy-1");
        assert_eq!(back.replicas, 3);
        assert_eq!(back.strategy, DeploymentStrategy::Spread);
        assert_eq!(back.image, "docker.io/library/nginx:latest");
    }

    #[test]
    fn schedule_spread() {
        let nodes = vec![
            make_node("node-1", NodeStatus::Ready, 10, 2),
            make_node("node-2", NodeStatus::Ready, 10, 0),
            make_node("node-3", NodeStatus::Ready, 10, 5),
        ];
        let deployment = make_deployment(3, DeploymentStrategy::Spread);
        let result = schedule(&deployment, &nodes).unwrap();

        // All 3 replicas should be assigned.
        let total: u32 = result.assignments.values().sum();
        assert_eq!(total, 3);

        // With spread strategy, replicas should be distributed.
        // node-2 has 0 running (lowest), node-1 has 2, node-3 has 5.
        // Each should get 1 replica.
        assert_eq!(result.assignments.len(), 3);
        assert_eq!(result.reasons.len(), 3);
    }

    #[test]
    fn schedule_binpack() {
        let nodes = vec![
            make_node("node-1", NodeStatus::Ready, 10, 7),
            make_node("node-2", NodeStatus::Ready, 10, 2),
            make_node("node-3", NodeStatus::Ready, 10, 0),
        ];
        let deployment = make_deployment(5, DeploymentStrategy::BinPack);
        let result = schedule(&deployment, &nodes).unwrap();

        let total: u32 = result.assignments.values().sum();
        assert_eq!(total, 5);

        // BinPack fills most-packed first: node-1 (7 running, 3 slots) gets 3,
        // then node-2 (2 running, 8 slots) gets 2.
        assert_eq!(*result.assignments.get("node-1").unwrap_or(&0), 3);
        assert_eq!(*result.assignments.get("node-2").unwrap_or(&0), 2);
    }

    #[test]
    fn schedule_pinned() {
        let nodes = vec![
            make_node("node-1", NodeStatus::Ready, 10, 2),
            make_node("node-2", NodeStatus::Ready, 10, 0),
        ];
        let deployment = make_deployment(
            3,
            DeploymentStrategy::Pinned {
                node_id: "node-2".to_string(),
            },
        );
        let result = schedule(&deployment, &nodes).unwrap();

        assert_eq!(result.assignments.len(), 1);
        assert_eq!(*result.assignments.get("node-2").unwrap(), 3);
    }

    #[test]
    fn schedule_no_ready_nodes() {
        let nodes = vec![
            make_node("node-1", NodeStatus::NotReady, 10, 0),
            make_node("node-2", NodeStatus::Draining, 10, 0),
            make_node("node-3", NodeStatus::Cordoned, 10, 0),
        ];
        let deployment = make_deployment(1, DeploymentStrategy::Spread);
        let err = schedule(&deployment, &nodes).unwrap_err();
        assert!(matches!(err, StivaError::Fleet(_)));
    }

    #[test]
    fn schedule_constraints_filter() {
        let mut labels = HashMap::new();
        labels.insert("zone".to_string(), "us-east".to_string());

        let nodes = vec![
            make_node_with_labels("node-1", NodeStatus::Ready, 10, 0, labels.clone()),
            make_node("node-2", NodeStatus::Ready, 10, 0), // no labels
            make_node_with_labels("node-3", NodeStatus::Ready, 10, 0, labels.clone()),
        ];

        let mut deployment = make_deployment(2, DeploymentStrategy::Spread);
        deployment.constraints.node_labels = labels;

        let result = schedule(&deployment, &nodes).unwrap();
        let total: u32 = result.assignments.values().sum();
        assert_eq!(total, 2);

        // Only node-1 and node-3 match the label constraint.
        assert!(result.assignments.contains_key("node-1"));
        assert!(result.assignments.contains_key("node-3"));
        assert!(!result.assignments.contains_key("node-2"));
    }

    #[test]
    fn schedule_constraints_memory_filter() {
        let nodes = vec![
            make_node_with_resources("small", 512, 2, 10, 0),
            make_node_with_resources("large", 8192, 8, 10, 0),
        ];

        let mut deployment = make_deployment(1, DeploymentStrategy::Spread);
        deployment.constraints.min_memory_mb = Some(4096);

        let result = schedule(&deployment, &nodes).unwrap();
        assert_eq!(result.assignments.len(), 1);
        assert!(result.assignments.contains_key("large"));
    }

    #[test]
    fn node_status_serde() {
        let statuses = [
            NodeStatus::Ready,
            NodeStatus::NotReady,
            NodeStatus::Draining,
            NodeStatus::Cordoned,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let back: NodeStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn deployment_strategy_default() {
        let strategy = DeploymentStrategy::default();
        assert_eq!(strategy, DeploymentStrategy::Spread);
    }

    #[test]
    fn select_migration_target_by_capacity() {
        let nodes = vec![
            make_node("node-1", NodeStatus::Ready, 10, 8), // 2 free
            make_node("node-2", NodeStatus::Ready, 10, 3), // 7 free
            make_node("node-3", NodeStatus::Ready, 10, 5), // 5 free
        ];

        let constraints = DeploymentConstraints::default();
        let target = select_migration_target(&nodes, &constraints).unwrap();
        // node-2 has the most free slots (7).
        assert_eq!(target, "node-2");
    }

    #[test]
    fn select_migration_target_no_ready() {
        let nodes = vec![make_node("node-1", NodeStatus::NotReady, 10, 0)];
        let constraints = DeploymentConstraints::default();
        let err = select_migration_target(&nodes, &constraints).unwrap_err();
        assert!(matches!(err, StivaError::Fleet(_)));
    }

    #[test]
    fn constraints_default() {
        let constraints = DeploymentConstraints::default();
        assert!(constraints.node_labels.is_empty());
        assert!(constraints.min_memory_mb.is_none());
        assert!(constraints.min_cpus.is_none());
        assert!(constraints.preferred_nodes.is_empty());

        // Default constraints should match any node.
        let node = make_node("any", NodeStatus::Ready, 10, 0);
        assert!(node_matches_constraints(&node, &constraints));
    }

    #[test]
    fn fleet_node_serde() {
        let node = make_node("node-1", NodeStatus::Ready, 10, 3);
        let json = serde_json::to_string(&node).unwrap();
        let back: FleetNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "node-1");
        assert_eq!(back.status, NodeStatus::Ready);
        assert_eq!(back.capacity.max_containers, 10);
        assert_eq!(back.capacity.running_containers, 3);
    }

    #[test]
    fn schedule_result_serde() {
        let result = ScheduleResult {
            assignments: HashMap::from([("node-1".to_string(), 2), ("node-2".to_string(), 1)]),
            reasons: vec!["test".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ScheduleResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.assignments.len(), 2);
        assert_eq!(*back.assignments.get("node-1").unwrap(), 2);
    }

    #[test]
    fn deployment_strategy_serde() {
        let strategies = vec![
            DeploymentStrategy::Spread,
            DeploymentStrategy::BinPack,
            DeploymentStrategy::Pinned {
                node_id: "node-1".to_string(),
            },
        ];
        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let back: DeploymentStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(strategy, back);
        }
    }
}
