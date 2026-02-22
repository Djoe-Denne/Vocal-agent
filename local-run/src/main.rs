use std::env;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy)]
struct ServiceSpec {
    name: &'static str,
    package: &'static str,
    bin: &'static str,
    working_dir: &'static str,
    feature: Option<&'static str>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("local-run failed: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut use_cuda = true;
    let mut run_env = "development".to_string();

    for arg in env::args().skip(1) {
        if arg == "--cpu" {
            use_cuda = false;
        } else if let Some(value) = arg.strip_prefix("--env=") {
            run_env = value.to_string();
        } else {
            return Err(format!(
                "unknown argument `{arg}`\nusage: cargo run -p local-run [--cpu] [--env=development|test|production]"
            ));
        }
    }

    let repo_root = resolve_repo_root()?;
    kill_existing_processes(&[
        "audio-service",
        "asr-service",
        "alignment-service",
        "orchestration-service",
    ]);

    let services = vec![
        ServiceSpec {
            name: "audio-service",
            package: "audio-setup",
            bin: "audio-service",
            working_dir: "audio-service",
            feature: None,
        },
        ServiceSpec {
            name: "asr-service",
            package: "asr-setup",
            bin: "asr-service",
            working_dir: "asr-service",
            feature: if use_cuda { Some("whisper-cuda") } else { None },
        },
        ServiceSpec {
            name: "alignment-service",
            package: "alignment-setup",
            bin: "alignment-service",
            working_dir: "alignment-service",
            feature: if use_cuda { Some("wav2vec2-cuda") } else { None },
        },
        ServiceSpec {
            name: "orchestration-service",
            package: "orchestration-setup",
            bin: "orchestration-service",
            working_dir: "orchestration-service",
            feature: None,
        },
    ];

    let mut children = Vec::with_capacity(services.len());
    for service in services {
        let child = spawn_service(service, &repo_root, &run_env)?;
        println!("started {} (pid={})", service.name, child.id());
        children.push((service, child));
    }

    println!("all services started; press Ctrl+C to stop");
    monitor_children(&mut children)
}

fn spawn_service(
    service: ServiceSpec,
    repo_root: &Path,
    run_env: &str,
) -> Result<Child, String> {
    let working_dir = repo_root.join(service.working_dir);
    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("-p")
        .arg(service.package)
        .arg("--bin")
        .arg(service.bin);
    if let Some(feature) = service.feature {
        command.arg("--features").arg(feature);
    }
    command
        .current_dir(&working_dir)
        .env("RUN_ENV", run_env)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    command.spawn().map_err(|err| {
        format!(
            "could not start {} from `{}`: {err}",
            service.name,
            working_dir.display()
        )
    })
}

fn monitor_children(children: &mut [(ServiceSpec, Child)]) -> Result<(), String> {
    loop {
        for idx in 0..children.len() {
            let status = match children[idx].1.try_wait() {
                Ok(Some(status)) => {
                    status
                }
                Ok(None) => continue,
                Err(err) => {
                    let service_name = children[idx].0.name;
                    for (_, other_child) in children.iter_mut() {
                        let _ = other_child.kill();
                    }
                    return Err(format!("failed while monitoring {}: {err}", service_name));
                }
            };

            let service_name = children[idx].0.name;
            eprintln!("{} exited with status {}", service_name, status);
            for (other_idx, (_, other_child)) in children.iter_mut().enumerate() {
                if other_idx != idx {
                    let _ = other_child.kill();
                }
            }
            if status.success() {
                return Ok(());
            }
            return Err(format!("{} exited unexpectedly", service_name));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn resolve_repo_root() -> Result<PathBuf, String> {
    let current_dir = env::current_dir().map_err(|err| format!("cannot read cwd: {err}"))?;
    if looks_like_repo_root(&current_dir) {
        return Ok(current_dir);
    }

    let manifest_parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "could not infer repository root".to_string())?;
    if looks_like_repo_root(&manifest_parent) {
        return Ok(manifest_parent);
    }

    Err("local-run must be launched from the workspace root".to_string())
}

fn looks_like_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("audio-service").is_dir()
        && path.join("asr-service").is_dir()
        && path.join("alignment-service").is_dir()
        && path.join("orchestration-service").is_dir()
}

#[cfg(target_os = "windows")]
fn kill_existing_processes(names: &[&str]) {
    for name in names {
        let exe_name = format!("{name}.exe");
        let _ = Command::new("taskkill")
            .arg("/IM")
            .arg(exe_name)
            .arg("/F")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(not(target_os = "windows"))]
fn kill_existing_processes(names: &[&str]) {
    for name in names {
        let _ = Command::new("pkill")
            .arg("-f")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
