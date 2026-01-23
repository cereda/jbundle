use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::BuildSystem;
use crate::error::PackError;

pub fn build_uberjar(project_dir: &Path, system: BuildSystem) -> Result<PathBuf, PackError> {
    match system {
        BuildSystem::DepsEdn => build_deps_edn(project_dir),
        BuildSystem::Leiningen => build_leiningen(project_dir),
    }
}

fn build_deps_edn(project_dir: &Path) -> Result<PathBuf, PackError> {
    let strategy = detect_deps_strategy(project_dir);

    match strategy {
        DepsStrategy::ToolsBuild => run_tools_build(project_dir),
        DepsStrategy::Uberjar => run_depstar_uberjar(project_dir),
    }
}

#[derive(Debug)]
enum DepsStrategy {
    ToolsBuild,
    Uberjar,
}

fn detect_deps_strategy(project_dir: &Path) -> DepsStrategy {
    if project_dir.join("build.clj").exists() {
        return DepsStrategy::ToolsBuild;
    }

    let deps_path = project_dir.join("deps.edn");
    if let Ok(content) = std::fs::read_to_string(&deps_path) {
        if content.contains(":uberjar") {
            return DepsStrategy::Uberjar;
        }
    }

    DepsStrategy::ToolsBuild
}

fn run_tools_build(project_dir: &Path) -> Result<PathBuf, PackError> {
    tracing::info!("running: clojure -T:build uber");

    let output = Command::new("clojure")
        .args(["-T:build", "uber"])
        .current_dir(project_dir)
        .output()
        .map_err(|e| PackError::BuildFailed(format!("failed to run clojure: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PackError::BuildFailed(format!("clojure -T:build uber failed:\n{stderr}")));
    }

    find_uberjar(project_dir)
}

fn run_depstar_uberjar(project_dir: &Path) -> Result<PathBuf, PackError> {
    tracing::info!("running: clojure -X:uberjar");

    let output = Command::new("clojure")
        .args(["-X:uberjar"])
        .current_dir(project_dir)
        .output()
        .map_err(|e| PackError::BuildFailed(format!("failed to run clojure: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PackError::BuildFailed(format!("clojure -X:uberjar failed:\n{stderr}")));
    }

    find_uberjar(project_dir)
}

fn build_leiningen(project_dir: &Path) -> Result<PathBuf, PackError> {
    tracing::info!("running: lein uberjar");

    let output = Command::new("lein")
        .arg("uberjar")
        .current_dir(project_dir)
        .output()
        .map_err(|e| PackError::BuildFailed(format!("failed to run lein: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PackError::BuildFailed(format!("lein uberjar failed:\n{stderr}")));
    }

    find_uberjar(project_dir)
}

fn find_uberjar(project_dir: &Path) -> Result<PathBuf, PackError> {
    let target_dir = project_dir.join("target");

    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        let mut candidates: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map_or(false, |ext| ext == "jar")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map_or(false, |n| {
                            n.contains("standalone") || n.contains("uber")
                        })
            })
            .collect();

        candidates.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        candidates.reverse();

        if let Some(jar) = candidates.into_iter().next() {
            return Ok(jar);
        }
    }

    // Look for any .jar in target/ (depstar, custom names)
    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        let mut jars: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map_or(false, |ext| ext == "jar")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map_or(false, |n| !n.contains("sources") && !n.contains("javadoc"))
            })
            .collect();

        jars.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        jars.reverse();

        if let Some(jar) = jars.into_iter().next() {
            return Ok(jar);
        }
    }

    Err(PackError::UberjarNotFound(target_dir))
}
