extern crate bindgen;
extern crate fs_extra;
extern crate num_cpus;
extern crate target_lexicon;
#[cfg(target_env = "msvc")]
extern crate vswhere;

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{self, Path};
use std::process::{Command, Output};
use std::str::FromStr;

use fs_extra::dir;
use fs_extra::error::Result as FsResult;

use target_lexicon::Triple;

/// Prints stuff about error
fn handle_err<A: AsRef<str>>(o: io::Result<Output>, cmd: A) -> Output {
    let o = match o {
        Err(e) => {
            println!("{}", cmd.as_ref());
            println!("\tIO Error on exec:\n{:?}", e);
            ::std::process::exit(1);
        }
        Ok(o) => o,
    };
    if !o.status.success() {
        let stderr = String::from_utf8_lossy(o.stderr.as_slice());
        let stdout = String::from_utf8_lossy(o.stdout.as_slice());
        println!("{}", cmd.as_ref());
        match o.status.code() {
            Option::Some(x) => println!("\tExit Code: {:?}", x),
            _ => {}
        };
        println!("\tStdErr:\n {}", stderr);
        println!("\tStdOut:\n {}", stdout);
        ::std::process::exit(1);
    }
    o
}

const BINDGEN_JOBS: &'static [(&'static str, &'static str)] = &[
    (
        "xed/include/public/xed/xed-interface.h",
        "../xed_interface.rs",
    ),
    ("xed/include/public/xed/xed-version.h", "../xed_version.rs"),
];

fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    match fs::create_dir(path) {
        Err(e) => match e.kind() {
            io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(e),
        },
        x => x,
    }
}

fn overwrite_dir<P1: AsRef<Path>, P2: AsRef<Path>>(src: P1, dest: P2) -> FsResult<u64> {
    dir::copy(
        src,
        dest,
        &dir::CopyOptions {
            overwrite: true,
            skip_exist: true,
            copy_inside: true,
            ..dir::CopyOptions::new()
        },
    )
}

/// Autogenerates bindings
fn build_bindings() {
    for job in BINDGEN_JOBS {
        let dot_h = job.0;
        let dot_rs = job.1;
        let bindings = match bindgen::Builder::default()
            .clang_arg("--include-directory=xed/obj")
            .clang_arg("--include-directory=xed/include/public/xed")
            .clang_arg("-DXED_ENCODER")
            .header(dot_h)
            .generate()
        {
            Ok(x) => x,
            Err(e) => panic!("Could not generate bindings for {}. Error {:?}", dot_h, e),
        };
        match bindings.write_to_file(dot_rs) {
            Ok(_) => {}
            Err(e) => panic!(
                "Could not write generated bindings to {}. Error {:?}",
                dot_rs, e
            ),
        };
    }
}

#[cfg(target_env = "msvc")]
fn add_msvc_arg(cmd: &mut Command) -> Result<&mut Command, Box<Error>> {
    // Should include preview versions in this but vswhere
    // currently panics on those
    let instinfos = vswhere::Config::new().run_default_path()?;

    for inst in instinfos {
        if inst.installation_version().major() == 15 {
            let mut path = inst.installation_path();
            return Ok(cmd.arg(format!("--vc-dir={}", path.join("VC").to_str().unwrap())));
        }
    }

    println!("cargo:warning=Unable to find a non-preview version of MSVC, this may cause compilation failures.");

    Ok(cmd)
}
#[cfg(not(target_env = "msvc"))]
fn add_msvc_arg(cmd: &mut Command) -> Result<&mut Command, Box<Error>> {
    Ok(cmd)
}

/// Build script entry point
fn main() -> Result<(), Box<Error>> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let triple = Triple::from_str(&env::var("TARGET").unwrap()).unwrap();

    // linker directory
    let current_dir = env::current_dir().expect("Could not fetch current directory");
    let lib_dir = {
        let mut x = current_dir.clone();
        x.push("xed");
        x.push("build");
        x.push("obj");
        x
    };
    let xed_dir = {
        let mut x = current_dir.clone();
        x.push("xed");
        x
    };
    let mbuild_dir = {
        let mut x = current_dir.clone();
        x.push("mbuild");
        x
    };

    let mut new_dir = path::PathBuf::from(&out_dir);
    new_dir.push("xed-build");
    //dir::remove(new_dir.clone()).err();
    create_dir(new_dir.clone())?;

    new_dir.push("mbuild");
    if !new_dir.exists() {
        overwrite_dir(mbuild_dir, new_dir.clone())?;
    }

    new_dir.pop();
    new_dir.push("xed");
    if !new_dir.exists() {
        overwrite_dir(xed_dir, new_dir.clone())?;
    }

    new_dir.pop();
    env::set_current_dir(new_dir.clone())?;

    // Ignore changes in all other files except build.rs
    println!("cargo:rerun-if-changed=build.rs");

    // Build the project
    let output = add_msvc_arg(Command::new("python").arg("mfile.py"))?
        .arg(format!("--jobs={}", 8))
        .arg("--silent")
        .arg("--static-stripped")
        //.arg("--extra-ccflags=-fPIC")
        .arg("--opt=3")
        .arg("--no-werror")
        //.arg(format!("--toolchain={}", toolchain))
        .arg(format!("--host-cpu={}", triple.architecture))
        .current_dir("xed")
        .output();
    handle_err(output, "Failed to run `mfile.py`");

    // Configure linker
    println!(
        "cargo:rustc-link-search=native={}",
        lib_dir.to_string_lossy()
    );
    println!("cargo:rustc-link-lib=static=xed");

    // auto generate bindings
    build_bindings();

    Ok(())
}
