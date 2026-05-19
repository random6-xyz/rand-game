use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=schema/game_common.fbs");
    println!("cargo:rerun-if-changed=schema/game_input.fbs");
    println!("cargo:rerun-if-changed=schema/game_output.fbs");

    std::fs::create_dir_all("src/flatbuffers_generated")
        .expect("failed to create flatbuffers output directory");

    let status = Command::new("flatc")
        .args([
            "--rust",
            "-o",
            "src/flatbuffers_generated",
            "schema/game_common.fbs",
            "schema/game_input.fbs",
            "schema/game_output.fbs",
        ])
        .status()
        .expect("failed to run flatc");

    assert!(status.success(), "flatc failed");
}
