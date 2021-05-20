use std::{
    collections::VecDeque,
    ffi::OsStr,
    io::Result,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn main() {
    let shader_files = find_shader_files();

    for path in shader_files.iter() {
        println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
    }

    if !skip_compiling_shaders() {
        compile_shaders()
    }
}

fn find_shader_files() -> Vec<PathBuf> {
    let shader_dir_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");

    let mut result: Vec<PathBuf> = Vec::new();

    let mut directories: VecDeque<PathBuf> = VecDeque::new();
    directories.push_back(shader_dir_path.clone());

    while let Some(dir) = directories.pop_front() {
        std::fs::read_dir(dir)
            .unwrap()
            .map(Result::unwrap)
            .for_each(|path| {
                if path.file_type().unwrap().is_dir() {
                    directories.push_back(path.path());
                } else if path.file_type().unwrap().is_file()
                    && path.path().extension() != Some(OsStr::new("spv"))
                {
                    result.push(path.path());
                }
            });
    }

    result
}

fn skip_compiling_shaders() -> bool {
    if let Ok(v) = std::env::var("SKIP_SHADER_COMPILATION") {
        v.parse::<bool>().unwrap_or(false)
    } else {
        false
    }
}

fn compile_shaders() {
    println!("Compiling shaders...");

    let shader_dir_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");

    std::fs::read_dir(shader_dir_path.clone())
        .unwrap()
        .map(Result::unwrap)
        .filter(|dir| {
            dir.file_type().unwrap().is_file() && dir.path().extension() != Some(OsStr::new("spv"))
        })
        .for_each(|file| {
            let path = file.path();
            let name = path.file_name().unwrap().to_str().unwrap();
            let output_name = format!("{}.spv", &name);

            if Command::new("glslangValidator").output().is_err() {
                eprintln!("Error compiling shaders: 'glslangValidator' not found, do you have the Vulkan SDK installed?");
                eprintln!("Get it at https://vulkan.lunarg.com/");
                std::process::exit(1);
            }

            let result = Command::new("glslangValidator")
                .current_dir(&shader_dir_path)
                .arg("-V")
                .arg(&path)
                .arg("-o")
                .arg(output_name)
                .output();

            handle_program_result(result);
        })
}

fn handle_program_result(result: Result<Output>) {
    match result {
        Ok(output) => {
            if output.status.success() {
                println!("Shader compilation succedeed.");
                print!(
                    "{}",
                    String::from_utf8(output.stdout).unwrap_or(
                        "Failed to print program stdout".to_string()
                    )
                );
            } else {
                eprintln!(
                    "Shader compilation failed. Status: {}",
                    output.status
                );
                eprint!(
                    "{}",
                    String::from_utf8(output.stdout).unwrap_or(
                        "Failed to print program stdout".to_string()
                    )
                );
                eprint!(
                    "{}",
                    String::from_utf8(output.stderr).unwrap_or(
                        "Failed to print program stderr".to_string()
                    )
                );
                panic!("Shader compilation failed. Status: {}", output.status);
            }
        }
        Err(error) => {
            panic!("Failed to compile shader. Cause: {}", error);
        }
    }
}
