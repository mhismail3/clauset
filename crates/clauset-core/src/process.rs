//! Process management for Claude CLI.

use crate::{ClausetError, OutputParser, Result};
use clauset_types::{ClaudeEvent, SessionMode};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Events emitted by managed processes.
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// Claude event from stream-json mode.
    Claude(ClaudeEvent),
    /// Raw terminal output from PTY mode.
    TerminalOutput { session_id: Uuid, data: Vec<u8> },
    /// Process has exited.
    Exited { session_id: Uuid, exit_code: Option<i32> },
    /// Error occurred.
    Error { session_id: Uuid, message: String },
}

/// Options for spawning a Claude process.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub session_id: Uuid,
    pub claude_session_id: Uuid,
    pub project_path: PathBuf,
    pub prompt: String,
    pub model: Option<String>,
    pub mode: SessionMode,
    pub resume: bool,
}

/// Manages Claude CLI processes.
pub struct ProcessManager {
    claude_path: PathBuf,
    processes: Arc<RwLock<HashMap<Uuid, ManagedProcess>>>,
}

enum ManagedProcess {
    StreamJson {
        _handle: tokio::task::JoinHandle<()>,
        stdin_tx: mpsc::Sender<String>,
    },
    Terminal {
        _handle: std::thread::JoinHandle<()>,
        writer: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
        master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    },
}

impl ProcessManager {
    /// Create a new process manager.
    pub fn new(claude_path: PathBuf) -> Self {
        Self {
            claude_path,
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Spawn a new Claude process.
    pub async fn spawn(
        &self,
        opts: SpawnOptions,
        event_tx: broadcast::Sender<ProcessEvent>,
    ) -> Result<()> {
        match opts.mode {
            SessionMode::StreamJson => self.spawn_stream_json(opts, event_tx).await,
            SessionMode::Terminal => self.spawn_terminal(opts, event_tx).await,
        }
    }

    async fn spawn_stream_json(
        &self,
        opts: SpawnOptions,
        event_tx: broadcast::Sender<ProcessEvent>,
    ) -> Result<()> {
        // Validate project path exists
        if !opts.project_path.exists() {
            error!("Project path does not exist: {:?}", opts.project_path);
            return Err(ClausetError::ProcessSpawnFailed(format!(
                "Project path does not exist: {:?}",
                opts.project_path
            )));
        }

        // Validate Claude binary exists
        if !self.claude_path.exists() {
            error!("Claude binary not found at: {:?}", self.claude_path);
            return Err(ClausetError::ProcessSpawnFailed(format!(
                "Claude binary not found at: {:?}",
                self.claude_path
            )));
        }

        let mut cmd = tokio::process::Command::new(&self.claude_path);

        // Build arguments - use print mode with streaming JSON output
        // --verbose is REQUIRED when using -p with --output-format=stream-json
        cmd.args(["-p", "--verbose", "--output-format", "stream-json"]);

        if opts.resume {
            cmd.args(["--resume", &opts.claude_session_id.to_string()]);
        }

        if let Some(model) = &opts.model {
            cmd.args(["--model", model]);
        }

        // Add the prompt as the final argument
        if !opts.prompt.is_empty() {
            cmd.arg(&opts.prompt);
        }

        cmd.current_dir(&opts.project_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        info!(
            "Spawning Claude process: {:?} in {:?}",
            self.claude_path, opts.project_path
        );
        debug!("Prompt: {}", opts.prompt);

        let mut child = cmd.spawn().map_err(|e| {
            error!("Failed to spawn Claude process: {}", e);
            ClausetError::ProcessSpawnFailed(format!("Failed to spawn: {}", e))
        })?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take();
        let session_id = opts.session_id;

        // Channel for sending input to Claude
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);

        // Stdin writer task
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                while let Some(input) = stdin_rx.recv().await {
                    if stdin.write_all(input.as_bytes()).await.is_err() {
                        break;
                    }
                    if stdin.write_all(b"\n").await.is_err() {
                        break;
                    }
                    if stdin.flush().await.is_err() {
                        break;
                    }
                }
            });
        }

