//! Owned native test processes with byte-preserving output and deadlines.
use std::io::Read;
use std::process::{Child, ChildStdin, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

/// How long a test lets a program run before it stops the program and fails:
/// far past any test program's time, so it stops a program that never
/// finishes.
pub(crate) const PROGRAM_DEADLINE: Duration = Duration::from_secs(60);

/// A test owns its process group, continuously drains both output channels,
/// and reaps it on success, timeout or panic. This bounds native test execution,
/// not any source-language proof or acceptance decision.
pub struct ProgramChild {
    child: Child,
    output: Option<std::thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    errors: Option<std::thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    deadline: Instant,
}

impl ProgramChild {
    pub(crate) fn spawn(command: &mut Command) -> std::io::Result<Self> {
        Self::spawn_with_limit(command, PROGRAM_DEADLINE)
    }

    pub(crate) fn spawn_with_limit(
        command: &mut Command,
        limit: Duration,
    ) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
        }
        let mut child = command.spawn()?;
        fn drain(
            reader: impl Read + Send + 'static,
        ) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
            std::thread::spawn(move || {
                let mut reader = reader;
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes)?;
                Ok(bytes)
            })
        }
        Ok(Self {
            output: child.stdout.take().map(drain),
            errors: child.stderr.take().map(drain),
            child,
            deadline: Instant::now() + limit,
        })
    }

    /// The child's standard input, when its command piped one.
    pub(crate) fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    /// The operating system's identifier of the running process.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_none() && Instant::now() >= self.deadline {
            self.terminate();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "native test child exceeded its deadline",
            ));
        }
        Ok(status)
    }

    fn terminate(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        #[cfg(unix)]
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", self.child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        #[cfg(windows)]
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &self.child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn wait_with_output(mut self) -> std::io::Result<Output> {
        let status = loop {
            if let Some(status) = self.try_wait()? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        fn collected(
            thread: Option<std::thread::JoinHandle<std::io::Result<Vec<u8>>>>,
        ) -> std::io::Result<Vec<u8>> {
            match thread {
                None => Ok(Vec::new()),
                Some(thread) => thread.join().expect("output reader thread"),
            }
        }
        Ok(Output {
            status,
            stdout: collected(self.output.take())?,
            stderr: collected(self.errors.take())?,
        })
    }
}

impl Drop for ProgramChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Runs `command` as `Command::output` does, with no standard input and both
/// outputs captured, as an owned process that is stopped once `limit` has
/// passed and then answers `TimedOut`.
pub(crate) fn output_within(command: &mut Command, limit: Duration) -> std::io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    ProgramChild::spawn_with_limit(command, limit)?.wait_with_output()
}

pub(crate) fn run_command(command: &mut Command) -> Output {
    output_within(command, PROGRAM_DEADLINE)
        .unwrap_or_else(|error| panic!("run native test command: {error}"))
}
