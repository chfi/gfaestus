use std::{
    ffi::OsStr,
    io::Result,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn main() {
    if !skip_compiling_shaders() {
        compile_shaders()
    }
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
                    String::from_utf8(output.stdout)
                        .unwrap_or("Failed to print program stdout".to_string())
                );
            } else {
                eprintln!("Shader compilation failed. Status: {}", output.status);
                eprint!(
                    "{}",
                    String::from_utf8(output.stdout)
                        .unwrap_or("Failed to print program stdout".to_string())
                );
                eprint!(
                    "{}",
                    String::from_utf8(output.stderr)
                        .unwrap_or("Failed to print program stderr".to_string())
                );
                panic!("Shader compilation failed. Status: {}", output.status);
            }
        }
        Err(error) => {
            panic!("Failed to compile shader. Cause: {}", error);
        }
    }
}
