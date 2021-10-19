use std::env;
use std::fs;
use std::process::Command;
use std::time::SystemTime;

use bytes::Bytes;

use git_error::{GitError, GitResult};
use object::{Contributor, Object, ObjectReference, Sha};
use std::collections::HashMap;

mod git_error;
mod object;
mod pack;
mod parser;
mod remote;
mod zlib;

fn main() -> GitResult<()> {
    let args: Vec<String> = env::args().collect();
    match args[1].as_str() {
        "init" => {
            init(".")?;
            println!("Initialized git directory")
        }
        "cat-file" if args[2] == "-p" => print!("{}", read_object(&args[3])?.content()?),
        "hash-object" if args[2] == "-w" => {
            let bytes = Bytes::from(fs::read(&args[3]).expect("Could not find the object"));
            let hash = write_object(".", &Object::Blob(bytes))?;
            println!("{}", object::to_hex(&hash))
        }
        "commit-tree" if args[3] == "-p" && args[5] == "-m" => {
            let contributor = Contributor {
                name: "Andrei".to_owned(),
                email: "andrei@example.com".to_owned(),
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs(),
                timezone: std::str::from_utf8(&Command::new("date").arg("+%z").output()?.stdout)?
                    .trim_end()
                    .to_owned(),
            };
            let hash = write_object(
                ".",
                &Object::Commit {
                    tree: args[2].clone(),
                    parents: vec![args[4].clone()],
                    author: contributor.clone(),
                    committer: contributor,
                    message: format!("{}\n", args[6]),
                },
            )?;
            println!("{}", object::to_hex(&hash))
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
        "write-tree" => println!("{}", object::to_hex(&write_tree(".", &[".git"])?)),
        "clone" => {
            let git_url = args[2].clone();
            let dir = args[3].clone();
            fs::create_dir(&dir)?;
            init(dir.as_str())?;
            let head = &remote::get_refs(&git_url)?[0].sha;
            let pack_objects = remote::fetch_ref(&git_url, head)?;
            for (_, o) in pack_objects.iter() {
                write_object(dir.as_str(), o)?;
            }
            let head_commit = pack_objects
                .get(head)
                .ok_or(format!("Head ({}) not found in the pack file", head))?;
            let head_tree_sha = match head_commit {
                Object::Commit { tree, .. } => tree,
                _ => {
                    return Err(GitError(format!(
                        "Head ({}) is not pointing to a commit",
                        head
                    )))
                }
            };
            let files = build_tree(
                &pack_objects,
                pack_objects.get(head_tree_sha).ok_or("Tree not found")?,
                vec![dir],
            )?;
            for (path, content) in files {
                // println!("{}", path);
                fs::DirBuilder::new()
                    .recursive(true)
                    .create(path[..path.len() - 1].join("/"))?;
                fs::write(path.join("/"), content)?;
            }
            println!("Done");
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

fn init(prefix: &str) -> GitResult<()> {
    fs::create_dir(format!("{}/{}", prefix, ".git"))?;
    fs::create_dir(format!("{}/{}", prefix, ".git/objects"))?;
    fs::create_dir(format!("{}/{}", prefix, ".git/refs"))?;
    fs::write(
        format!("{}/{}", prefix, ".git/HEAD"),
        "ref: refs/heads/master\n",
    )?;
    Ok(())
}

fn build_tree<'a>(
    objects: &'a HashMap<String, Object>,
    obj: &'a Object,
    prefix: Vec<String>,
) -> GitResult<HashMap<Vec<String>, &'a Bytes>> {
    match obj {
        Object::Blob(content) => {
            let mut res = HashMap::with_capacity(1);
            res.insert(prefix, content);
            Ok(res)
        }
        Object::Tree(refs) => {
            let mut res = HashMap::new();
            for r in refs {
                let inner_tree = build_tree(
                    objects,
                    objects
                        .get(&object::to_hex(&r.hash))
                        .ok_or(format!("Object not found: {}", object::to_hex(&r.hash)))?,
                    prefix.iter().chain(&[r.name.clone()]).cloned().collect(),
                )?;
                res.extend(inner_tree);
            }
            Ok(res)
        }
        Object::Commit { .. } => Err(GitError(String::from("Tree is pointing to a commit"))),
    }
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
            let bytes = Bytes::from(fs::read(&path_buf)?);
            hash = write_object(".", &Object::Blob(bytes))?;
            mode = 100644;
        };

        refs.push(ObjectReference { mode, name, hash })
    }

    refs.sort_by(|a, b| a.name.cmp(&b.name));
    write_object(".", &Object::Tree(refs))
}

fn read_object(sha: &str) -> GitResult<Object> {
    let path = format!("./.git/objects/{}/{}", &sha[0..2], &sha[2..]);
    let bytes = fs::read(path)?;
    let (_, content) = zlib::read(Bytes::from(bytes))?;
    Object::decode(content)
}

fn write_object(root: &str, obj: &Object) -> GitResult<Sha> {
    let (hash, data) = obj.encode();
    let result = zlib::write(&data)?;
    let hex = object::to_hex(&hash);

    let dir = format!("{}/.git/objects/{}", root, &hex[0..2]);
    if fs::metadata(&dir).is_err() {
        fs::create_dir(&dir)?;
    }
    let path = format!("{}/{}", dir, &hex[2..]);
    if fs::metadata(&path).is_err() {
        fs::write(path, result)?;
    }
    Ok(hash)
}
