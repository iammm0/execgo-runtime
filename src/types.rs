use std::{
    collections::{BTreeMap, HashMap},
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

const DEFAULT_OUTPUT_INLINE_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_WALL_TIME_MS: u64 = 5 * 60 * 1000;
const CAPABILITY_SNAPSHOT_VERSION: &str = "v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Accepted,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    LaunchFailed,
    Timeout,
    Cancelled,
    MemoryLimitExceeded,
    CpuLimitExceeded,
    ResourceLimitExceeded,
    SandboxSetupFailed,
    ExitNonZero,
    UnsupportedCapability,
    InsufficientResources,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeErrorInfo {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Submitted,
    Accepted,
    Planned,
    Degraded,
    ResourceReserved,
    ResourceReleased,
    Started,
    KillRequested,
    TimeoutTriggered,
    Finished,
    Failed,
    Cancelled,
    Recovered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionKind {
    Command,
    Script,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionSpec {
    pub kind: ExecutionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<Vec<String>>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl ExecutionSpec {
    pub fn validate(&self) -> AppResult<()> {
        match self.kind {
            ExecutionKind::Command => {
                let program = self.program.as_deref().map(str::trim).unwrap_or_default();
                if program.is_empty() {
                    return Err(AppError::InvalidInput(
                        "execution.program is required for command tasks".into(),
                    ));
                }
                if self.script.as_ref().is_some_and(|v| !v.trim().is_empty()) {
                    return Err(AppError::InvalidInput(
                        "execution.script must be empty for command tasks".into(),
                    ));
                }
            }
            ExecutionKind::Script => {
                let script = self.script.as_deref().map(str::trim).unwrap_or_default();
                if script.is_empty() {
                    return Err(AppError::InvalidInput(
                        "execution.script is required for script tasks".into(),
                    ));
                }
                if self.program.as_ref().is_some_and(|v| !v.trim().is_empty()) {
                    return Err(AppError::InvalidInput(
                        "execution.program must be empty for script tasks".into(),
                    ));
                }
                if let Some(interpreter) = &self.interpreter {
                    if interpreter.is_empty() || interpreter[0].trim().is_empty() {
                        return Err(AppError::InvalidInput(
                            "execution.interpreter must contain at least one non-empty value"
                                .into(),
                        ));
                    }
                }
            }
        }

        if self
            .env
            .keys()
            .any(|key| key.trim().is_empty() || key.contains('='))
        {
            return Err(AppError::InvalidInput(
                "execution.env keys must be non-empty and cannot contain '='".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMode {
    #[default]
    Adaptive,
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TaskPolicy {
    #[serde(default)]
    pub capability_mode: CapabilityMode,
}

impl TaskPolicy {
    pub fn validate(&self) -> AppResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ControlContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_plane_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_runtime_profile: Option<String>,
    #[serde(default)]
    pub requires_strict_sandbox: bool,
    #[serde(default)]
    pub requires_resource_reservation: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl ControlContext {
    pub fn validate(&self) -> AppResult<()> {
        if self
            .labels
            .keys()
            .any(|key| key.trim().is_empty() || key.contains('='))
        {
            return Err(AppError::InvalidInput(
                "control_context.labels keys must be non-empty and cannot contain '='".into(),
            ));
        }

        for value in [
            self.control_plane_mode.as_deref(),
            self.tenant.as_deref(),
            self.expected_runtime_profile.as_deref(),
        ] {
            if value.is_some_and(|item| item.trim().is_empty()) {
                return Err(AppError::InvalidInput(
                    "control_context values cannot be empty strings".into(),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    #[default]
    Process,
    LinuxSandbox,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamespaceConfig {
    #[serde(default = "default_true")]
    pub mount: bool,
    #[serde(default = "default_true")]
    pub pid: bool,
    #[serde(default = "default_true")]
    pub uts: bool,
    #[serde(default = "default_true")]
    pub ipc: bool,
    #[serde(default)]
    pub net: bool,
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            mount: true,
            pid: true,
            uts: true,
            ipc: true,
            net: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxPolicy {
    #[serde(default)]
    pub profile: SandboxProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_subdir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<String>,
    #[serde(default)]
    pub chroot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<NamespaceConfig>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            profile: SandboxProfile::Process,
            workspace_subdir: None,
            rootfs: None,
            chroot: false,
            namespaces: None,
        }
    }
}

impl SandboxPolicy {
    pub fn validate(&self) -> AppResult<()> {
        if let Some(subdir) = &self.workspace_subdir {
            validate_relative_workspace_subdir(subdir)?;
        }
        if self.chroot
            && self
                .rootfs
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(AppError::InvalidInput(
                "sandbox.rootfs is required when sandbox.chroot=true".into(),
            ));
        }
        if matches!(self.profile, SandboxProfile::Process) && self.chroot {
            return Err(AppError::InvalidInput(
                "sandbox.chroot requires sandbox.profile=linux_sandbox".into(),
            ));
        }
        Ok(())
    }

    pub fn effective_namespaces(&self) -> NamespaceConfig {
        self.namespaces.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceLimits {
    #[serde(default = "default_wall_time_ms")]
    pub wall_time_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_time_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u64>,
    #[serde(default = "default_output_inline_bytes")]
    pub stdout_max_bytes: u64,
    #[serde(default = "default_output_inline_bytes")]
    pub stderr_max_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            wall_time_ms: default_wall_time_ms(),
            cpu_time_sec: None,
            memory_bytes: None,
            pids_max: None,
            stdout_max_bytes: default_output_inline_bytes(),
            stderr_max_bytes: default_output_inline_bytes(),
        }
    }
}

impl ResourceLimits {
    pub fn validate(&self) -> AppResult<()> {
        if self.wall_time_ms == 0 {
            return Err(AppError::InvalidInput(
                "limits.wall_time_ms must be greater than 0".into(),
            ));
        }
        if self.stdout_max_bytes == 0 || self.stderr_max_bytes == 0 {
            return Err(AppError::InvalidInput(
                "limits.stdout_max_bytes and limits.stderr_max_bytes must be greater than 0".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceEnforcementPlan {
    pub wall_time_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_time_sec: Option<u64>,
    #[serde(default)]
    pub cpu_time_enforced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default)]
    pub memory_enforced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u64>,
    #[serde(default)]
    pub pids_enforced: bool,
    #[serde(default)]
    pub cgroup_enforced: bool,
    #[serde(default)]
    pub oom_detection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlan {
    #[serde(default)]
    pub capability_mode: CapabilityMode,
    pub requested_sandbox: SandboxPolicy,
    pub effective_sandbox: SandboxPolicy,
    pub resource_enforcement: ResourceEnforcementPlan,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_warnings: Vec<String>,
}

impl ExecutionPlan {
    pub fn legacy(sandbox: SandboxPolicy, limits: ResourceLimits) -> Self {
        let cgroup_enforced = matches!(sandbox.profile, SandboxProfile::LinuxSandbox);
        Self {
            capability_mode: CapabilityMode::Adaptive,
            requested_sandbox: sandbox.clone(),
            effective_sandbox: sandbox,
            resource_enforcement: ResourceEnforcementPlan {
                wall_time_ms: limits.wall_time_ms,
                cpu_time_sec: limits.cpu_time_sec,
                cpu_time_enforced: limits.cpu_time_sec.is_some(),
                memory_bytes: limits.memory_bytes,
                memory_enforced: limits.memory_bytes.is_some(),
                pids_max: limits.pids_max,
                pids_enforced: limits.pids_max.is_some() && cgroup_enforced,
                cgroup_enforced,
                oom_detection: limits.memory_bytes.is_some() && cgroup_enforced,
            },
            degraded: false,
            fallback_reasons: Vec::new(),
            capability_warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskResourceReservation {
    pub task_slots: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
}

impl TaskResourceReservation {
    pub fn from_limits(limits: &ResourceLimits) -> Self {
        Self {
            task_slots: 1,
            memory_bytes: limits.memory_bytes,
            pids: limits.pids_max,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmitTaskRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub execution: ExecutionSpec,
    #[serde(default)]
    pub limits: ResourceLimits,
    #[serde(default)]
    pub sandbox: SandboxPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<TaskPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_context: Option<ControlContext>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl SubmitTaskRequest {
    pub fn validate(&self) -> AppResult<()> {
        if let Some(task_id) = &self.task_id {
            validate_task_id(task_id)?;
        }
        self.execution.validate()?;
        self.limits.validate()?;
        self.sandbox.validate()?;
        if let Some(policy) = &self.policy {
            policy.validate()?;
        }
        if let Some(control_context) = &self.control_context {
            control_context.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmitTaskResponse {
    pub task_id: String,
    pub handle_id: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceUsage {
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_cpu_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_cpu_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_peak_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskArtifacts {
    pub task_dir: String,
    pub request_path: String,
    pub result_path: String,
    pub stdout_path: String,
    pub stderr_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatusResponse {
    pub task_id: String,
    pub handle_id: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shim_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pgid: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_signal: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RuntimeErrorInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResourceUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_plan: Option<ExecutionPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservation: Option<TaskResourceReservation>,
    pub artifacts: TaskArtifacts,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventRecord {
    pub seq: i64,
    pub task_id: String,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePlatform {
    pub os: String,
    pub arch: String,
    pub containerized: bool,
    pub kubernetes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionCapabilities {
    pub command: bool,
    pub script: bool,
    pub process_group: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamespaceCapabilities {
    pub mount: bool,
    pub pid: bool,
    pub uts: bool,
    pub ipc: bool,
    pub net: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxCapabilities {
    pub process: bool,
    pub linux_sandbox: bool,
    pub chroot: bool,
    pub namespaces: NamespaceCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageCapabilities {
    pub data_dir_writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceCapacity {
    pub task_slots: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceCapabilities {
    pub rlimit_cpu: bool,
    pub rlimit_memory: bool,
    pub cgroup_v2: bool,
    pub cgroup_writable: bool,
    pub memory_limit: bool,
    pub pids_limit: bool,
    pub oom_detection: bool,
    pub cpu_quota: bool,
    pub ledger: bool,
    pub capacity: ResourceCapacity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeCapabilities {
    pub runtime_id: String,
    pub snapshot_version: String,
    pub collected_at: DateTime<Utc>,
    pub platform: RuntimePlatform,
    pub execution: ExecutionCapabilities,
    pub sandbox: SandboxCapabilities,
    pub storage: StorageCapabilities,
    pub resources: ResourceCapabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stable_semantics: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enhanced_semantics: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, String>,
}

impl RuntimeCapabilities {
    pub fn snapshot_version() -> &'static str {
        CAPABILITY_SNAPSHOT_VERSION
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeInfoResponse {
    pub runtime_id: String,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub snapshot_version: String,
    pub platform: RuntimePlatform,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeConfigResponse {
    pub runtime_id: String,
    pub listen_addr: String,
    pub data_dir: String,
    pub max_running_tasks: usize,
    pub max_queued_tasks: usize,
    pub termination_grace_ms: u64,
    pub result_retention_secs: u64,
    pub gc_interval_ms: u64,
    pub dispatch_poll_interval_ms: u64,
    pub cgroup_root: String,
    pub default_capability_mode: CapabilityMode,
    pub cgroup_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveTaskReservation {
    pub task_id: String,
    pub status: TaskStatus,
    pub reservation: TaskResourceReservation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeResourcesResponse {
    pub runtime_id: String,
    pub capacity: ResourceCapacity,
    pub reserved: ResourceCapacity,
    pub available: ResourceCapacity,
    #[serde(default)]
    pub active_reservations: Vec<ActiveTaskReservation>,
    pub accepted_waiting_tasks: u64,
}

pub fn validate_task_id(task_id: &str) -> AppResult<()> {
    let trimmed = task_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("task_id cannot be empty".into()));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(AppError::InvalidInput(
            "task_id may only contain letters, digits, '-', '_' and '.'".into(),
        ));
    }
    Ok(())
}

pub fn resolve_workspace_dir(task_dir: &Path, sandbox: &SandboxPolicy) -> AppResult<PathBuf> {
    let base = task_dir.join("workspace");
    if let Some(subdir) = &sandbox.workspace_subdir {
        validate_relative_workspace_subdir(subdir)?;
        Ok(base.join(subdir))
    } else {
        Ok(base)
    }
}

pub fn default_output_inline_bytes() -> u64 {
    DEFAULT_OUTPUT_INLINE_BYTES
}

pub fn default_wall_time_ms() -> u64 {
    DEFAULT_WALL_TIME_MS
}

fn default_true() -> bool {
    true
}

fn validate_relative_workspace_subdir(subdir: &str) -> AppResult<()> {
    let path = Path::new(subdir);
    if path.is_absolute() {
        return Err(AppError::InvalidInput(
            "sandbox.workspace_subdir must be relative".into(),
        ));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::InvalidInput(
            "sandbox.workspace_subdir cannot contain parent traversal".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_command_execution() {
        let spec = ExecutionSpec {
            kind: ExecutionKind::Command,
            program: Some("echo".into()),
            args: vec!["ok".into()],
            script: None,
            interpreter: None,
            env: HashMap::new(),
        };
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn rejects_absolute_workspace_subdir() {
        let sandbox = SandboxPolicy {
            profile: SandboxProfile::Process,
            workspace_subdir: Some("/tmp".into()),
            rootfs: None,
            chroot: false,
            namespaces: None,
        };
        assert!(sandbox.validate().is_err());
    }

    #[test]
    fn default_policy_is_adaptive() {
        assert_eq!(
            TaskPolicy::default().capability_mode,
            CapabilityMode::Adaptive
        );
    }

    #[test]
    fn legacy_execution_plan_keeps_requested_sandbox() {
        let sandbox = SandboxPolicy {
            profile: SandboxProfile::LinuxSandbox,
            workspace_subdir: None,
            rootfs: None,
            chroot: false,
            namespaces: Some(NamespaceConfig::default()),
        };
        let plan = ExecutionPlan::legacy(sandbox.clone(), ResourceLimits::default());
        assert_eq!(plan.requested_sandbox, sandbox);
        assert_eq!(plan.effective_sandbox.profile, SandboxProfile::LinuxSandbox);
    }
}
