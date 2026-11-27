use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RC_EXE") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let kits_bin = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits_bin) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path().join(r"x64\rc.exe")))
            .filter(|p| p.is_file())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            return Some(latest);
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/app.rc");
    println!("cargo:rerun-if-changed=assets/icons/app.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let rc_path = Path::new("assets/icons/app.rc");
    let ico_path = Path::new("assets/icons/app.ico");
    if !rc_path.is_file() || !ico_path.is_file() {
        return;
    }

    let Some(rc_exe) = find_rc_exe() else {
        println!("cargo:warning=rc.exe not found; skipping gui.exe icon resource");
        return;
    };

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let res_path = out_dir.join("app.res");
    let status = Command::new(rc_exe)
        .args(["/nologo", "/fo"])
        .arg(&res_path)
        .arg(rc_path)
        .status()
        .expect("invoke rc.exe");
    assert!(status.success(), "rc.exe failed with {status}");

    println!("cargo:rustc-link-arg-bin=gui={}", res_path.display());
}
