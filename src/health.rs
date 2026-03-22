//! Container health monitoring — wraps majra HeartbeatTracker for container liveness.

use majra::heartbeat::{ConcurrentHeartbeatTracker, HeartbeatConfig, Status};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

/// Container restart policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// Never restart (default).
    Never,
    /// Always restart on exit.
    Always,
    /// Restart only on non-zero exit, up to max_retries.
    OnFailure { max_retries: u32 },
    /// Restart unless explicitly stopped by user.
    UnlessStopped,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::Never
    }
}

/// Health status of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Starting,
    Unknown,
}

impl From<Status> for HealthStatus {
    fn from(s: Status) -> Self {
        match s {
            Status::Online => HealthStatus::Healthy,
            Status::Suspect => HealthStatus::Starting,
            Status::Offline => HealthStatus::Unhealthy,
            _ => HealthStatus::Unknown,
        }
    }
}

/// Restart tracking for a container.
#[derive(Debug, Clone)]
struct RestartState {
    policy: RestartPolicy,
    restart_count: u32,
    user_stopped: bool,
}

/// Container health monitor using majra's heartbeat FSM.
pub struct HealthMonitor {
    tracker: ConcurrentHeartbeatTracker,
    restart_states: RwLock<HashMap<String, RestartState>>,
}

impl HealthMonitor {
    /// Create a new health monitor.
    pub fn new() -> Self {
        Self::with_config(HeartbeatConfig {
            suspect_after: Duration::from_secs(30),
            offline_after: Duration::from_secs(90),
            eviction_policy: None,
        })
    }

    /// Create with custom heartbeat configuration.
    pub fn with_config(config: HeartbeatConfig) -> Self {
        Self {
            tracker: ConcurrentHeartbeatTracker::new(config),
            restart_states: RwLock::new(HashMap::new()),
        }
    }

    /// Register a container for health tracking.
    pub async fn register(&self, container_id: &str, restart_policy: RestartPolicy) {
        let metadata = serde_json::json!({
            "type": "container",
            "restart_policy": format!("{restart_policy:?}"),
        });
        self.tracker.register(container_id, metadata);

        let mut states = self.restart_states.write().await;
        states.insert(
            container_id.to_string(),
            RestartState {
                policy: restart_policy,
                restart_count: 0,
                user_stopped: false,
            },
        );

        info!(container = container_id, "registered for health monitoring");
    }

    /// Record a successful heartbeat for a container.
    pub fn heartbeat(&self, container_id: &str) -> bool {
        self.tracker.heartbeat(container_id)
    }

    /// Deregister a container from health tracking.
    pub async fn deregister(&self, container_id: &str) {
        self.tracker.deregister(container_id);
        self.restart_states.write().await.remove(container_id);
    }

    /// Mark a container as explicitly stopped by the user.
    pub async fn mark_user_stopped(&self, container_id: &str) {
        let mut states = self.restart_states.write().await;
        if let Some(state) = states.get_mut(container_id) {
            state.user_stopped = true;
        }
    }

    /// Get health status for a container.
    pub fn get_status(&self, container_id: &str) -> HealthStatus {
        self.tracker
            .get(container_id)
            .map(|ns| HealthStatus::from(ns.status))
            .unwrap_or(HealthStatus::Unknown)
    }

    /// Update all container statuses and return containers that need restart.
    pub async fn check_and_restart(&self) -> Vec<String> {
        // Update the FSM — this transitions Online→Suspect→Offline.
        let _ = self.tracker.update_statuses();

        // Check all currently-offline containers for restart eligibility.
        let offline = self.tracker.list_by_status(Status::Offline);
        let mut needs_restart = Vec::new();

        let mut states = self.restart_states.write().await;

        for (id, _node_state) in &offline {
            if let Some(state) = states.get_mut(id) {
                let should_restart = match &state.policy {
                    RestartPolicy::Never => false,
                    RestartPolicy::Always => !state.user_stopped,
                    RestartPolicy::OnFailure { max_retries } => {
                        !state.user_stopped && state.restart_count < *max_retries
                    }
                    RestartPolicy::UnlessStopped => !state.user_stopped,
                };

                if should_restart {
                    state.restart_count += 1;
                    needs_restart.push(id.clone());
                    info!(
                        container = id.as_str(),
                        restart_count = state.restart_count,
                        "container needs restart"
                    );
                }
            }
        }

        needs_restart
    }

