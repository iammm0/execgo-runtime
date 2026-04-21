use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;

use crate::types::{
    ExecutionCapabilities, NamespaceCapabilities, ResourceCapabilities, ResourceCapacity,
    RuntimeCapabilities, RuntimePlatform, SandboxCapabilities, StorageCapabilities,
};

#[derive(Debug, Clone)]
pub struct CapabilityProbeInput {
    pub runtime_id: String,
    pub data_dir: PathBuf,
    pub cgroup_root: PathBuf,
    pub max_running_tasks: usize,
    pub disable_linux_sandbox: bool,
    pub disable_cgroup: bool,
    pub capacity_memory_bytes: Option<u64>,
    pub capacity_pids: Option<u64>,
}

pub fn probe_runtime_capabilities(input: &CapabilityProbeInput) -> RuntimeCapabilities {
    let mut warnings = Vec::new();
    let mut overrides = BTreeMap::new();

    if input.disable_linux_sandbox {
        overrides.insert("linux_sandbox".into(), "disabled".into());
    }
    if input.disable_cgroup {
        overrides.insert("cgroup".into(), "disabled".into());
    }
    if let Some(value) = input.capacity_memory_bytes {
        overrides.insert("capacity_memory_bytes".into(), value.to_string());
    }
    if let Some(value) = input.capacity_pids {
        overrides.insert("capacity_pids".into(), value.to_string());
    }

    let data_dir_writable = data_dir_writable(&input.data_dir);
    if !data_dir_writable {
        warnings.push(format!(
            "data-dir is not writable: {}",
            input.data_dir.to_string_lossy()
        ));
    }

    let platform = RuntimePlatform {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        containerized: detect_containerized(),
        kubernetes: detect_kubernetes(),
    };

    let linux = cfg!(target_os = "linux");
    let root_user = current_euid() == Some(0);
    let linux_sandbox = linux && root_user && !input.disable_linux_sandbox;
    if !linux_sandbox {
        warnings.push(if linux {
            "linux_sandbox is unavailable without root-equivalent namespace permissions".into()
        } else {
            "linux_sandbox is unavailable on this host".into()
        });
    }

    let cgroup_v2 = linux && Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
    let cgroup_writable =
        cgroup_v2 && !input.disable_cgroup && path_likely_writable(&input.cgroup_root);
    if cgroup_v2 && !cgroup_writable {
        warnings.push(format!(
            "cgroup v2 detected but cgroup root is not writable: {}",
            input.cgroup_root.to_string_lossy()
        ));
    }

    let memory_capacity = input.capacity_memory_bytes.or_else(detect_memory_bytes);
    let pids_capacity = input.capacity_pids.or_else(detect_pids_capacity);

    let resources = ResourceCapabilities {
        rlimit_cpu: cfg!(unix),
        rlimit_memory: cfg!(unix),
        cgroup_v2,
        cgroup_writable,
        memory_limit: cfg!(unix),
        pids_limit: cgroup_writable,
        oom_detection: cgroup_writable,
        cpu_quota: false,
        ledger: true,
        capacity: ResourceCapacity {
            task_slots: input.max_running_tasks as u64,
            memory_bytes: memory_capacity,
            pids: pids_capacity,
        },
    };

    RuntimeCapabilities {
        runtime_id: input.runtime_id.clone(),
        snapshot_version: RuntimeCapabilities::snapshot_version().to_string(),
        collected_at: Utc::now(),
        platform,
        execution: ExecutionCapabilities {
            command: true,
            script: true,
            process_group: cfg!(unix),
        },
        sandbox: SandboxCapabilities {
            process: true,
            linux_sandbox,
            chroot: linux_sandbox,
            namespaces: NamespaceCapabilities {
                mount: linux_sandbox,
                pid: linux_sandbox,
                uts: linux_sandbox,
                ipc: linux_sandbox,
                net: linux_sandbox,
            },
        },
        storage: StorageCapabilities { data_dir_writable },
        resources,
        stable_semantics: stable_semantics(),
        enhanced_semantics: enhanced_semantics(linux_sandbox, cgroup_writable),
        degraded: !warnings.is_empty(),
        warnings,
        overrides,
    }
}

fn stable_semantics() -> Vec<String> {
    [
        "submit",
        "status",
        "events",
        "stdout_stderr",
        "timeout",
        "kill",
        "artifacts",
        "result_persistence",
        "recovery",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn enhanced_semantics(linux_sandbox: bool, cgroup_writable: bool) -> Vec<String> {
    let mut items = vec!["resource_ledger".to_string()];
    if linux_sandbox {
        items.extend([
            "linux_sandbox".to_string(),
            "namespaces".to_string(),
            "chroot".to_string(),
        ]);
    }
    if cgroup_writable {
        items.extend([
            "cgroup_memory".to_string(),
            "cgroup_pids".to_string(),
            "oom_detection".to_string(),
        ]);
    }
    items
}

fn data_dir_writable(path: &Path) -> bool {
    let probe_path = path.join(format!(".execgo-runtime-probe-{}", std::process::id()));
    match fs::write(&probe_path, b"probe") {
        Ok(()) => {
            let _ = fs::remove_file(probe_path);
            true
        }
        Err(_) => false,
    }
}

fn path_likely_writable(path: &Path) -> bool {
    let candidate = if path.exists() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    fs::metadata(candidate)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false)
}

fn detect_containerized() -> bool {
    Path::new("/.dockerenv").exists()
        || Path::new("/run/.containerenv").exists()
        || read_to_string("/proc/1/cgroup").is_some_and(|contents| {
            contents.contains("docker")
                || contents.contains("containerd")
                || contents.contains("kubepods")
        })
}

fn detect_kubernetes() -> bool {
    std::env::var_os("KUBERNETES_SERVICE_HOST").is_some()
        || read_to_string("/proc/1/cgroup").is_some_and(|contents| contents.contains("kubepods"))
}

fn detect_memory_bytes() -> Option<u64> {
    #[cfg(unix)]
    unsafe {
        let pages = libc::sysconf(libc::_SC_PHYS_PAGES);
        let page_size = libc::sysconf(libc::_SC_PAGESIZE);
        if pages > 0 && page_size > 0 {
            return Some((pages as u64).saturating_mul(page_size as u64));
        }
    }
    None
}

fn detect_pids_capacity() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        read_to_string("/proc/sys/kernel/pid_max")
            .and_then(|value| value.trim().parse::<u64>().ok())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn current_euid() -> Option<u32> {
    #[cfg(unix)]
    unsafe {
        Some(libc::geteuid())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn read_to_string(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn probe_returns_stable_shape() {
        let temp = TempDir::new().expect("tempdir");
        let capabilities = probe_runtime_capabilities(&CapabilityProbeInput {
            runtime_id: "test-runtime".into(),
            data_dir: temp.path().to_path_buf(),
            cgroup_root: temp.path().join("cgroup"),
            max_running_tasks: 3,
            disable_linux_sandbox: true,
            disable_cgroup: true,
            capacity_memory_bytes: Some(1024),
            capacity_pids: Some(64),
        });

        assert_eq!(capabilities.runtime_id, "test-runtime");
        assert_eq!(capabilities.resources.capacity.task_slots, 3);
        assert_eq!(capabilities.resources.capacity.memory_bytes, Some(1024));
        assert!(!capabilities.sandbox.linux_sandbox);
        assert_eq!(
            capabilities
                .overrides
                .get("linux_sandbox")
                .map(String::as_str),
            Some("disabled")
        );
    }
}
