use std::{
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use reqwest::Client;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::time::sleep;

fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("local addr")
        .port()
}

struct TestServer {
    base_url: String,
    child: Child,
    _temp: TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let temp = TempDir::new().expect("tempdir");
        let port = find_free_port();
        let base_url = format!("http://127.0.0.1:{port}");
        let mut child = Command::new(env!("CARGO_BIN_EXE_execgo-runtime"));
        child
            .arg("serve")
            .arg("--listen-addr")
            .arg(format!("127.0.0.1:{port}"))
            .arg("--data-dir")
            .arg(temp.path())
            .arg("--termination-grace-ms")
            .arg("200")
            .arg("--dispatch-poll-interval-ms")
            .arg("100")
            .arg("--gc-interval-ms")
            .arg("10000")
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = child.spawn().expect("spawn server");
        let server = Self {
            base_url,
            child,
            _temp: temp,
        };
        server.wait_ready().await;
        server
    }

    async fn wait_ready(&self) {
        let client = Client::new();
        for _ in 0..100 {
            if let Ok(response) = client.get(format!("{}/readyz", self.base_url)).send().await {
                if response.status().is_success() {
                    return;
                }
            }
            sleep(Duration::from_millis(50)).await;
        }
        panic!("server did not become ready");
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn submit_task(server: &TestServer, payload: Value) -> Value {
    Client::new()
        .post(format!("{}/api/v1/tasks", server.base_url))
        .json(&payload)
        .send()
        .await
        .expect("submit request")
        .error_for_status()
        .expect("submit success")
        .json::<Value>()
        .await
        .expect("submit json")
}

async fn get_status(server: &TestServer, task_id: &str) -> Value {
    Client::new()
        .get(format!("{}/api/v1/tasks/{task_id}", server.base_url))
        .send()
        .await
        .expect("status request")
        .error_for_status()
        .expect("status success")
        .json::<Value>()
        .await
        .expect("status json")
}

async fn wait_terminal(server: &TestServer, task_id: &str) -> Value {
    for _ in 0..120 {
        let status = get_status(server, task_id).await;
        if matches!(
            status.get("status").and_then(Value::as_str),
            Some("success" | "failed" | "cancelled")
        ) {
            return status;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("task did not reach terminal state");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_task_runs_to_success_and_persists_artifacts() {
    let server = TestServer::start().await;
    let submitted = submit_task(
        &server,
        json!({
            "execution": {
                "kind": "command",
                "program": "/bin/sh",
                "args": ["-c", "echo hello-runtime; echo problem >&2"]
            }
        }),
    )
    .await;
    let task_id = submitted["task_id"].as_str().expect("task id");

    let terminal = wait_terminal(&server, task_id).await;
    assert_eq!(terminal["status"], "success");
    assert!(terminal["stdout"]
        .as_str()
        .unwrap_or_default()
        .contains("hello-runtime"));
    assert!(terminal["stderr"]
        .as_str()
        .unwrap_or_default()
        .contains("problem"));

    let result_path = terminal["artifacts"]["result_path"]
        .as_str()
        .expect("result path");
    assert!(Path::new(result_path).exists());

    let events = Client::new()
        .get(format!("{}/api/v1/tasks/{task_id}/events", server.base_url))
        .send()
        .await
        .expect("events request")
        .error_for_status()
        .expect("events success")
        .json::<Value>()
        .await
        .expect("events json");
    let event_types: Vec<_> = events
        .as_array()
        .expect("events array")
        .iter()
        .filter_map(|event| event.get("event_type").and_then(Value::as_str))
        .collect();
    assert!(event_types.contains(&"submitted"));
    assert!(event_types.contains(&"accepted"));
    assert!(event_types.contains(&"started"));
    assert!(event_types.contains(&"finished"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_run_and_kill_flow_work() {
    let server = TestServer::start().await;
    let payload = json!({
        "execution": {
            "kind": "command",
            "program": "/bin/sh",
            "args": ["-c", "sleep 5"]
        }
    });

    let submit_output = Command::new(env!("CARGO_BIN_EXE_execgo-runtime"))
        .arg("submit")
        .arg("--server")
        .arg(&server.base_url)
        .arg("--json")
        .arg(payload.to_string())
        .output()
        .expect("cli submit");
    assert!(submit_output.status.success());
    let submitted: Value =
        serde_json::from_slice(&submit_output.stdout).expect("submit stdout json");
    let task_id = submitted["task_id"].as_str().expect("task id");

    for _ in 0..40 {
        let status = get_status(&server, task_id).await;
        if status["status"] == "running" {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    let kill_output = Command::new(env!("CARGO_BIN_EXE_execgo-runtime"))
        .arg("kill")
        .arg("--server")
        .arg(&server.base_url)
        .arg(task_id)
        .output()
        .expect("cli kill");
    assert!(kill_output.status.success());

    let terminal = wait_terminal(&server, task_id).await;
    assert_eq!(terminal["status"], "cancelled");

    let run_output = Command::new(env!("CARGO_BIN_EXE_execgo-runtime"))
        .arg("run")
        .arg("--server")
        .arg(&server.base_url)
        .arg("--json")
        .arg(
            json!({
                "execution": {
                    "kind": "command",
                    "program": "/bin/sh",
                    "args": ["-c", "echo cli-run"]
                }
            })
            .to_string(),
        )
        .output()
        .expect("cli run");
    assert!(run_output.status.success());
    let run_status: Value = serde_json::from_slice(&run_output.stdout).expect("run stdout json");
    assert_eq!(run_status["status"], "success");
    assert!(run_status["stdout"]
        .as_str()
        .unwrap_or_default()
        .contains("cli-run"));
}
