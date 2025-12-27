//! Process management for Claude CLI.

use crate::{ClausetError, OutputParser, Result};
use clauset_types::{ClaudeEvent, SessionMode};
use portable_pty::{native_pty_system, Child as PtyChild, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

/// Events emitted by managed processes.
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// Claude event from stream-json mode.
    Claude(ClaudeEvent),
    /// Raw terminal output from PTY mode.
    /// DEPRECATED: Use SequencedTerminalOutput for reliable streaming.
    TerminalOutput { session_id: Uuid, data: Vec<u8> },
    /// Sequenced terminal output for reliable streaming protocol.
    /// Each chunk has a monotonically increasing sequence number for ordering and gap detection.
    SequencedTerminalOutput {
        session_id: Uuid,
        /// Monotonically increasing sequence number (per session)
        seq: u64,
        /// Terminal data (raw bytes including ANSI codes)
        data: Vec<u8>,
        /// Timestamp when chunk was captured (ms since Unix epoch)
        timestamp: u64,
    },
    /// Process has exited.
    Exited { session_id: Uuid, exit_code: Option<i32> },
    /// Error occurred.
    Error { session_id: Uuid, message: String },
    /// Session activity updated (for dashboard).
    ActivityUpdate {
        session_id: Uuid,
        model: String,
        cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        context_percent: u8,
        current_activity: String,
        current_step: Option<String>,
        recent_actions: Vec<crate::buffer::RecentAction>,
    },
    /// Chat event for chat mode view.
    Chat(clauset_types::ChatEvent),
    /// New prompt indexed for Prompt Library.
    NewPrompt(clauset_types::PromptSummary),
    /// Interactive event for native UI rendering.
    Interactive(clauset_types::InteractiveEvent),
    /// Subagent (Task tool) started.
    SubagentStarted {
        session_id: Uuid,
        agent_id: String,
        agent_type: String,
    },
    /// Subagent (Task tool) stopped.
    SubagentStopped {
        session_id: Uuid,
        agent_id: String,
    },
    /// Subagent (Task tool) completed with details.
    SubagentCompleted {
        session_id: Uuid,
        agent_type: String,
        description: String,
        result: String,
    },
    /// Tool execution failed.
    ToolError {
        session_id: Uuid,
        tool_name: String,
        error: String,
        is_timeout: bool,
    },
    /// Context compaction starting.
    ContextCompacting {
        session_id: Uuid,
        trigger: String,
    },
    /// Permission request shown.
    PermissionRequest {
        session_id: Uuid,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    /// Context token update from hook data.
    ContextUpdate {
        session_id: Uuid,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
        context_window_size: u64,
    },
    /// Session mode changed (e.g., entered/exited Plan Mode).
    ModeChange {
        session_id: Uuid,
        mode: clauset_types::ChatMode,
    },
    /// TUI menu event for native UI rendering.
    /// Sent when a TUI selection menu is detected in terminal output.
    TuiMenu(clauset_types::TuiMenuEvent),
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
    /// URL for hooks to send events back to
    pub clauset_url: String,
}

/// Manages Claude CLI processes.
pub struct ProcessManager {
    claude_path: PathBuf,
    processes: Arc<RwLock<HashMap<Uuid, ManagedProcess>>>,
}

enum ManagedProcess {
    StreamJson {
        handle: tokio::task::JoinHandle<()>,
        stdin_tx: mpsc::Sender<String>,
    },
    Terminal {
        handle: std::thread::JoinHandle<()>,
        writer: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
        master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
        /// Signal to stop the reader thread
        shutdown: Arc<AtomicBool>,
        /// Child process for proper termination
        child: Arc<std::sync::Mutex<Box<dyn PtyChild + Send + Sync>>>,
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
            error!(target: "clauset::process", "Project path does not exist: {:?}", opts.project_path);
            return Err(ClausetError::ProcessSpawnFailed(format!(
                "Project path does not exist: {:?}",
                opts.project_path
            )));
        }

        // Validate Claude binary exists
        if !self.claude_path.exists() {
            error!(target: "clauset::process", "Claude binary not found at: {:?}", self.claude_path);
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
            target: "clauset::process",
            "Spawning Claude process: {:?} in {:?}",
            self.claude_path, opts.project_path
        );
        debug!(target: "clauset::process", "Prompt: {}", opts.prompt);

        let mut child = cmd.spawn().map_err(|e| {
            error!(target: "clauset::process", "Failed to spawn Claude process: {}", e);
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
                    warn!(target: "clauset::process", "Claude stderr [{}]: {}", sid, line);
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
                debug!(target: "clauset::process", "Claude stdout: {}", line);
                if let Ok(Some(event)) = parser.parse_line(&line) {
                    let _ = tx.send(ProcessEvent::Claude(event));
                }
            }

            // Wait for process to exit
            let exit_code = child.wait().await.ok().and_then(|s| s.code());
            info!(target: "clauset::process", "Claude process exited with code: {:?}", exit_code);
            let _ = tx.send(ProcessEvent::Exited { session_id, exit_code });
        });

        self.processes.write().await.insert(
            opts.session_id,
            ManagedProcess::StreamJson {
                handle,
                stdin_tx,
            },
        );

        info!(target: "clauset::process", "Claude process spawned successfully for session {}", session_id);
        Ok(())
    }

    async fn spawn_terminal(
        &self,
        opts: SpawnOptions,
        event_tx: broadcast::Sender<ProcessEvent>,
    ) -> Result<()> {
        info!(
            target: "clauset::process",
            "Spawning Claude terminal session in {:?}",
            opts.project_path
        );

        let pty_system = native_pty_system();

        // Start with VERY conservative default that fits even small phone screens
        // iPhone in portrait needs ~45 cols max, so use 40 for safety
        // Frontend will send actual resize immediately after connecting
        let initial_cols = 40;
        let initial_rows = 24;
        debug!(target: "clauset::process", "Creating PTY with initial size: {}x{}", initial_cols, initial_rows);

        let pair = pty_system
            .openpty(PtySize {
                rows: initial_rows,
                cols: initial_cols,
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

        // Pass the selected model to Claude CLI
        if let Some(model) = &opts.model {
            cmd.args(["--model", model]);
            debug!(target: "clauset::process", "Using model: {}", model);
        }

        // Set environment variables for Clauset hooks integration
        // These allow the hook script to identify which session and where to send events
        cmd.env("CLAUSET_SESSION_ID", opts.session_id.to_string());
        cmd.env("CLAUSET_URL", &opts.clauset_url);

        cmd.cwd(&opts.project_path);

        debug!(target: "clauset::process", "Spawning Claude with session ID: {}", opts.claude_session_id);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| ClausetError::ProcessSpawnFailed(e.to_string()))?;

        // Wrap child for shared access (needed for termination)
        let child: Arc<std::sync::Mutex<Box<dyn PtyChild + Send + Sync>>> =
            Arc::new(std::sync::Mutex::new(child));

        // Create shutdown signal for clean termination
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_thread = shutdown.clone();

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
            debug!(target: "clauset::process", "PTY reader thread started for session {}", session_id);

            // Track if we've sent the initial prompt
            let mut prompt_sent = initial_prompt.is_empty();
            let mut total_bytes = 0usize;
            let mut accumulated_output = String::new();

            loop {
                // Check shutdown signal before each read
                if shutdown_for_thread.load(Ordering::SeqCst) {
                    debug!(target: "clauset::process", "PTY reader thread received shutdown signal for session {}", session_id);
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => {
                        debug!(target: "clauset::process", "PTY reader got EOF for session {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        total_bytes += n;
                        let output_str = String::from_utf8_lossy(&buf[..n]);
                        trace!(target: "clauset::process", "PTY output ({} bytes, total {}): {}", n, total_bytes,
                              output_str.chars().take(200).collect::<String>());

                        let _ = tx.send(ProcessEvent::TerminalOutput {
                            session_id,
                            data: buf[..n].to_vec(),
                        });

                        // Accumulate output to detect Claude's ready prompt
                        if !prompt_sent {
                            accumulated_output.push_str(&output_str);

                            // Claude Code shows a prompt indicator when ready for input
                            // Look for common prompt patterns: "> ", "❯ ", or after seeing
                            // enough output indicating Claude has started
                            let ready = accumulated_output.contains("> ")
                                || accumulated_output.contains("❯ ")
                                || accumulated_output.contains("claude")
                                || total_bytes > 500;

                            if ready {
                                // Wait a bit longer to ensure Claude is fully ready
                                std::thread::sleep(Duration::from_millis(800));
                                if let Ok(mut w) = writer_clone.lock() {
                                    // Trim any trailing whitespace/newlines from prompt
                                    let prompt_text = initial_prompt.trim();

                                    // Send the prompt text
                                    let _ = w.write_all(prompt_text.as_bytes());
                                    let _ = w.flush();

                                    // Delay between text and Enter - TUI needs Enter as separate event
                                    std::thread::sleep(Duration::from_millis(100));

                                    // Send Enter key to execute (carriage return)
                                    let _ = w.write_all(b"\r");
                                    let _ = w.flush();
                                    debug!(target: "clauset::process", "Sent initial prompt to Claude: {}", prompt_text);
                                }
                                prompt_sent = true;
                            }
                        }
                    }
                    Err(e) => {
                        // Don't log error if we're shutting down (expected)
                        if !shutdown_for_thread.load(Ordering::SeqCst) {
                            error!(target: "clauset::process", "PTY read error for session {}: {}", session_id, e);
                        }
                        break;
                    }
                }
            }

            // Don't wait for process here - that's handled by terminate()
            // Just signal that the reader thread is done
            debug!(target: "clauset::process", "PTY reader thread exiting for session {}", session_id);
        });

        self.processes.write().await.insert(
            opts.session_id,
            ManagedProcess::Terminal {
                handle,
                writer,
                master: Arc::new(std::sync::Mutex::new(pair.master)),
                shutdown,
                child,
            },
        );

        info!(target: "clauset::process", "Claude terminal session spawned for session {}", session_id);
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
                // For terminal mode, send input followed by carriage return.
                // Important: Send text and Enter key separately with a delay,
                // Claude Code's TUI needs the Enter key to arrive as a distinct input event.
                let mut writer = writer.lock().unwrap();

                // Trim any trailing whitespace/newlines from input
                let input_text = input.trim();

                // First, write the text content
                writer
                    .write_all(input_text.as_bytes())
                    .map_err(|e| ClausetError::IoError(e))?;
                writer.flush().map_err(|e| ClausetError::IoError(e))?;

                // Delay to let the TUI process the text before Enter
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Now send Enter key (carriage return) to execute
                writer
                    .write_all(b"\r")
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
        debug!(target: "clauset::process", "Resizing PTY for session {} to {}x{}", session_id, cols, rows);
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
            debug!(target: "clauset::process", "PTY resize successful for session {}", session_id);
        } else {
            warn!(target: "clauset::process", "No terminal process found for session {} during resize", session_id);
        }
        Ok(())
    }

    /// Terminate a process gracefully.
    ///
    /// For Terminal mode: send "exit" → wait → close PTY → join thread → kill process group
    /// For StreamJson mode: abort the tokio task
    pub async fn terminate(&self, session_id: Uuid) -> Result<()> {
        let process = self.processes.write().await.remove(&session_id);

        if let Some(process) = process {
            match process {
                ManagedProcess::Terminal {
                    handle,
                    shutdown,
                    child,
                    master,
                    writer,
                } => {
                    info!(target: "clauset::process", "Terminating terminal session {}", session_id);

                    // 1. Get child PID for later cleanup
                    let pid = if let Ok(c) = child.lock() {
                        c.process_id()
                    } else {
                        None
                    };

                    // 2. Try graceful exit by sending "exit" command to Claude
                    if let Ok(mut w) = writer.lock() {
                        info!(target: "clauset::process", "Sending 'exit' command to Claude for session {}", session_id);
                        // Send Ctrl+C first to cancel any pending operation, then exit
                        let _ = w.write_all(b"\x03");
                        let _ = w.flush();
                        let _ = w.write_all(b"exit\r");
                        let _ = w.flush();
                    }

                    // 3. Wait up to 2 seconds for Claude to exit gracefully
                    let graceful_timeout = Duration::from_secs(2);
                    let start = std::time::Instant::now();
                    let mut exited_gracefully = false;

                    while start.elapsed() < graceful_timeout {
                        if handle.is_finished() {
                            info!(target: "clauset::process", "Claude exited gracefully for session {}", session_id);
                            exited_gracefully = true;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }

                    // 4. Signal the reader thread to stop
                    shutdown.store(true, Ordering::SeqCst);

                    // 5. Close PTY master and writer to unblock the reader thread
                    // The reader thread is blocked on reader.read(), which won't return
                    // until the PTY master is closed
                    drop(writer);
                    drop(master);
                    if !exited_gracefully {
                        info!(target: "clauset::process", "PTY closed (forcing exit) for session {}", session_id);
                    }

                    // 6. Join the thread using spawn_blocking (should complete quickly now)
                    let join_result = tokio::task::spawn_blocking(move || handle.join()).await;

                    match join_result {
                        Ok(Ok(())) => {
                            debug!(target: "clauset::process", "Reader thread joined successfully for session {}", session_id);
                        }
                        Ok(Err(e)) => {
                            warn!(target: "clauset::process", "Reader thread panicked for session {}: {:?}", session_id, e);
                        }
                        Err(e) => {
                            warn!(target: "clauset::process", "Failed to join reader thread for session {}: {:?}", session_id, e);
                        }
                    }

                    // 7. Kill the process group if it didn't exit gracefully
                    // Claude CLI spawns Node.js subprocesses that need to be killed too
                    #[cfg(unix)]
                    if !exited_gracefully {
                        if let Some(pid) = pid {
                            info!(target: "clauset::process", "Sending SIGKILL to process group {} for session {}", pid, session_id);
                            unsafe {
                                // Negative PID kills the entire process group
                                libc::kill(-(pid as i32), libc::SIGKILL);
                            }
                        }
                    }

                    #[cfg(not(unix))]
                    {
                        let _ = pid;
                        let _ = &child;
                        let _ = exited_gracefully;
                    }

                    // 8. Reap the zombie process
                    if let Ok(mut c) = child.lock() {
                        let _ = c.try_wait();
                    }

                    info!(target: "clauset::process", "Terminal session {} terminated", session_id);
                }
                ManagedProcess::StreamJson { handle, .. } => {
                    info!(target: "clauset::process", "Terminating stream-json session {}", session_id);
                    handle.abort();
                    info!(target: "clauset::process", "Stream-json session {} terminated", session_id);
                }
            }
        }

        Ok(())
    }

    /// Check if a session has an active process.
    pub async fn is_active(&self, session_id: Uuid) -> bool {
        self.processes.read().await.contains_key(&session_id)
    }
}
