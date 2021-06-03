use std::{
    collections::VecDeque,
    ffi::OsStr,
    io::Result,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn main() {
    if Command::new("glslc").output().is_err() {
        eprintln!("Error compiling shaders: 'glslc' not found, do you have the Vulkan SDK installed?");
        eprintln!("Get it at https://vulkan.lunarg.com/");
        std::process::exit(1);
    }

    let shader_files = find_shader_files();

    for path in shader_files.iter() {
        println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
    }

    compile_shaders(&shader_files)
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
                    && path.path().extension() != Some(OsStr::new("glsl"))
                {
                    result.push(path.path());
                }
            });
    }

    result
}

fn compile_shaders(files: &[PathBuf]) {
    for path in files {
        let name = path.file_name().unwrap().to_str().unwrap();
        let output_name = format!("{}.spv", &name);

        let mut output_path = path.clone();
        output_path.pop();
        output_path.push(output_name);

        let result = Command::new("glslc")
            .arg(&path)
            .arg("-o")
            .arg(output_path)
            .output();

        handle_program_result(result);
    }
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
