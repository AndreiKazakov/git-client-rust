use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::env;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::io::{Read, Write};

fn main() -> Result<(), GitError> {
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
            println!("{}", hash)
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

enum Object {
    Blob(Vec<u8>),
}

impl Object {
    fn content(&self) -> Result<String, GitError> {
        match self {
            Self::Blob(bytes) => Ok(std::str::from_utf8(bytes)?.to_owned()),
        }
    }

    fn encode(&self) -> Vec<u8> {
        match self {
            Self::Blob(bytes) => {
                let mut res = Vec::new();
                res.extend_from_slice(b"blob ");
                res.extend_from_slice(bytes.len().to_string().as_bytes());
                res.push(b'\0');
                res.extend(bytes);
                res
            }
        }
    }

    fn decode(bytes: Vec<u8>) -> Result<Self, GitError> {
        match &bytes[0..4] {
            b"blob" => Ok(Object::Blob(
                bytes
                    .into_iter()
                    .skip_while(|c| *c != b'\0')
                    .skip(1)
                    .collect(),
            )),
            _ => Err(GitError(format!(
                "Unsupported object type: {}",
                std::str::from_utf8(
                    &bytes
                        .into_iter()
                        .take_while(|c| *c != b' ')
                        .collect::<Vec<u8>>()
                )?
            ))),
        }
    }
}

struct ObjectReference {
    mode: u16,
    name: String,
    hash: Vec<u8>,
}

fn read_object(sha: &str) -> Result<Object, GitError> {
    let path = format!("./.git/objects/{}/{}", &sha[0..2], &sha[2..]);
    let bytes = fs::read(path)?;
    let mut decoder = ZlibDecoder::new(bytes.as_slice());
    let mut content = Vec::new();
    decoder.read_to_end(&mut content)?;
    Object::decode(content)
}

fn get_hex(string: Vec<u8>) -> Result<String, GitError> {
    use std::fmt::Write;

    let mut sha_one = Sha1::new();
    sha_one.update(string);
    let bytes = sha_one.finalize();
    let mut hash = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(hash, "{:02x}", byte)?;
    }
    Ok(hash)
}

fn write_object(obj: Object) -> Result<String, GitError> {
    let data = obj.encode();
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data.as_slice())?;
    let result = encoder.finish()?;
    let hash = get_hex(data)?;

    let dir = format!("./.git/objects/{}", &hash[0..2]);
    if fs::metadata(&dir).is_err() {
        fs::create_dir(&dir)?;
    }
    let path = format!("{}/{}", dir, &hash[2..]);
    fs::write(path, result)?;
    Ok(hash)
}

struct GitError(String);

impl Debug for GitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<A: ToString> From<A> for GitError {
    fn from(a: A) -> Self {
        GitError(a.to_string())
    }
}
