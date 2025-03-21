use core::fmt;
use std::process::Command;
use minijinja::{context, path_loader, Environment};
use std::fmt::Display;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::{env, io, process};

const OUTPUT_DIRNAME: &'static str = "dist";
const JS_DIRNAME: &'static str = "javascript";

#[derive(Debug)]
enum Error {
    Io(io::Error),
    Minijinja(minijinja::Error),
    StripPrefix(std::path::StripPrefixError),
    Npm,
    Unexpected(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Minijinja(e) => write!(f, "Minijinja error: {e}"),
            Self::StripPrefix(e) => write!(f, "StripPrefixError: {e}"),
            Self::Npm => write!(f, "Error when running the 'npm' command"),
            Self::Unexpected(e) => write!(f, "Unexpected error: {e}"),
        }
    }
}

#[derive(Debug)]
enum InputPath {
    /// An HTML template to be rendered using minijinja into
    /// the output dir
    HtmlTemplate(PathBuf),
    /// A file that to be copied into the output dir as it is
    File(PathBuf),
    /// A directory to be copied into the output dir as it is,
    /// recursively
    Dir(PathBuf),
    /// A javascript module to be built using `npm` and the built
    /// bundle to be copied into the output dir
    JsModule(PathBuf),
}

fn is_html_template(filename: &str) -> bool {
    filename.ends_with(".html.jinja")
}

// @TODO: Allow user specified exclusions
fn must_skip(filename: &str) -> bool {
    filename.starts_with(".git")       // the git repo, .gitignore etc. files
        || filename == OUTPUT_DIRNAME  // the output directory
        || filename.ends_with('~')     // emacs backup files
        || filename.ends_with('#')     // emacs tmp files
        || filename.starts_with('_')   // included jinja templates
}

/// Checks if a path is a javascript module that needs to be built.
///
/// The criteria is that the directory must be named `JS_DIRNAME` and
/// must contain package.json file inside it.
fn is_js_module(path: &Path) -> Result<bool, Error> {
    let filename = path
        .file_name()
        .ok_or(Error::Unexpected(format!("No filename: {}", path.display())))?;
    if filename != JS_DIRNAME {
        return Ok(false)
    }
    let pkg_json_exists = path
        .join("package.json")
        .try_exists()
        .map_err(Error::Io)?;
    Ok(pkg_json_exists)
}



fn get_input_files(base_dir: &Path) -> Result<Vec<InputPath>, Error> {
    let mut result = vec![];
    for member in fs::read_dir(base_dir).map_err(Error::Io)? {
        let entry = member.map_err(Error::Io)?;
        let filename = entry.file_name();
        // Assuming filenames are valid utf-8
        let filename_lossy = filename.to_string_lossy();
        if must_skip(&filename_lossy) {
            // @TODO: Replace with a log line
            // println!("Ignoring entry: {entry:?}");
            continue;
        }
        if is_html_template(&filename_lossy) {
            result.push(InputPath::HtmlTemplate(entry.path()));
        } else {
            let filetype = entry.file_type().map_err(Error::Io)?;
            if filetype.is_dir() {
                if is_js_module(&entry.path())? {
                    result.push(InputPath::JsModule(entry.path()));
                } else {
                    for nested_file in get_input_files(&entry.path())? {
                        result.push(nested_file);
                    }
                }
            } else if filetype.is_file() {
                result.push(InputPath::File(entry.path()));
            } else if filetype.is_symlink() {
                let target = entry.path().canonicalize().map_err(Error::Io)?;
                // @NOTE: Here we're checking whether the symlink
                // target is a file or a dir, but the original symlink
                // itself is being added to the result. That is
                // because at the time of copying files, we need the
                // symlink path to be able to find path relative to
                // the src/input dir.
                if target.is_file() {
                    result.push(InputPath::File(entry.path()));
                } else if target.is_dir() {
                    result.push(InputPath::Dir(entry.path()));
                } else {
                    panic!("unexpected condition met");
                }
            }
        }
    }
    Ok(result)
}

fn ensure_dir(dir: &Path) -> Result<(), io::Error> {
    match dir.try_exists() {
        Ok(true) => Ok(()),
        Ok(false) => fs::create_dir(dir),
        Err(e) => Err(e),
    }
}

fn ensure_parent_dir(path: &Path) -> Result<(), Error> {
    let parent = path.parent().ok_or(Error::Unexpected(format!(
        "Parent dir could not be found: {}",
        path.display()
    )))?;
    ensure_dir(parent).map_err(Error::Io)
}

