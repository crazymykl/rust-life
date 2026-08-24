use assert_cmd::{Command, cargo};
use rust_life::CLEAR;

fn bin() -> Command {
    Command::new(cargo::cargo_bin!("rust-life"))
}

#[test]
fn test_help() {
    bin().arg("--help").assert().success();
}

#[test]
fn test_cli() {
    bin()
        .args([
            #[cfg(feature = "gui")]
            "--no-gui",
            "-g0",
        ])
        .assert()
        .success();
}

#[test]
fn test_cli_template() {
    let base_args = &[
        #[cfg(feature = "gui")]
        "--no-gui",
        "-g0",
        "-t",
        "@",
    ][..];

    bin()
        .args([base_args, &["-p1"]].concat())
        .assert()
        .stdout("...\n.@.\n...\n")
        .success();

    bin()
        .args([base_args, &["-p", "1", "1"]].concat())
        .assert()
        .stdout("...\n.@.\n...\n")
        .success();

    bin()
        .args([base_args, &["-p", "1", "1", "1"]].concat())
        .assert()
        .stdout("...\n.@.\n...\n")
        .success();

    bin()
        .args([base_args, &["-p", "1", "1", "1", "1"]].concat())
        .assert()
        .stdout("...\n.@.\n...\n")
        .success();

    bin()
        .args([base_args, &["-p", "1", "1", "1", "1", "1"]].concat())
        .assert()
        .failure();

    bin().args([base_args, &["-p"]].concat()).assert().failure();

    let align_args = &[base_args, &["-r3", "-c3"]].concat()[..];

    bin()
        .args(align_args)
        .assert()
        .stdout("...\n.@.\n...\n")
        .success();

    bin()
        .args([align_args, &["--align", "top-left"]].concat())
        .assert()
        .stdout("@..\n...\n...\n")
        .success();

    bin()
        .args([align_args, &["-a=bottom-right"]].concat())
        .assert()
        .stdout("...\n...\n..@\n")
        .success();
}

#[test]
fn test_cli_generations() {
    let base_args = &[
        #[cfg(feature = "gui")]
        "--no-gui",
        "-t",
        "...\n@@@\n...",
        "-p0",
    ][..];

    bin()
        .args([base_args, &["-g0"]].concat())
        .assert()
        .stdout("...\n@@@\n...\n")
        .success();

    bin()
        .args([base_args, &["-g1"]].concat())
        .assert()
        .stdout(".@.\n.@.\n.@.\n")
        .success();

    bin()
        .args([base_args, &["-g2"]].concat())
        .assert()
        .stdout("...\n@@@\n...\n")
        .success();

    bin()
        .args([base_args, &["-G1"]].concat())
        .assert()
        .stdout(format!("{CLEAR}...\n@@@\n...\n{CLEAR}.@.\n.@.\n.@.\n"))
        .success();

    bin()
        .args([base_args, &["-G2"]].concat())
        .assert()
        .stdout(format!(
            "{CLEAR}...\n@@@\n...\n{CLEAR}.@.\n.@.\n.@.\n{CLEAR}...\n@@@\n...\n"
        ))
        .success();

    bin()
        .args([base_args, &["-g1", "-G2"]].concat())
        .assert()
        .stdout(format!("{CLEAR}.@.\n.@.\n.@.\n{CLEAR}...\n@@@\n...\n"))
        .success();
}

#[test]
fn test_cli_rules() {
    // A valid custom rule is accepted and runs (Day & Night on a blank 3x3).
    bin()
        .args([
            #[cfg(feature = "gui")]
            "--no-gui",
            "-g0",
            "--rules",
            "B368/S245",
            "-t",
            "@",
            "-p1",
        ])
        .assert()
        .stdout("...\n.@.\n...\n")
        .success();

    // An out-of-range neighbor count is rejected at parse time.
    bin()
        .args([
            #[cfg(feature = "gui")]
            "--no-gui",
            "-g0",
            "--rules",
            "B9/S23",
        ])
        .assert()
        .failure();
}

#[test]
fn test_cli_backend_bitboard() {
    // The bit-packed backend must evolve identically to the default `Board`.
    bin()
        .args([
            #[cfg(feature = "gui")]
            "--no-gui",
            "--backend",
            "bitboard",
            "-g1",
            "-t",
            "...\n@@@\n...",
            "-p0",
        ])
        .assert()
        .stdout(".@.\n.@.\n.@.\n")
        .success();
}

#[cfg(feature = "unstable")]
#[test]
fn test_cli_backend_simd() {
    // The SIMD backend must evolve identically to the default `Board`.
    bin()
        .args([
            #[cfg(feature = "gui")]
            "--no-gui",
            "--backend",
            "simd",
            "-g1",
            "-t",
            "...\n@@@\n...",
            "-p0",
        ])
        .assert()
        .stdout(".@.\n.@.\n.@.\n")
        .success();
}

#[test]
fn test_cli_bad_template() {
    // A template with an invalid character is rejected at parse time and the
    // process exits non-zero.
    bin()
        .args([
            #[cfg(feature = "gui")]
            "--no-gui",
            "-g0",
            "-t",
            "X",
        ])
        .assert()
        .failure();
}

#[test]
#[cfg(feature = "gui")]
fn test_gui_scale() {
    bin().args(["-s0"]).assert().failure();
    bin().args(["-s.5", "-G0", "-x"]).assert().success();
    bin().args(["-s4", "-G0", "-x"]).assert().success();
    bin().args(["-s9999"]).assert().failure();
}

#[test]
#[cfg(feature = "gui")]
fn test_gui_run() {
    bin().args(["-g0", "-G5", "-x"]).assert().success();
}
