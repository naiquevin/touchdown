use minijinja::{context, path_loader, Environment};
use std::path::{Path, PathBuf};
use std::io;
use std::fs::{self, DirEntry, File};

#[derive(Debug)]
enum Error {
    Io(io::Error),
}

fn is_page(entry: &DirEntry) -> bool {
    let filename = entry.file_name();
    // Assuming filenames are valid utf-8
    let filename_lossy = filename.to_string_lossy();
    !filename_lossy.starts_with('_') && filename_lossy.ends_with(".html.jinja")
}

fn get_pages(base_dir: &Path) -> Vec<PathBuf> {
    let mut pages = vec![];
    for member in fs::read_dir(base_dir).unwrap() {
        let entry = member.unwrap();
        if is_page(&entry) {
            pages.push(entry.path());
        }
    }
    pages
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

fn main() -> Result<(), Error> {
    let input_dir = Path::new("/home/vineet/code/metropolis/website");
    let output_dir = Path::new("/home/vineet/code/metropolis/website/dist");
    ensure_output_dir(output_dir)?;
    let env = init_jinja_env(input_dir);
    let pages = get_pages(&Path::new(input_dir));
    for file in pages {
        let output_path = to_output_path(output_dir, &file);
        let mut output_file = File::create(output_path).map_err(Error::Io)?;
        let filename = file.file_name().unwrap().to_string_lossy();
        let tmpl = env.get_template(&filename).unwrap();
        tmpl.render_to_write(context!(), &mut output_file).unwrap();
        println!("Output file: {output_file:?}");
    }
    Ok(())
}
