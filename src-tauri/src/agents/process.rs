use super::StartContext;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::File,
    io::Write,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
};
#[derive(Debug)]
pub enum Output {
    Message(Value),
    Closed,
}
pub struct Process {
    child: Child,
    stdin: mpsc::Sender<Value>,
    pub output: mpsc::Receiver<Output>,
    pgid: Option<i32>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    trace: Option<Arc<Mutex<File>>>,
    sensitive: Arc<AtomicBool>,
}
impl Process {
    pub fn spawn(context: &StartContext, args: &[String]) -> Result<Self, String> {
        crate::privacy::private_dir(&context.log_dir).map_err(|e| e.to_string())?;
        let trace = if std::env::var("CODE_UI_TRACE").as_deref() == Ok("1") {
            Some(Arc::new(Mutex::new(
                crate::privacy::append_file(&context.log_dir.join(format!(
                    "{}.{}.trace.jsonl",
                    context.chat_id,
                    context.settings.agent.as_str()
                )))
                .map_err(|e| e.to_string())?,
            )))
        } else {
            None
        };
        let sensitive = Arc::new(AtomicBool::new(false));
        let mut child = Command::new(&context.binary)
            .args(args)
            .current_dir(&context.cwd)
            .env_clear()
            .envs(&context.env)
            .process_group(0)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let pgid = child.id().ok_or("Child has no process id")? as i32;
        let mut stdin = child.stdin.take().ok_or("Child has no stdin")?;
        let stdout = child.stdout.take().ok_or("Child has no stdout")?;
        let stderr_pipe = child.stderr.take().ok_or("Child has no stderr")?;
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, output) = mpsc::channel(256);
        let (writer, mut inputs) = mpsc::channel::<Value>(32);
        let writer_events = tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = inputs.recv().await {
                let mut bytes = serde_json::to_vec(&msg).expect("JSON value serialization failed");
                bytes.push(b'\n');
                if stdin.write_all(&bytes).await.is_err() || stdin.flush().await.is_err() {
                    let _ = writer_events.send(Output::Closed).await;
                    break;
                }
            }
        });
        let recorder = trace.clone();
        let stdout_sensitive = sensitive.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut bytes = Vec::new();
            loop {
                bytes.clear();
                match reader.read_until(b'\n', &mut bytes).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => match serde_json::from_slice::<Value>(&bytes) {
                        Ok(msg) => {
                            record(&recorder, &stdout_sensitive, "out", &msg);
                            if tx.send(Output::Message(msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => tracing::warn!("Skipped non-JSON agent stdout"),
                    },
                }
            }
            let _ = tx.send(Output::Closed).await;
        });
        let tail = stderr.clone();
        let log = context.log_dir.join(format!(
            "{}.{}.stderr.log",
            context.chat_id,
            context.settings.agent.as_str()
        ));
        let stderr_sensitive = sensitive.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr_pipe).lines();
            let mut file = crate::privacy::append_file(&log).ok();
            while let Ok(Some(line)) = lines.next_line().await {
                if stderr_sensitive.load(Ordering::Relaxed) {
                    continue;
                }
                if let Some(file) = &mut file {
                    let _ = writeln!(file, "{line}");
                }
                if let Ok(mut tail) = tail.lock() {
                    tail.push_back(line);
                    if tail.len() > 200 {
                        tail.pop_front();
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin: writer,
            output,
            pgid: Some(pgid),
            stderr,
            trace,
            sensitive,
        })
    }
    pub async fn write(&mut self, msg: &Value) -> Result<(), String> {
        record(&self.trace, &self.sensitive, "in", msg);
        self.stdin
            .send(msg.clone())
            .await
            .map_err(|_| "Agent stdin closed".into())
    }
    pub fn stderr_tail(&self) -> String {
        if self.sensitive.load(Ordering::Relaxed) {
            return "Diagnostics hidden after a secret-input request.".into();
        }
        self.stderr
            .lock()
            .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }
    pub async fn terminate(&mut self) {
        let Some(pgid) = self.pgid.take() else {
            return;
        };
        // Each agent and its subprocesses have their own group.
        unsafe {
            libc::killpg(pgid, libc::SIGTERM);
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await;
        // Also remove surviving descendants when the group leader exited first.
        unsafe {
            libc::killpg(pgid, libc::SIGKILL);
        }
        let _ = self.child.wait().await;
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(pgid) = self.pgid.take() {
            unsafe {
                libc::killpg(pgid, libc::SIGTERM);
            }
        }
    }
}

fn record(trace: &Option<Arc<Mutex<File>>>, sensitive: &AtomicBool, dir: &str, msg: &Value) {
    if crate::privacy::has_secret_input(msg) {
        sensitive.store(true, Ordering::Relaxed);
    }
    if sensitive.load(Ordering::Relaxed) {
        return;
    }
    if let Some(trace) = trace
        && let Ok(mut file) = trace.lock()
    {
        let _ = writeln!(
            file,
            "{}",
            json!({"dir":dir,"t":crate::db::now_ms(),"msg":crate::privacy::redact_trace(msg)})
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn termination_stops_the_fake_cli_and_its_descendant() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("code-ui-process-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let binary = root.join("fake-cli");
        std::fs::write(&binary, "#!/bin/sh\nsleep 120 &\nchild=$!\nprintf '{\"childPid\":%s}\\n' \"$child\"\nwait \"$child\"\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let context = StartContext {
            cwd: root.clone(),
            env: std::collections::HashMap::from([("PATH".into(), "/usr/bin:/bin".into())]),
            binary: binary.to_string_lossy().into_owned(),
            native_id: None,
            chat_id: "fake-process".into(),
            settings: super::super::TurnSettings::default(),
            log_dir: root.join("logs"),
        };
        let mut process = Process::spawn(&context, &[]).unwrap();
        let output = tokio::time::timeout(Duration::from_secs(3), process.output.recv())
            .await
            .unwrap()
            .unwrap();
        let Output::Message(message) = output else {
            panic!("fake CLI closed before reporting its child");
        };
        let child = message["childPid"].as_u64().unwrap();
        process.terminate().await;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let state = std::fs::read_to_string(format!("/proc/{child}/stat"));
                if state.is_err() || state.unwrap().split_once(") ").unwrap().1.starts_with('Z') {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("CLI descendant survived termination");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trace_stops_before_a_secret_prompt_and_never_records_its_response() {
        let dir = std::env::temp_dir().join(format!("code-ui-trace-{}", uuid::Uuid::new_v4()));
        crate::privacy::private_dir(&dir).unwrap();
        let path = dir.join("trace.jsonl");
        let trace = Some(Arc::new(Mutex::new(
            crate::privacy::append_file(&path).unwrap(),
        )));
        let sensitive = AtomicBool::new(false);
        record(
            &trace,
            &sensitive,
            "in",
            &json!({"result":{"answers":{"q":"ordinary-answer"}}}),
        );
        record(
            &trace,
            &sensitive,
            "out",
            &json!({"params":{"questions":[{"isSecret":true}]}}),
        );
        record(
            &trace,
            &sensitive,
            "in",
            &json!({"result":{"answers":{"q":"private-token"}}}),
        );
        record(
            &trace,
            &sensitive,
            "out",
            &json!({"text":"echo private-token"}),
        );
        let retained = std::fs::read_to_string(&path).unwrap();
        assert!(sensitive.load(Ordering::Relaxed));
        assert!(!retained.contains("ordinary-answer"));
        assert!(!retained.contains("private-token"));
        assert_eq!(retained.lines().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