fn to_output_path(src_dir: &Path, output_dir: &Path, input_path: &Path) -> Result<PathBuf, Error> {
    let rel_path = input_path
        .strip_prefix(src_dir)
        .map_err(Error::StripPrefix)?;
    let output_path = match rel_path.extension() {
        Some(ext) => {
            if ext == "jinja" {
                // @SAFE: use of unwrap as the error conditions are
                // not possible in this block
                output_dir
                    .join(rel_path.parent().unwrap())
                    .join(rel_path.file_stem().unwrap())
            } else {
                output_dir.join(rel_path)
            }
        }
        None => output_dir.join(rel_path),
    };
    Ok(output_path)
}

fn init_jinja_env(templates_dir: &Path) -> Environment {
    let mut env = Environment::new();
    env.set_loader(path_loader(templates_dir));
    env
}

fn render_page(
    env: &Environment,
    path: &Path,
    output_dir: &Path,
    src_dir: &Path,
) -> Result<(), Error> {
    let output_path = to_output_path(src_dir, output_dir, path)?;
    ensure_parent_dir(&output_path)?;
    let mut output_file = File::create(output_path).map_err(Error::Io)?;
    let tmpl_path = path.strip_prefix(src_dir)
        .map_err(Error::StripPrefix)?
        .to_string_lossy();
    let tmpl = env.get_template(&tmpl_path).map_err(Error::Minijinja)?;
    tmpl.render_to_write(context!(), &mut output_file)
        .map_err(Error::Minijinja)?;
    println!("Rendered template to file: {output_file:?}");
    Ok(())
}

fn copy_dir_recursive(path: &Path, output_dir: &Path, src_dir: &Path) -> Result<(), Error> {
    let dst = to_output_path(src_dir, output_dir, path)?;
    ensure_parent_dir(&dst)?;
    // @TODO: Remove the following after confirmation
    // fs::create_dir_all(&dst).map_err(Error::Io)?;
    for entry in fs::read_dir(path).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        let ty = entry.file_type().map_err(Error::Io)?;
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst, src_dir)?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name())).map_err(Error::Io)?;
        }
    }
    println!("Copied dir recursively: {}", dst.display());
    Ok(())
}

fn copy_file(path: &Path, output_dir: &Path, src_dir: &Path) -> Result<(), Error> {
    let dst = to_output_path(src_dir, output_dir, path)?;
    ensure_parent_dir(&dst)?;
    fs::copy(path, &dst).map_err(Error::Io)?;
    println!("Copied file: {}", dst.display());
    Ok(())
}

/// Builds a js module inside the source dir by shelling out to
/// `npm`. Assumes that:
///
///   1. the js file is built inside a subdir named `public`
///   2. the bundle file is named `main.js` and
///   3. has an associated source map `main.js.map`
fn build_js_module(path: &Path, output_dir: &Path) -> Result<(), Error> {
    println!("Executing command: npm run build");
    let status = Command::new("npm")
        .current_dir(path)
        .args(["run", "build"])
        .status()
        .map_err(Error::Io)?;
    if status.success() {
        let output_js_dir = output_dir.join(JS_DIRNAME);
        ensure_dir(&output_js_dir).map_err(Error::Io)?;
        for filename in vec!["main.js", "main.js.map"] {
            let src = path.join("public").join(filename);
            let dst = output_dir.join(JS_DIRNAME).join(filename);
            fs::copy(src, &dst).map_err(Error::Io)?;
            println!("Copied built js file: {}", dst.display());
        }
        Ok(())
    } else {
        Err(Error::Npm)
    }
}

fn generate_site(src_dir: &Path) -> Result<(), Error> {
    let output_dir = src_dir.join(OUTPUT_DIRNAME);
    ensure_dir(&output_dir).map_err(Error::Io)?;
    let env = init_jinja_env(src_dir);
    let input_files = get_input_files(&Path::new(src_dir))?;
    for file in input_files {
        match file {
            InputPath::HtmlTemplate(path) => render_page(&env, &path, &output_dir, &src_dir)?,
            InputPath::File(path) => copy_file(&path, &output_dir, &src_dir)?,
            InputPath::Dir(path) => copy_dir_recursive(&path, &output_dir, &src_dir)?,
            InputPath::JsModule(path) => build_js_module(&path, &output_dir)?,
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let src = Path::new(&args[1]);
    match generate_site(src) {
        Ok(_) => process::exit(0),
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}