        // Stderr reader task - log errors
        if let Some(stderr) = stderr {
            let sid = session_id;
            let tx_err = event_tx.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    warn!("Claude stderr [{}]: {}", sid, line);
                    // Send error events to frontend
                    let _ = tx_err.send(ProcessEvent::Error {
                        session_id: sid,
                        message: line,
                    });
                }
            });
        }

        // Stdout reader task
        let tx = event_tx.clone();
        let handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut parser = OutputParser::new();

            while let Ok(Some(line)) = lines.next_line().await {
                debug!("Claude stdout: {}", line);
                if let Ok(Some(event)) = parser.parse_line(&line) {
                    let _ = tx.send(ProcessEvent::Claude(event));
                }
            }

            // Wait for process to exit
            let exit_code = child.wait().await.ok().and_then(|s| s.code());
            info!("Claude process exited with code: {:?}", exit_code);
            let _ = tx.send(ProcessEvent::Exited { session_id, exit_code });
        });

        self.processes.write().await.insert(
            opts.session_id,
            ManagedProcess::StreamJson {
                _handle: handle,
                stdin_tx,
            },
        );

        info!("Claude process spawned successfully for session {}", session_id);
        Ok(())
    }

    async fn spawn_terminal(
        &self,
        opts: SpawnOptions,
        event_tx: broadcast::Sender<ProcessEvent>,
    ) -> Result<()> {
        info!(
            "Spawning Claude terminal session in {:?}",
            opts.project_path
        );

        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| ClausetError::PtyError(e.to_string()))?;

        let mut cmd = CommandBuilder::new(&self.claude_path);

        // If resuming, use --resume with the session ID
        // Otherwise, just start Claude normally (it creates its own session)
        if opts.resume {
            cmd.args(["--resume", &opts.claude_session_id.to_string()]);
        }

        cmd.cwd(&opts.project_path);

        info!("Spawning Claude with session ID: {}", opts.claude_session_id);

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| ClausetError::ProcessSpawnFailed(e.to_string()))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| ClausetError::PtyError(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| ClausetError::PtyError(e.to_string()))?;

        let writer = Arc::new(std::sync::Mutex::new(writer));
        let writer_clone = writer.clone();

        let session_id = opts.session_id;
        let tx = event_tx.clone();
        let initial_prompt = opts.prompt.clone();

        // Reader thread (PTY reading is blocking)
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            info!("PTY reader thread started for session {}", session_id);

            // Track if we've sent the initial prompt
            let mut prompt_sent = initial_prompt.is_empty();
            let mut total_bytes = 0usize;

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        info!("PTY reader got EOF for session {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        total_bytes += n;
                        let output_str = String::from_utf8_lossy(&buf[..n]);
                        info!("PTY output ({} bytes, total {}): {}", n, total_bytes,
                              output_str.chars().take(200).collect::<String>());

                        let _ = tx.send(ProcessEvent::TerminalOutput {
                            session_id,
                            data: buf[..n].to_vec(),
                        });

                        // Send initial prompt once after receiving some output
                        // (Claude shows its prompt after startup)
                        if !prompt_sent && total_bytes > 100 {
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            if let Ok(mut w) = writer_clone.lock() {
                                let prompt_with_newline = format!("{}\n", initial_prompt);
                                let _ = w.write_all(prompt_with_newline.as_bytes());
                                let _ = w.flush();
                                info!("Sent initial prompt to Claude: {}", initial_prompt);
                            }
                            prompt_sent = true;
                        }
                    }
                    Err(e) => {
                        error!("PTY read error for session {}: {}", session_id, e);
                        break;
                    }
                }
            }

            // Wait for process to exit
            let exit_code = child.wait().ok().map(|s| s.exit_code() as i32);
            info!("Claude process exited with code {:?} for session {}", exit_code, session_id);
            let _ = tx.send(ProcessEvent::Exited {
                session_id,
                exit_code,
            });
        });

        self.processes.write().await.insert(
            opts.session_id,
            ManagedProcess::Terminal {
                _handle: handle,
                writer,
                master: Arc::new(std::sync::Mutex::new(pair.master)),
            },
        );

        info!("Claude terminal session spawned for session {}", session_id);
        Ok(())
    }

    /// Send input to a session (works for both StreamJson and Terminal modes).
    pub async fn send_input(&self, session_id: Uuid, input: &str) -> Result<()> {
        let processes = self.processes.read().await;
        match processes.get(&session_id) {
            Some(ManagedProcess::StreamJson { stdin_tx, .. }) => {
                stdin_tx
                    .send(input.to_string())
                    .await
                    .map_err(|_| ClausetError::ChannelSendError)?;
            }
            Some(ManagedProcess::Terminal { writer, .. }) => {
                // For terminal mode, send input followed by newline
                let mut writer = writer.lock().unwrap();
                let input_with_newline = format!("{}\n", input);
                writer
                    .write_all(input_with_newline.as_bytes())
                    .map_err(|e| ClausetError::IoError(e))?;
                writer.flush().map_err(|e| ClausetError::IoError(e))?;
            }
            None => {}
        }
        Ok(())
    }

    /// Send raw terminal input to a PTY session.
    pub async fn send_terminal_input(&self, session_id: Uuid, data: &[u8]) -> Result<()> {
        let processes = self.processes.read().await;
        if let Some(ManagedProcess::Terminal { writer, .. }) = processes.get(&session_id) {
            let mut writer = writer.lock().unwrap();
            writer
                .write_all(data)
                .map_err(|e| ClausetError::IoError(e))?;
            writer.flush().map_err(|e| ClausetError::IoError(e))?;
        }
        Ok(())
    }

    /// Resize a PTY terminal.
    pub async fn resize_terminal(&self, session_id: Uuid, rows: u16, cols: u16) -> Result<()> {
        let processes = self.processes.read().await;
        if let Some(ManagedProcess::Terminal { master, .. }) = processes.get(&session_id) {
            let master = master.lock().unwrap();
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| ClausetError::PtyError(e.to_string()))?;
        }
        Ok(())
    }

    /// Terminate a process.
    pub async fn terminate(&self, session_id: Uuid) -> Result<()> {
        self.processes.write().await.remove(&session_id);
        Ok(())
    }

    /// Check if a session has an active process.
    pub async fn is_active(&self, session_id: Uuid) -> bool {
        self.processes.read().await.contains_key(&session_id)
    }
}