    /// Get the restart count for a container.
    pub async fn restart_count(&self, container_id: &str) -> u32 {
        self.restart_states
            .read()
            .await
            .get(container_id)
            .map(|s| s.restart_count)
            .unwrap_or(0)
    }

    /// Number of tracked containers.
    pub fn len(&self) -> usize {
        self.tracker.len()
    }

    /// Whether any containers are tracked.
    pub fn is_empty(&self) -> bool {
        self.tracker.is_empty()
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_from_majra() {
        assert_eq!(HealthStatus::from(Status::Online), HealthStatus::Healthy);
        assert_eq!(HealthStatus::from(Status::Suspect), HealthStatus::Starting);
        assert_eq!(HealthStatus::from(Status::Offline), HealthStatus::Unhealthy);
    }

    #[test]
    fn health_status_serde() {
        for status in [
            HealthStatus::Healthy,
            HealthStatus::Unhealthy,
            HealthStatus::Starting,
            HealthStatus::Unknown,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: HealthStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[tokio::test]
    async fn register_and_heartbeat() {
        let monitor = HealthMonitor::new();
        monitor.register("c1", RestartPolicy::Never).await;

        assert_eq!(monitor.len(), 1);
        assert!(monitor.heartbeat("c1"));
        assert_eq!(monitor.get_status("c1"), HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn deregister() {
        let monitor = HealthMonitor::new();
        monitor.register("c1", RestartPolicy::Never).await;
        monitor.deregister("c1").await;
        assert!(monitor.is_empty());
    }

    #[tokio::test]
    async fn restart_count_increments() {
        let monitor = HealthMonitor::with_config(HeartbeatConfig {
            suspect_after: Duration::from_millis(1),
            offline_after: Duration::from_millis(2),
            eviction_policy: None,
        });

        monitor.register("c1", RestartPolicy::Always).await;

        // Wait for offline detection.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let needs_restart = monitor.check_and_restart().await;
        assert!(needs_restart.contains(&"c1".to_string()));
        assert_eq!(monitor.restart_count("c1").await, 1);
    }

    #[tokio::test]
    async fn never_policy_no_restart() {
        let monitor = HealthMonitor::with_config(HeartbeatConfig {
            suspect_after: Duration::from_millis(1),
            offline_after: Duration::from_millis(2),
            eviction_policy: None,
        });

        monitor.register("c1", RestartPolicy::Never).await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let needs_restart = monitor.check_and_restart().await;
        assert!(!needs_restart.contains(&"c1".to_string()));
    }

    #[tokio::test]
    async fn on_failure_max_retries() {
        let monitor = HealthMonitor::with_config(HeartbeatConfig {
            suspect_after: Duration::from_millis(1),
            offline_after: Duration::from_millis(2),
            eviction_policy: None,
        });

        monitor
            .register("c1", RestartPolicy::OnFailure { max_retries: 2 })
            .await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        // First restart.
        let r1 = monitor.check_and_restart().await;
        assert!(r1.contains(&"c1".to_string()));

        // Second restart.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let r2 = monitor.check_and_restart().await;
        assert!(r2.contains(&"c1".to_string()));

        // Third attempt — should NOT restart (max_retries = 2).
        tokio::time::sleep(Duration::from_millis(10)).await;
        let r3 = monitor.check_and_restart().await;
        assert!(!r3.contains(&"c1".to_string()));

        assert_eq!(monitor.restart_count("c1").await, 2);
    }

    #[tokio::test]
    async fn unless_stopped_respects_user_stop() {
        let monitor = HealthMonitor::with_config(HeartbeatConfig {
            suspect_after: Duration::from_millis(1),
            offline_after: Duration::from_millis(2),
            eviction_policy: None,
        });

        monitor.register("c1", RestartPolicy::UnlessStopped).await;

        // User explicitly stops.
        monitor.mark_user_stopped("c1").await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let needs_restart = monitor.check_and_restart().await;
        assert!(!needs_restart.contains(&"c1".to_string()));
    }

    #[test]
    fn unknown_status_for_unregistered() {
        let monitor = HealthMonitor::new();
        assert_eq!(monitor.get_status("nonexistent"), HealthStatus::Unknown);
    }

    #[test]
    fn heartbeat_unregistered_returns_false() {
        let monitor = HealthMonitor::new();
        assert!(!monitor.heartbeat("nonexistent"));
    }

    #[test]
    fn default_monitor() {
        let monitor = HealthMonitor::default();
        assert!(monitor.is_empty());
    }
}
