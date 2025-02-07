use minijinja::{context, path_loader, Environment};
use std::path::{Path, PathBuf};
use std::io;
use std::fs::{self, File};

#[allow(unused)]
#[derive(Debug)]
enum Error {
    Io(io::Error),
}

#[allow(unused)]
#[derive(Debug)]
enum InputFile {
    Page(PathBuf),
    File(PathBuf),
    Dir(PathBuf),
}

fn is_page(filename: &str) -> bool {
    filename.ends_with(".html.jinja")
}

// @TODO: Allow user specified exclusions
fn must_skip(filename: &str) -> bool {
    filename == ".git" || filename == "dist" || filename.ends_with('~') || filename.starts_with('_')
}

fn get_input_files(base_dir: &Path) -> Result<Vec<InputFile>, Error> {
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
        if is_page(&filename_lossy) {
            result.push(InputFile::Page(entry.path()));
        } else {
            let filetype = entry.file_type().map_err(Error::Io)?;
            if filetype.is_dir() {
                result.push(InputFile::Dir(entry.path()));
            } else if filetype.is_file() {
                result.push(InputFile::File(entry.path()));
            } else {
                // It's a symlink. Not sure how to handle it so skip
                // it for now
                continue;
            }
        }
    }
    Ok(result)
}

fn ensure_output_dir(dir: &Path) -> Result<(), Error> {
    match dir.try_exists() {
        Ok(true) => Ok(()),
        Ok(false) => fs::create_dir(dir).map_err(Error::Io),
        Err(e) => Err(Error::Io(e)),
    }
}

fn to_output_path(output_dir: &Path, input_path: &Path) -> PathBuf {
    output_dir.join(input_path.file_stem().unwrap())
}

fn init_jinja_env(templates_dir: &Path) -> Environment {
    let mut env = Environment::new();
    env.set_loader(path_loader(templates_dir));
    env
}

fn render_page(env: &Environment, output_dir: &Path, file: &Path) -> Result<(), Error> {
    let output_path = to_output_path(output_dir, &file);
    let mut output_file = File::create(output_path).map_err(Error::Io)?;
    let filename = file.file_name().unwrap().to_string_lossy();
    let tmpl = env.get_template(&filename).unwrap();
    tmpl.render_to_write(context!(), &mut output_file).unwrap();
    println!("Rendered template to file: {output_file:?}");
    Ok(())
}

fn copy_dir_recursive(src: &Path, output_dir: &Path) -> io::Result<()> {
    let dir_name = src.file_name().unwrap();
    let dst = output_dir.join(dir_name);
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst)?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    println!("Copied dir recursively: {}", dst.display());
    Ok(())
}

fn copy_file(src: &Path, output_dir: &Path) -> io::Result<()> {
    let filename = src.file_name().unwrap();
    let dst = output_dir.join(filename);
    fs::copy(src, &dst)?;
    println!("Copied file: {}", dst.display());
    Ok(())
}

fn main() -> Result<(), Error> {
    let input_dir = Path::new("/home/vineet/code/metropolis/website");
    let output_dir = Path::new("/home/vineet/code/metropolis/website/dist");
    ensure_output_dir(output_dir)?;
    let env = init_jinja_env(input_dir);
    let input_files = get_input_files(&Path::new(input_dir))?;
    for file in input_files {
        match file {
            InputFile::Page(path) => render_page(&env, &output_dir, &path)?,
            InputFile::File(path) => copy_file(&path, &output_dir).map_err(Error::Io)?,
            InputFile::Dir(path) => copy_dir_recursive(&path, &output_dir).map_err(Error::Io)?,
        }
    }
    Ok(())
}
