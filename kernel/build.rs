//! Generate the board configuration shared by Rust and the linker script.
//!
//! Cargo features are the single board selector. This build script turns the
//! selected profile into a Rust include and an editor-visible C preprocessor
//! header, so the linker script does not need a second address configuration.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Board {
    feature: &'static str,
    architecture: &'static str,
    output_arch: &'static str,
    kernel_base: u64,
}

const BOARDS: &[Board] = &[
    Board {
        feature: "riscv64-qemu",
        architecture: "riscv64",
        output_arch: "riscv",
        kernel_base: 0x8020_0000,
    },
    Board {
        feature: "spacemitk3-com260kit",
        architecture: "riscv64",
        output_arch: "riscv",
        kernel_base: 0x1_4000_0000,
    },
    Board {
        feature: "aarch64-qemu",
        architecture: "aarch64",
        output_arch: "aarch64",
        kernel_base: 0x4008_0000,
    },
];

const LINKER_SCRIPT: &str = "src/linker.ld";

fn feature_enabled(feature: &str) -> bool {
    let variable = format!("CARGO_FEATURE_{}", feature.replace('-', "_"));
    env::var_os(variable).is_some()
        || env::var("CARGO_CFG_FEATURE")
            .map(|features| features.split(',').any(|candidate| candidate == feature))
            .unwrap_or(false)
}

fn write_file(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap_or_else(|error| {
        panic!("failed to write {}: {error}", path.display());
    });
}

fn preprocess_linker(manifest_dir: &Path, out_dir: &Path, board: &Board, output: &Path) {
    let source = manifest_dir.join(LINKER_SCRIPT);
    let cpp = env::var_os("CPP").unwrap_or_else(|| "cpp".into());
    let status = Command::new(&cpp)
        .args(["-P", "-x", "assembler-with-cpp"])
        .arg(format!("-I{}", out_dir.display()))
        .arg(&source)
        .arg("-o")
        .arg(output)
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {}: {error}", cpp.to_string_lossy()));

    if !status.success() {
        panic!(
            "{} failed while preprocessing {}",
            cpp.to_string_lossy(),
            source.display()
        );
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let selected: Vec<&Board> = BOARDS
        .iter()
        .filter(|board| feature_enabled(board.feature))
        .collect();

    if selected.len() != 1 {
        panic!(
            "select exactly one board feature, got {:?}",
            selected
                .iter()
                .map(|board| board.feature)
                .collect::<Vec<_>>()
        );
    }
    let board = selected[0];

    let rust_config = format!(
        "pub const BOARD_NAME: &str = {:?};\n\
         pub const BOARD_ARCH: &str = {:?};\n\
         pub const KERNEL_BASE_ADDRESS: usize = {:#x};\n\
         pub const KERNEL_ENTRY_ADDRESS: usize = {:#x};\n",
        board.feature, board.architecture, board.kernel_base, board.kernel_base
    );
    write_file(&out_dir.join("board_config.rs"), &rust_config);

    // Keep this file under src/ intentionally: editors can preprocess the LD
    // file without knowing Cargo's target-specific OUT_DIR. It is generated
    // and ignored by Git; the linker build regenerates it every time.
    let header = format!(
        "#ifndef BLUESTAROS_BOARD_CONFIG_H\n\
         #define BLUESTAROS_BOARD_CONFIG_H\n\
         #define BLUESTAROS_BOARD_NAME {:?}\n\
         #define BLUESTAROS_BOARD_ARCH_{} 1\n\
         #define BLUESTAROS_OUTPUT_ARCH {}\n\
         #define BLUESTAROS_KERNEL_BASE_ADDRESS {:#x}\n\
         #define BLUESTAROS_KERNEL_ENTRY_ADDRESS {:#x}\n\
         #endif\n",
        board.feature,
        board.architecture.to_ascii_uppercase(),
        board.output_arch,
        board.kernel_base,
        board.kernel_base
    );
    write_file(&out_dir.join("board_config.h"), &header);
    write_file(&manifest_dir.join("src/board_config.h"), &header);

    let linker_output = out_dir.join("board-linker.ld");
    preprocess_linker(&manifest_dir, &out_dir, board, &linker_output);

    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join(LINKER_SCRIPT).display()
    );
    for candidate in BOARDS {
        println!(
            "cargo:rerun-if-env-changed=CARGO_FEATURE_{}",
            candidate.feature.replace('-', "_").to_ascii_uppercase()
        );
    }
    println!("cargo:rustc-link-arg-bin=os=-T{}", linker_output.display());
}
