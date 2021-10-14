use std::env;
use std::fs;
use std::io::{Read, Write};
use std::process::Command;
use std::time::SystemTime;

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};

use git_error::{GitError, GitResult};
use object::{Contributer, Object, ObjectReference, Sha};

mod git_error;
mod object;
mod parser;

fn main() -> GitResult<()> {
    let args: Vec<String> = env::args().collect();
    match args[1].as_str() {
        "init" => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/master\n")?;
            println!("Initialized git directory")
        }
        "cat-file" if args[2] == "-p" => print!("{}", read_object(&args[3])?.content()?),
        "hash-object" if args[2] == "-w" => {
            let bytes = fs::read(&args[3]).expect("Could not find the object");
            let hash = write_object(Object::Blob(bytes))?;
            println!("{}", to_hex(&hash)?)
        }
        "commit-tree" if args[3] == "-p" && args[5] == "-m" => {
            let contributer = Contributer {
                name: "Andrei".to_owned(),
                email: "andrei@example.com".to_owned(),
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs(),
                timezone: std::str::from_utf8(&Command::new("date").arg("+%z").output()?.stdout)?
                    .trim_end()
                    .to_owned(),
            };
            let hash = write_object(Object::Commit {
                tree: args[2].clone(),
                parents: vec![args[4].clone()],
                author: contributer.clone(),
                committer: contributer,
                message: format!("{}\n", args[6]),
            })?;
            println!("{}", to_hex(&hash)?)
        }
        "ls-tree" if args[2] == "--name-only" => match read_object(&args[3])? {
            Object::Tree(refs) => println!(
                "{}",
                refs.iter()
                    .map(|r| &*r.name)
                    .collect::<Vec<&str>>()
                    .join("\n")
            ),
            _ => return Err(GitError("Not a tree".to_owned())),
        },
        "write-tree" => println!("{}", to_hex(&write_tree(".", &[".git"])?)?),
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

fn write_tree(path: &str, ignore: &[&str]) -> GitResult<Sha> {
    let mut refs = Vec::new();

    for f in fs::read_dir(path)? {
        let path_buf = f?.path();
        let name = path_buf
            .file_name()
            .ok_or("Could not get a file path")?
            .to_str()
            .ok_or("Could not get a file path")?
            .to_owned();
        if ignore.contains(&&*name) {
            continue;
        }
        let hash;
        let mode;

        if path_buf.is_dir() {
            hash = write_tree(
                path_buf.to_str().ok_or("Could not get a file path")?,
                &ignore,
            )?;
            mode = 40000;
        } else {
            let bytes = fs::read(&path_buf)?;
            hash = write_object(Object::Blob(bytes))?;
            mode = 100644;
        };

        refs.push(ObjectReference { mode, name, hash })
    }

    refs.sort_by(|a, b| a.name.cmp(&b.name));
    write_object(Object::Tree(refs))
}

fn read_object(sha: &str) -> GitResult<Object> {
    let path = format!("./.git/objects/{}/{}", &sha[0..2], &sha[2..]);
    let bytes = fs::read(path)?;
    let mut decoder = ZlibDecoder::new(bytes.as_slice());
    let mut content = Vec::new();
    decoder.read_to_end(&mut content)?;
    Object::decode(content)
}

fn get_sha(string: &[u8]) -> GitResult<Sha> {
    let mut sha_one = Sha1::new();
    sha_one.update(string);
    let bytes = sha_one.finalize();
    let mut sha = [0u8; 20];
    sha[..20].copy_from_slice(&bytes);
    Ok(sha)
}

fn to_hex(bytes: &Sha) -> GitResult<String> {
    use std::fmt::Write;

    let mut hash = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter() {
        write!(hash, "{:02x}", byte)?;
    }
    Ok(hash)
}

fn write_object(obj: Object) -> GitResult<Sha> {
    let data = obj.encode();
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data)?;
    let result = encoder.finish()?;
    let hash = get_sha(&data)?;
    let hex = to_hex(&hash)?;

    let dir = format!("./.git/objects/{}", &hex[0..2]);
    if fs::metadata(&dir).is_err() {
        fs::create_dir(&dir)?;
    }
    let path = format!("{}/{}", dir, &hex[2..]);
    if fs::metadata(&path).is_err() {
        fs::write(path, result)?;
    }
    Ok(hash)
}
