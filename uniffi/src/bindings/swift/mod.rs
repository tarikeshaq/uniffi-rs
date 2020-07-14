/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    ffi::OsString,
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use anyhow::{anyhow, bail};

pub mod gen_swift;
pub use gen_swift::{BridgingHeader, Config, ModuleMap, SwiftWrapper};

use super::super::interface::ComponentInterface;

pub struct Bindings {
    header: String,
    library: String,
}

pub fn write_bindings(ci: &ComponentInterface, out_dir: &Path) -> Result<()> {
    let out_path = PathBuf::from(out_dir);

    // We're going to generate an "umbrella header" declaration for the swift module,
    // and swift doesn't like having multiple umbrella headers in the same directory.
    // Work around this by creating a subdirectory for each uniffi component.
    // Probably there's a better way to do this...?
    let mut module_dir = out_path.clone();
    module_dir.push(format!("{}.swiftmodule-dir", ci.namespace()));
    fs::create_dir_all(&module_dir)?;

    let mut module_map_file = module_dir.clone();
    module_map_file.push("uniffi.modulemap");

    let mut header_file = module_dir.clone();
    header_file.push(format!("{}-Bridging-Header.h", ci.namespace()));

    let mut source_file = out_path;
    source_file.push(format!("{}.swift", ci.namespace()));

    let Bindings { header, library } = generate_bindings(&ci)?;

    let mut h = File::create(&header_file)?;
    write!(h, "{}", header)?;

    let mut m = File::create(&module_map_file)?;
    write!(m, "{}", generate_module_map(&ci, &header_file)?)?;

    let mut l = File::create(&source_file)?;
    write!(l, "{}", library)?;

    Ok(())
}

/// Generate Swift bindings for the given ComponentInterface, as a string.
pub fn generate_bindings(ci: &ComponentInterface) -> Result<Bindings> {
    let config = Config::from(&ci);
    use askama::Template;
    let header = BridgingHeader::new(&config, &ci)
        .render()
        .map_err(|_| anyhow!("failed to render Swift bridging header"))?;
    let library = SwiftWrapper::new(&config, &ci)
        .render()
        .map_err(|_| anyhow!("failed to render Swift library"))?;
    Ok(Bindings { header, library })
}

fn generate_module_map(ci: &ComponentInterface, header_path: &Path) -> Result<String> {
    use askama::Template;
    let module_map = ModuleMap::new(&ci, header_path)
        .render()
        .map_err(|_| anyhow!("failed to render Swift module map"))?;
    Ok(module_map)
}

/// ...
pub fn compile_bindings(ci: &ComponentInterface, out_dir: &Path) -> Result<()> {
    let out_path = PathBuf::from(out_dir);

    let mut module_map_file = out_path.clone();
    module_map_file.push(format!("{}.swiftmodule-dir", ci.namespace()));
    module_map_file.push("uniffi.modulemap");

    let mut module_map_file_option = OsString::from("-fmodule-map-file=");
    module_map_file_option.push(module_map_file.as_os_str());

    let mut source_file = out_path.clone();
    source_file.push(format!("{}.swift", ci.namespace()));

    let mut dylib_file = out_path.clone();
    dylib_file.push(format!("lib{}.dylib", ci.namespace()));

    // `-emit-library -o <path>` generates a `.dylib`, so that we can use the
    // Swift module from the REPL. Otherwise, we'll get "Couldn't lookup
    // symbols" when we try to import the module.
    // See https://bugs.swift.org/browse/SR-1191.

    let status = std::process::Command::new("swiftc")
        .arg("-module-name")
        .arg(ci.namespace())
        .arg("-emit-library")
        .arg("-o")
        .arg(&dylib_file)
        .arg("-emit-module")
        .arg("-emit-module-path")
        .arg(&out_path)
        .arg("-parse-as-library")
        .arg("-L")
        .arg(&out_path)
        .arg(format!("-luniffi_{}", ci.namespace()))
        .arg("-Xcc")
        .arg(module_map_file_option)
        .arg(source_file)
        .spawn()?
        .wait()?;
    if !status.success() {
        bail!("running `swiftc` failed")
    }
    Ok(())
}

pub fn run_script(out_dir: Option<&Path>, script_file: Option<&Path>) -> Result<()> {
    let mut cmd = std::process::Command::new("swift");

    // Find any module maps and/or dylibs in the target directory, and tell swift to use them.
    if let Some(out_dir) = out_dir {
        cmd.arg("-I").arg(out_dir).arg("-L").arg(out_dir);
        for entry in PathBuf::from(out_dir).read_dir()? {
            let entry = entry?;
            if let Some(ext) = entry.path().extension() {
                if ext == "swiftmodule-dir" {
                    let mut module_map_file = PathBuf::from(entry.path());
                    module_map_file.push("uniffi.modulemap");
                    let mut option = OsString::from("-fmodule-map-file=");
                    option.push(module_map_file);
                    cmd.arg("-Xcc");
                    cmd.arg(option);
                } else if ext == "dylib" || ext == "so" {
                    let mut option = OsString::from("-l");
                    option.push(entry.path());
                    cmd.arg(option);
                }
            }
        }
    }

    if let Some(script) = script_file {
        cmd.arg(script);
    }

    if !cmd.spawn()?.wait()?.success() {
        bail!("running `swift` failed")
    }
    Ok(())
}
