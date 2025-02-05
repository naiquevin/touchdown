use minijinja::{context, path_loader, Environment};
use std::path::{Path, PathBuf};
use std::io;
use std::fs::{self, DirEntry, File};

#[allow(unused)]
#[derive(Debug)]
enum Error {
    Io(io::Error),
}

#[allow(unused)]
enum InputFile {
    Page(PathBuf),
    File(PathBuf),
    Dir(PathBuf),
}

fn is_page(entry: &DirEntry) -> bool {
    let filename = entry.file_name();
    // Assuming filenames are valid utf-8
    let filename_lossy = filename.to_string_lossy();
    !filename_lossy.starts_with('_') && filename_lossy.ends_with(".html.jinja")
}

// @TODO: Allow user specified exclusions
fn is_ignorable(entry: &DirEntry) -> bool {
    let filename = entry.file_name();
    // Assuming filenames are valid utf-8
    let filename_lossy = filename.to_string_lossy();
    filename_lossy == ".git" || filename_lossy == "dist" || filename_lossy.ends_with('~')
}

fn get_input_files(base_dir: &Path) -> Result<Vec<InputFile>, Error> {
    let mut pages = vec![];
    for member in fs::read_dir(base_dir).map_err(Error::Io)? {
        let entry = member.map_err(Error::Io)?;
        if is_ignorable(&entry) {
            // @TODO: Replace with a log line
            // println!("Ignoring entry: {entry:?}");
            continue;
        }
        if is_page(&entry) {
            pages.push(InputFile::Page(entry.path()));
        }
    }
    Ok(pages)
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
    println!("Output file: {output_file:?}");
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
            InputFile::File(_) => todo!(),
            InputFile::Dir(_) => todo!(),
        }
    }
    Ok(())
}
