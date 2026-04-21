use std::{path::PathBuf, time::Duration};

use clap::{ArgGroup, Args, Parser, Subcommand};

use crate::types::CapabilityMode;

#[derive(Debug, Parser)]
#[command(name = "execgo-runtime", version, about = "ExecGo runtime data plane")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve(ServeArgs),
    Submit(RemoteTaskArgs),
    Status(StatusArgs),
    Wait(WaitArgs),
    Kill(StatusArgs),
    Run(RemoteTaskArgs),
    #[command(hide = true, name = "internal-shim")]
    InternalShim(InternalShimArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub listen_addr: String,
    #[arg(long, default_value = "data")]
    pub data_dir: PathBuf,
    #[arg(long, default_value = "4")]
    pub max_running_tasks: usize,
    #[arg(long, default_value = "128")]
    pub max_queued_tasks: usize,
    #[arg(long, default_value = "5000")]
    pub termination_grace_ms: u64,
    #[arg(long, default_value = "604800")]
    pub result_retention_secs: u64,
    #[arg(long, default_value = "1000")]
    pub gc_interval_ms: u64,
    #[arg(long, default_value = "250")]
    pub dispatch_poll_interval_ms: u64,
    #[arg(long, default_value = "/sys/fs/cgroup/execgo-runtime")]
    pub cgroup_root: PathBuf,
    #[arg(long, env = "EXECGO_RUNTIME_ID")]
    pub runtime_id: Option<String>,
    #[arg(
        long,
        env = "EXECGO_RUNTIME_DEFAULT_CAPABILITY_MODE",
        default_value = "adaptive"
    )]
    pub default_capability_mode: CapabilityMode,
    #[arg(
        long,
        env = "EXECGO_RUNTIME_DISABLE_LINUX_SANDBOX",
        default_value_t = false
    )]
    pub disable_linux_sandbox: bool,
    #[arg(long, env = "EXECGO_RUNTIME_DISABLE_CGROUP", default_value_t = false)]
    pub disable_cgroup: bool,
    #[arg(long, env = "EXECGO_RUNTIME_CAPACITY_MEMORY_BYTES")]
    pub capacity_memory_bytes: Option<u64>,
    #[arg(long, env = "EXECGO_RUNTIME_CAPACITY_PIDS")]
    pub capacity_pids: Option<u64>,
}

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("input")
        .required(true)
        .args(["file", "json"])
))]
pub struct RemoteTaskArgs {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub server: String,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub json: Option<String>,
    #[arg(long, default_value = "500")]
    pub poll_interval_ms: u64,
    #[arg(long)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Args)]
pub struct StatusArgs {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub server: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Args)]
pub struct WaitArgs {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub server: String,
    pub task_id: String,
    #[arg(long)]
    pub timeout_ms: Option<u64>,
    #[arg(long, default_value = "500")]
    pub poll_interval_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct InternalShimArgs {
    #[arg(long)]
    pub database: PathBuf,
    #[arg(long)]
    pub data_dir: PathBuf,
    #[arg(long)]
    pub task_id: String,
    #[arg(long)]
    pub termination_grace_ms: u64,
    #[arg(long)]
    pub cgroup_root: PathBuf,
}

impl WaitArgs {
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }
}
