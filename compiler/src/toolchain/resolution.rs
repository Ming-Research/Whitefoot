//! One selected native consumer, shared by emission probes and driver commands.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The source-build default clang: where a compiler with no package beside it
/// looks. Only this file names it; every other step takes the resolved
/// toolchain.
const fn clang_executable() -> &'static str {
    if cfg!(target_os = "windows") {
        "clang"
    } else {
        "/usr/bin/clang"
    }
}

/// Internal driver integration, not a command-line toolchain override.
#[doc(hidden)]
#[derive(Debug)]
pub struct Toolchain {
    clang: PathBuf,
    linker: Option<PathBuf>,
    sdk: Option<PathBuf>,
    pub(super) packaged: bool,
    /// The package's `toolchain-manifest.json`, above the `bin/` holding clang.
    manifest: Option<PathBuf>,
    version: OnceLock<Result<Vec<u8>, String>>,
}

impl Toolchain {
    fn from_executable_directory(directory: &Path) -> Result<Self, String> {
        Self::with_prerequisites(directory, configure_platform)
    }

    // Tests inject platform prerequisites without changing process environment
    // or requiring an installed SDK. Production always uses configure_platform.
    fn with_prerequisites(
        directory: &Path,
        configure: impl FnOnce(&mut Self, &Path) -> Result<(), String>,
    ) -> Result<Self, String> {
        let clang = directory.join(if cfg!(target_os = "windows") {
            "clang.exe"
        } else {
            "clang"
        });
        match fs::symlink_metadata(&clang) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if package_manifest_present(directory) {
                    return Err(format!(
                        "missing packaged clang {}; toolchain-manifest.json marks this installation as a bundled package",
                        clang.display()
                    ));
                }
                return Ok(Self {
                    clang: PathBuf::from(clang_executable()),
                    linker: None,
                    sdk: None,
                    packaged: false,
                    manifest: None,
                    version: OnceLock::new(),
                });
            }
            Err(error) => {
                return Err(format!(
                    "cannot inspect packaged clang {}: {error}",
                    clang.display()
                ));
            }
        }
        let directory = fs::canonicalize(directory).map_err(|error| {
            format!(
                "cannot resolve packaged toolchain directory {}: {error}",
                directory.display()
            )
        })?;
        let clang = executable_path(&clang, "packaged clang")?;
        let mut toolchain = Self {
            clang,
            linker: None,
            sdk: None,
            packaged: true,
            manifest: directory
                .parent()
                .map(|root| root.join("toolchain-manifest.json")),
            version: OnceLock::new(),
        };
        configure(&mut toolchain, &directory)?;
        Ok(toolchain)
    }

    /// The selected clang, absolute in packaged toolchains.
    pub fn clang(&self) -> &Path {
        &self.clang
    }

    /// Every compile, probe and assembly command starts here.
    ///
    /// On Windows it names the emitted target explicitly, so a clang whose
    /// default target is MSVC compiles the runtime and the module for the
    /// same MinGW ABI the module's `target triple` states, rather than
    /// whichever ABI that clang was configured for.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.clang);
        #[cfg(target_os = "windows")]
        {
            let mut target = OsString::from("--target=");
            target.push(crate::target::WINDOWS_X86_64_TRIPLE);
            command.arg(target);
        }
        if let Some(sdk) = &self.sdk {
            command.arg("-isysroot").arg(sdk);
        }
        command
    }

    /// On Windows every link runs LLD's MinGW driver, since the GNU target's
    /// default linker would otherwise be whatever `ld` clang finds. Elsewhere
    /// linker selection applies only to packaged tools; source builds retain
    /// their existing ordinary and LTO linker flags unchanged.
    pub fn link_command(&self) -> Command {
        let mut command = self.command();
        #[cfg(target_os = "windows")]
        command.arg("-fuse-ld=lld");
        if let Some(linker) = &self.linker {
            #[cfg(target_os = "linux")]
            command.arg("-fuse-ld=lld");
            let mut argument = OsString::from("--ld-path=");
            argument.push(linker);
            command.arg(argument);
        }
        command
    }

    /// The package's `toolchain-manifest.json`, whose recorded clang answers a
    /// packaged compiler may take in place of probing. `None` for a source
    /// build, which has no package.
    pub(super) fn manifest(&self) -> Option<&Path> {
        self.manifest.as_deref()
    }

    /// The first line `clang --version` prints, read once per process.
    pub(super) fn version_line(&self) -> Result<&[u8], String> {
        self.version
            .get_or_init(|| {
                #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
                let output = Command::new(&self.clang)
                    .arg("--version")
                    .output()
                    .map_err(|error| {
                        format!("cannot start {} --version: {error}", self.clang.display())
                    })?;
                if !output.status.success() {
                    return Err(format!(
                        "{} --version exited with {}: {}",
                        self.clang.display(),
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                let line = output
                    .stdout
                    .split(|byte| *byte == b'\n')
                    .next()
                    .unwrap_or_default();
                if line.is_empty() {
                    return Err(format!(
                        "{} --version returned no version line",
                        self.clang.display()
                    ));
                }
                Ok(line.to_vec())
            })
            .as_ref()
            .map(Vec::as_slice)
            .map_err(Clone::clone)
    }

    /// Native product identity includes the selected paths and clang's first
    /// version line. Length prefixes preserve native OS path bytes unambiguously.
    pub fn cache_identity(&self) -> Result<[u8; 32], String> {
        let version = self.version_line()?;
        Ok(identity(
            &self.clang,
            version,
            self.linker.as_deref(),
            self.sdk.as_deref(),
        ))
    }
}

// Existence is enough to identify the root/bin installation contract; the
// manifest is not parsed to select compiler behavior. A broken symlink or an
// inaccessible marker must not enable a fallback to an unrelated host clang.
fn package_manifest_present(directory: &Path) -> bool {
    let Some(root) = directory.parent() else {
        return false;
    };
    match fs::symlink_metadata(root.join("toolchain-manifest.json")) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

#[cfg(target_os = "linux")]
fn configure_platform(toolchain: &mut Toolchain, directory: &Path) -> Result<(), String> {
    // lld selects its ELF driver from argv[0]; canonicalizing this alias to
    // the generic lld file would lose that selection. Validate its target,
    // then retain the absolute ld.lld alias spelling for clang's --ld-path.
    let alias = directory.join("ld.lld");
    executable_path(&alias, "packaged lld ELF driver alias ld.lld")?;
    toolchain.linker = Some(alias);
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_platform(toolchain: &mut Toolchain, _directory: &Path) -> Result<(), String> {
    let (sdk, linker) = discover_macos_prerequisites()?;
    toolchain.sdk = Some(sdk);
    toolchain.linker = Some(linker);
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_platform(toolchain: &mut Toolchain, directory: &Path) -> Result<(), String> {
    // As on Linux, lld selects its driver from argv[0]: `ld.lld` is the GNU
    // driver, which hands a `-m i386pep` link to its MinGW driver. Keep the
    // alias spelling for clang's --ld-path; the MinGW headers, import
    // libraries and compiler-rt builtins are found by the bundled clang
    // relative to its own directory.
    let alias = directory.join("ld.lld.exe");
    executable_path(&alias, "packaged lld GNU driver alias ld.lld.exe")?;
    toolchain.linker = Some(alias);
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn configure_platform(_toolchain: &mut Toolchain, _directory: &Path) -> Result<(), String> {
    Ok(())
}

fn identity(clang: &Path, version: &[u8], linker: Option<&Path>, sdk: Option<&Path>) -> [u8; 32] {
    let mut bytes = b"native-toolchain 2\n".to_vec();
    for field in [
        clang.as_os_str().as_encoded_bytes(),
        version,
        linker.map_or(&b"source-default-linkers"[..], |path| {
            path.as_os_str().as_encoded_bytes()
        }),
        sdk.map_or(&b"source-default-sdk"[..], |path| {
            path.as_os_str().as_encoded_bytes()
        }),
    ] {
        bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
        bytes.extend_from_slice(field);
    }
    crate::content_digest(&bytes)
}

fn executable_path(path: &Path, component: &str) -> Result<PathBuf, String> {
    let resolved = fs::canonicalize(path).map_err(|error| {
        format!(
            "missing or inaccessible {component} {}: {error}",
            path.display()
        )
    })?;
    let metadata = fs::metadata(&resolved)
        .map_err(|error| format!("cannot inspect {component} {}: {error}", resolved.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "{component} {} is not an executable file",
            resolved.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "{component} {} is not executable",
                resolved.display()
            ));
        }
    }
    Ok(resolved)
}

#[cfg(target_os = "macos")]
fn discover_macos_prerequisites() -> Result<(PathBuf, PathBuf), String> {
    let sdk = match std::env::var_os("SDKROOT") {
        Some(root) => validate_sdk(Path::new(&root))?,
        None => validate_sdk(&xcrun_path("--show-sdk-path", "macOS SDK")?)?,
    };
    let linker = xcrun_path("--find", "macOS system linker")?;
    let linker = executable_path(&linker, "macOS system linker").map_err(macos_help)?;
    Ok((sdk, linker))
}

#[cfg(target_os = "macos")]
fn validate_sdk(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || !path.join("usr/include").is_dir()
        || !path.join("usr/lib/libSystem.tbd").is_file()
        || !(path.join("SDKSettings.json").is_file() || path.join("SDKSettings.plist").is_file())
    {
        return Err(macos_help(format!(
            "invalid macOS SDK (SDKROOT or xcrun result) {}: expected an absolute SDK with headers, libSystem.tbd and SDKSettings",
            path.display()
        )));
    }
    fs::canonicalize(path).map_err(|error| {
        macos_help(format!(
            "cannot resolve macOS SDK {}: {error}",
            path.display()
        ))
    })
}

#[cfg(target_os = "macos")]
fn xcrun_path(argument: &str, component: &str) -> Result<PathBuf, String> {
    let mut command = Command::new("/usr/bin/xcrun");
    command.args(["--sdk", "macosx", argument]);
    if argument == "--find" {
        command.arg("ld");
    }
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    let output = command.output().map_err(|error| {
        macos_help(format!(
            "cannot discover {component} with {command:?}: {error}"
        ))
    })?;
    if !output.status.success() {
        return Err(macos_help(format!(
            "cannot discover {component} with {command:?}: {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    use std::os::unix::ffi::OsStringExt;
    let mut bytes = output.stdout;
    while bytes
        .last()
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
    {
        bytes.pop();
    }
    let path = PathBuf::from(OsString::from_vec(bytes));
    if !path.is_absolute() {
        return Err(macos_help(format!(
            "{command:?} returned no absolute path for {component}"
        )));
    }
    Ok(path)
}

#[cfg(target_os = "macos")]
fn macos_help(message: String) -> String {
    format!(
        "{message}; selected DEVELOPER_DIR={}; install Command Line Tools with xcode-select --install or select an existing Xcode/CLT installation",
        std::env::var_os("DEVELOPER_DIR").map_or_else(
            || "<xcode-select default>".to_owned(),
            |path| path.to_string_lossy().into_owned()
        )
    )
}

fn toolchain_for_executable(executable: &Path) -> Result<Toolchain, String> {
    let executable = fs::canonicalize(executable).map_err(|error| {
        format!(
            "cannot resolve the running compiler's executable {}: {error}",
            executable.display()
        )
    })?;
    let directory = executable
        .parent()
        .ok_or_else(|| "the running compiler has no executable directory".to_owned())?;
    Toolchain::from_executable_directory(directory)
}

/// Resolve current_exe through symlinks before looking beside whitefootc.
#[doc(hidden)]
pub fn resolved_toolchain() -> Result<&'static Toolchain, String> {
    static TOOLCHAIN: OnceLock<Result<Toolchain, String>> = OnceLock::new();
    TOOLCHAIN
        .get_or_init(|| {
            let executable = std::env::current_exe().map_err(|error| {
                format!("cannot find the running compiler's executable: {error}")
            })?;
            toolchain_for_executable(&executable)
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_sibling_preserves_source_default() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        let toolchain = Toolchain::from_executable_directory(&directory.0).expect("source default");
        assert_eq!(toolchain.clang(), Path::new(clang_executable()));
        assert!(!toolchain.packaged);
        assert!(toolchain.linker.is_none() && toolchain.sdk.is_none());
    }

    #[test]
    fn a_bundle_manifest_requires_its_sibling_clang() {
        let root = super::super::ProbeDirectory::new().expect("scratch directory");
        let directory = root.0.join("bin");
        fs::create_dir(&directory).expect("package bin directory");
        fs::write(root.0.join("toolchain-manifest.json"), "{}").expect("package marker");
        let error = Toolchain::from_executable_directory(&directory)
            .expect_err("a bundle with deleted clang must not select the host");
        assert!(error.contains("missing packaged clang"), "{error}");
        assert!(
            error.contains(
                &directory
                    .join(if cfg!(target_os = "windows") {
                        "clang.exe"
                    } else {
                        "clang"
                    })
                    .display()
                    .to_string()
            ),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_manifest_marker_still_requires_packaged_clang() {
        let root = super::super::ProbeDirectory::new().expect("scratch directory");
        let directory = root.0.join("bin");
        fs::create_dir(&directory).expect("package bin directory");
        std::os::unix::fs::symlink("missing-manifest", root.0.join("toolchain-manifest.json"))
            .expect("broken package marker");
        let error = Toolchain::from_executable_directory(&directory)
            .expect_err("a broken package marker must not enable host fallback");
        assert!(error.contains("missing packaged clang"), "{error}");
    }

    /// Write an executable script and return once it can be started. The
    /// script must be harmless to run one more time than the test runs it.
    ///
    /// The tests run on parallel threads. A child that another thread forks
    /// while this thread has the script open for writing keeps its copy of the
    /// write end until it execs, and Linux refuses to start a file that anyone
    /// still has open for writing (ETXTBSY, "Text file busy"). Every holder
    /// lets go when it execs or exits, and a child forked after the write end
    /// closed here holds none, so a start that stops being refused ends the
    /// window for good.
    #[cfg(unix)]
    fn write_executable(path: &Path, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, script).expect("write script");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("executable permission");
        wait_until_startable(path);
    }

    /// Start `path` until the kernel stops refusing it as busy.
    #[cfg(unix)]
    fn wait_until_startable(path: &Path) {
        use std::io::ErrorKind;
        use std::time::{Duration, Instant};
        // Bounds a hang, not a verdict: a holder is gone within moments.
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
            let started = Command::new(path).arg("--version").output();
            match started {
                Err(error) if error.kind() == ErrorKind::ExecutableFileBusy => {
                    assert!(
                        Instant::now() < deadline,
                        "{} stayed busy: {error}",
                        path.display()
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("cannot start {}: {error}", path.display()),
                Ok(_) => return,
            }
        }
    }

    #[cfg(unix)]
    fn fake_clang(directory: &Path) -> PathBuf {
        let clang = directory.join("clang");
        write_executable(
            &clang,
            "#!/bin/sh\nprintf '%s\\n' 'fake bundled clang 22'\n",
        );
        clang
    }

    // Linux refuses to start a file that is open for writing and macOS does
    // not, so the wait has something to wait for only on Linux.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_script_open_for_writing_is_waited_for() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        let script = directory.0.join("script");
        fs::write(&script, "#!/bin/sh\n").expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .expect("executable permission");
        let writer = fs::OpenOptions::new()
            .append(true)
            .open(&script)
            .expect("writer");
        let released = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
                released.store(true, Ordering::SeqCst);
                drop(writer);
            });
            wait_until_startable(&script);
            assert!(
                released.load(Ordering::SeqCst),
                "the script started while a writer still held it"
            );
        });
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sibling_clang_is_selected_and_lld_is_explicit() {
        let scratch = super::super::ProbeDirectory::new().expect("scratch directory");
        let directory = scratch.0.join("bundle with space");
        fs::create_dir(&directory).expect("bundle directory");
        let clang = fake_clang(&directory);
        let lld = directory.join("lld");
        fs::copy(&clang, &lld).expect("fake linker");
        std::os::unix::fs::symlink("lld", directory.join("ld.lld")).expect("ELF driver alias");
        let toolchain =
            Toolchain::from_executable_directory(&directory).expect("packaged toolchain");
        assert_eq!(
            toolchain.clang(),
            fs::canonicalize(&clang).expect("clang path")
        );
        let command = toolchain.link_command();
        let arguments = command
            .get_args()
            .map(|arg| arg.to_owned())
            .collect::<Vec<_>>();
        let mut expected = OsString::from("--ld-path=");
        expected.push(
            fs::canonicalize(&directory)
                .expect("bundle path")
                .join("ld.lld"),
        );
        assert_eq!(arguments, [OsString::from("-fuse-ld=lld"), expected]);
        toolchain.cache_identity().expect("fake clang version");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sibling_clang_without_lld_is_an_error() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        fake_clang(&directory.0);
        let error =
            Toolchain::from_executable_directory(&directory.0).expect_err("missing lld must fail");
        assert!(error.contains("lld"), "{error}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn generic_lld_without_its_elf_alias_is_incomplete() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        let clang = fake_clang(&directory.0);
        fs::copy(clang, directory.0.join("lld")).expect("generic lld file");
        let error = Toolchain::from_executable_directory(&directory.0)
            .expect_err("generic lld cannot select its ELF driver from argv[0]");
        assert!(error.contains("ld.lld"), "{error}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn compiler_symlink_selects_the_real_bundle_directory() {
        let scratch = super::super::ProbeDirectory::new().expect("scratch directory");
        let directory = scratch.0.join("real bundle with space");
        fs::create_dir(&directory).expect("bundle directory");
        let clang = fake_clang(&directory);
        fs::copy(&clang, directory.join("lld")).expect("fake linker");
        std::os::unix::fs::symlink("lld", directory.join("ld.lld")).expect("ELF alias");
        let compiler = directory.join("whitefootc");
        fs::write(&compiler, "fake compiler").expect("compiler path");
        let symlink = scratch.0.join("whitefootc link");
        std::os::unix::fs::symlink(&compiler, &symlink).expect("compiler symlink");
        let toolchain = toolchain_for_executable(&symlink).expect("real bundle resolution");
        assert_eq!(
            toolchain.clang(),
            fs::canonicalize(clang).expect("clang path")
        );
    }

    #[cfg(unix)]
    #[test]
    fn cache_identity_rejects_a_failed_version_command() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        write_executable(
            &directory.0.join("clang"),
            "#!/bin/sh\nprintf '%s\n' 'pretend version'\nexit 7\n",
        );
        let toolchain =
            Toolchain::with_prerequisites(&directory.0, |_, _| Ok(())).expect("fake bundle");
        let error = toolchain
            .cache_identity()
            .expect_err("failed version is no identity");
        assert!(
            error.contains("--version exited with exit status: 7"),
            "{error}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_invalid_sdk_is_a_toolchain_error() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        let error = validate_sdk(&directory.0).expect_err("a directory alone is not an SDK");
        assert!(
            error.contains("invalid macOS SDK") && error.contains("SDKROOT"),
            "{error}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sibling_clang_uses_injected_sdk_and_system_linker() {
        let scratch = super::super::ProbeDirectory::new().expect("scratch directory");
        let directory = scratch.0.join("bundle with space");
        fs::create_dir(&directory).expect("bundle directory");
        let clang = fake_clang(&directory);
        let sdk = directory.join("macOS SDK");
        let linker = directory.join("system ld");
        let toolchain = Toolchain::with_prerequisites(&directory, |toolchain, _| {
            toolchain.sdk = Some(sdk.clone());
            toolchain.linker = Some(linker.clone());
            Ok(())
        })
        .expect("packaged toolchain");
        assert_eq!(
            toolchain.clang(),
            fs::canonicalize(&clang).expect("clang path")
        );
        let command = toolchain.link_command();
        let arguments = command
            .get_args()
            .map(|arg| arg.to_owned())
            .collect::<Vec<_>>();
        let mut expected = OsString::from("--ld-path=");
        expected.push(&linker);
        assert_eq!(
            arguments,
            [OsString::from("-isysroot"), sdk.into_os_string(), expected]
        );
        toolchain.cache_identity().expect("fake clang version");
    }

    #[cfg(unix)]
    #[test]
    fn packaged_probe_launch_failure_is_an_error() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        let clang = fake_clang(&directory.0);
        let toolchain = Toolchain::with_prerequisites(&directory.0, |_, _| Ok(()))
            .expect("resolve fake bundled clang");
        toolchain.cache_identity().expect("initial version command");
        fs::remove_file(clang).expect("remove clang before probe");
        let error = super::super::ToolchainFacts::probe(&toolchain)
            .err()
            .expect("missing bundled executable must fail the probes");
        assert!(
            error.contains("capability probe") && error.contains("clang"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn broken_sibling_clang_cannot_select_source_default() {
        let directory = super::super::ProbeDirectory::new().expect("scratch directory");
        std::os::unix::fs::symlink("missing-clang", directory.0.join("clang"))
            .expect("broken packaged alias");
        let error = Toolchain::with_prerequisites(&directory.0, |_, _| Ok(()))
            .expect_err("broken sibling must be a package error");
        assert!(error.contains("packaged clang"), "{error}");
    }

    /// A package whose clang records every call's arguments and answers
    /// `--version`, and whose manifest is `manifest` when given. The clang
    /// accepts every probe, so a probed answer is `preserve_nonecc` taken,
    /// `captures(none)` and a `void` `llvm.coro.end`; `manifest_recording`
    /// records the opposite answers, so a taken record cannot be told from a
    /// probe only by being equal.
    #[cfg(unix)]
    fn recording_package(manifest: Option<&str>) -> (super::super::ProbeDirectory, PathBuf, Toolchain) {
        let root = super::super::ProbeDirectory::new().expect("scratch directory");
        let bin = root.0.join("bin");
        fs::create_dir(&bin).expect("package bin directory");
        let log = root.0.join("clang-calls.log");
        write_executable(
            &bin.join("clang"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = --version ]; then printf '%s\\n' 'clang version 22.1.8'; fi\nexit 0\n",
                log.display()
            ),
        );
        // Waiting for the script to become startable ran it once.
        let _ = fs::remove_file(&log);
        if let Some(text) = manifest {
            fs::write(root.0.join("toolchain-manifest.json"), text).expect("manifest");
        }
        let toolchain =
            Toolchain::with_prerequisites(&bin, |_, _| Ok(())).expect("fake package");
        (root, log, toolchain)
    }

    #[cfg(unix)]
    fn manifest_recording(version: &str) -> String {
        format!(
            "{{\"llvm_version\":\"22.1.8\",\"files\":[],\"clang_facts\":{{\"clang_version\":\"{version}\",\
             \"preserve_none\":false,\"no_capture_attribute\":\"nocapture\",\"coro_end_result\":\"i1\",\
             \"darwin_stack_probe_helper\":false}}}}"
        )
    }

    #[cfg(unix)]
    fn calls(log: &Path) -> Vec<String> {
        fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[cfg(unix)]
    const PROBES_ASKED: usize = if cfg!(target_os = "macos") { 4 } else { 3 };

    #[cfg(unix)]
    #[test]
    fn recorded_answers_for_this_clang_are_taken_without_a_probe() {
        use super::super::{FactsSource, resolve};
        let (_root, log, toolchain) =
            recording_package(Some(&manifest_recording("clang version 22.1.8")));
        let resolved = resolve(&toolchain).expect("resolve the package's facts");
        assert_eq!(resolved.source, FactsSource::Manifest);
        assert_eq!(resolved.probes_run, 0);
        assert_eq!(resolved.clang_version.as_deref(), Some("clang version 22.1.8"));
        // The record says what this clang's probes would not, so these are the
        // record's answers, not the probes'.
        assert!(!resolved.facts.preserve_none);
        assert_eq!(resolved.facts.no_capture_attribute, "nocapture");
        assert_eq!(resolved.facts.coro_end_result, "i1");
        assert_eq!(calls(&log), ["--version"], "only the version line is asked");
    }

    #[cfg(unix)]
    fn assert_probed_for(
        manifest: Option<&str>,
        reason_names: &[&str],
    ) {
        use super::super::{FactsSource, resolve};
        let (_root, log, toolchain) = recording_package(manifest);
        let resolved = resolve(&toolchain).expect("resolve the package's facts");
        let FactsSource::Probed { reason } = &resolved.source else {
            panic!("the record must not be taken");
        };
        let reason = reason.as_deref().expect("a package says why it probed");
        for name in reason_names {
            assert!(reason.contains(name), "{reason:?} does not name {name:?}");
        }
        assert_eq!(resolved.probes_run, PROBES_ASKED);
        assert!(resolved.facts.preserve_none, "the probes' answers");
        assert_eq!(resolved.facts.no_capture_attribute, "captures(none)");
        assert_eq!(resolved.facts.coro_end_result, "void");
        let calls = calls(&log);
        assert_eq!(calls.first().map(String::as_str), Some("--version"));
        assert_eq!(
            calls.iter().filter(|call| call.starts_with("-x ir")).count(),
            PROBES_ASKED,
            "{calls:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn another_recorded_clang_version_falls_back_to_probing_and_says_why() {
        assert_probed_for(
            Some(&manifest_recording("clang version 21.0.0")),
            &["clang version 21.0.0", "clang version 22.1.8"],
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_recorded_answer_falls_back_to_probing_and_names_it() {
        let without =
            manifest_recording("clang version 22.1.8").replace("\"preserve_none\":false,", "");
        assert_probed_for(Some(&without), &["preserve_none"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_package_with_no_manifest_probes_and_says_so() {
        assert_probed_for(None, &["toolchain-manifest.json"]);
    }

    #[test]
    fn cache_identity_tracks_every_toolchain_component() {
        let clang = Path::new("/bundle/clang");
        let linker = Path::new("/bundle/ld.lld");
        let sdk = Path::new("/sdk/one");
        let base = identity(clang, b"clang 22", Some(linker), Some(sdk));
        for changed in [
            identity(
                Path::new("/other/clang"),
                b"clang 22",
                Some(linker),
                Some(sdk),
            ),
            identity(clang, b"clang 23", Some(linker), Some(sdk)),
            identity(
                clang,
                b"clang 22",
                Some(Path::new("/other/ld.lld")),
                Some(sdk),
            ),
            identity(
                clang,
                b"clang 22",
                Some(linker),
                Some(Path::new("/sdk/two")),
            ),
        ] {
            assert_ne!(base, changed);
        }
    }
}
