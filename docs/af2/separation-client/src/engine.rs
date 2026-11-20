// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) The Audiform contributors.

//! A running engine: started once, a model loaded once, any number of
//! files separated with it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::protocol::{Output, Reply, Request, PROTOCOL};
use crate::Error;

/// How long the engine may take to answer the handshake.
const HELLO_WAIT: Duration = Duration::from_secs(20);

/// A model the engine has loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub model: String,
    pub device: String,
    pub stems: usize,
    pub channels: usize,
    pub sample_rate: u32,
    /// How long loading took (near zero when it was loaded already).
    pub seconds: f64,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq)]
pub enum RunEnd {
    Done { seconds: f64 },
    Cancelled,
}

pub struct Engine {
    child: Child,
    input: ChildStdin,
    replies: Receiver<Reply>,
    /// The engine's version, from its handshake.
    pub version: String,
    loaded: Option<Loaded>,
    next_job: u64,
}

impl Engine {
    /// Starts the engine at `exe` and shakes hands. `host_version` is
    /// Audiform's own version (2.0.0 or later).
    pub fn start(exe: &Path, host_version: &str) -> Result<Engine, Error> {
        let mut cmd = Command::new(exe);
        cmd.arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Stopped(format!("it could not be started: {e}")))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Error::Stopped("no input".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Stopped("no output".into()))?;
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(r) = serde_json::from_str::<Reply>(&line) {
                    if tx.send(r).is_err() {
                        return;
                    }
                }
            }
        });
        let mut e = Engine {
            child,
            input,
            replies,
            version: String::new(),
            loaded: None,
            next_job: 1,
        };
        e.send(&Request::Hello {
            protocol: PROTOCOL,
            host: "Audiform".into(),
            host_version: host_version.into(),
        })?;
        match e.replies.recv_timeout(HELLO_WAIT) {
            Ok(Reply::Ready { engine, protocol }) if protocol == PROTOCOL => {
                e.version = engine;
                Ok(e)
            }
            Ok(Reply::Error { message, .. }) => Err(Error::Engine(message)),
            Ok(other) => Err(Error::Engine(format!("unexpected answer: {other:?}"))),
            Err(_) => Err(Error::Stopped("it did not answer".into())),
        }
    }

    fn send(&mut self, r: &Request) -> Result<(), Error> {
        let line = serde_json::to_string(r).map_err(|e| Error::Engine(e.to_string()))?;
        writeln!(self.input, "{line}")
            .and_then(|_| self.input.flush())
            .map_err(|e| Error::Stopped(e.to_string()))
    }

    fn next(&self) -> Result<Reply, Error> {
        self.replies
            .recv()
            .map_err(|_| Error::Stopped("it closed".into()))
    }

    /// The model loaded now, if any.
    pub fn loaded(&self) -> Option<&Loaded> {
        self.loaded.as_ref()
    }

    /// Loads a model (`spec` as the model list keeps it) on the graphics
    /// card (`gpu`) or the processor. Instant when it is loaded already.
    pub fn load(
        &mut self,
        model: &str,
        spec: &Value,
        checkpoint: &Path,
        gpu: bool,
    ) -> Result<Loaded, Error> {
        self.send(&Request::Load {
            model: model.into(),
            spec: spec.clone(),
            checkpoint: checkpoint.to_path_buf(),
            device: if gpu { "gpu" } else { "cpu" }.into(),
        })?;
        loop {
            match self.next()? {
                Reply::Loaded {
                    model,
                    device,
                    stems,
                    channels,
                    sample_rate,
                    seconds,
                } => {
                    let l = Loaded {
                        model,
                        device,
                        stems,
                        channels,
                        sample_rate,
                        seconds,
                    };
                    self.loaded = Some(l.clone());
                    return Ok(l);
                }
                Reply::Error { gpu, message, .. } => {
                    self.loaded = None;
                    return Err(if gpu {
                        Error::Gpu(message)
                    } else {
                        Error::Engine(message)
                    });
                }
                _ => {}
            }
        }
    }

    /// Separates `input` (a WAV at the loaded model's sample rate) into
    /// `outputs`. `progress` gets 0..1; setting `cancel` stops the run
    /// between chunks (it is not reset here).
    pub fn run(
        &mut self,
        input: &Path,
        outputs: &[Output],
        overlap: Option<usize>,
        mut progress: impl FnMut(f32),
        cancel: &AtomicBool,
    ) -> Result<RunEnd, Error> {
        let job = self.next_job;
        self.next_job += 1;
        self.send(&Request::Run {
            job,
            input: input.to_path_buf(),
            outputs: outputs.to_vec(),
            overlap,
        })?;
        let mut asked = false;
        loop {
            if cancel.load(Ordering::Relaxed) && !asked {
                asked = true;
                self.send(&Request::Cancel { job })?;
            }
            match self.replies.recv_timeout(Duration::from_millis(100)) {
                Ok(Reply::Progress { job: j, done }) if j == job => progress(done),
                Ok(Reply::Done { job: j, seconds }) if j == job => {
                    return Ok(RunEnd::Done { seconds })
                }
                Ok(Reply::Cancelled { job: j }) if j == job => return Ok(RunEnd::Cancelled),
                Ok(Reply::Error {
                    job: j,
                    gpu,
                    message,
                }) if j.is_none_or(|j| j == job) => {
                    if gpu {
                        // The engine drops a model that failed on the graphics card.
                        self.loaded = None;
                        return Err(Error::Gpu(message));
                    }
                    return Err(Error::Engine(message));
                }
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.loaded = None;
                    return Err(Error::Stopped("it closed during the run".into()));
                }
            }
        }
    }

    /// Frees the model's memory (the engine keeps running).
    pub fn unload(&mut self) -> Result<(), Error> {
        self.send(&Request::Unload)?;
        while !matches!(self.next()?, Reply::Unloaded) {}
        self.loaded = None;
        Ok(())
    }

    /// Whether the engine process is still there.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.send(&Request::Quit);
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Where Audiform 2 keeps the engine: `engine2` under its separation folder.
pub fn engine_dir(separation_dir: &Path) -> PathBuf {
    separation_dir.join("engine2")
}
