use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::env;
use std::fs;
use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = env::args().collect();
    match args[1].as_str() {
        "init" => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/master\n").unwrap();
            println!("Initialized git directory")
        }
        "cat-file" if args[2] == "-p" => {
            let path = format!("./.git/objects/{}/{}", &args[3][0..2], &args[3][2..]);
            let bytes = fs::read(path).expect("Could not find the object");
            let mut decoder = ZlibDecoder::new(bytes.as_slice());
            let mut string = String::new();
            decoder
                .read_to_string(&mut string)
                .expect("Could not decode the object");
            print!("{}", get_object_contents(string))
        }
        "hash-object" if args[2] == "-w" => {
            let bytes = fs::read(&args[3]).expect("Could not find the object");
            let hash = write_object(bytes, "blob");

            println!("{}", hash)
        }
        _ => println!("unknown command: {}", args[1]),
    }
}

fn get_object_contents(obj: String) -> String {
    obj[obj.find('\u{0}').expect("Invalid object") + 1..].to_owned()
}

fn get_hex(string: Vec<u8>) -> String {
    use std::fmt::Write;

    let mut sha_one = Sha1::new();
    sha_one.update(string);
    let bytes = sha_one.finalize();
    let mut hash = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(hash, "{:02x}", byte).unwrap();
    }
    hash
}

fn write_object(contents: Vec<u8>, git_type: &str) -> String {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(format!("{} {}\0", git_type, contents.len()).as_bytes())
        .unwrap();
    encoder
        .write_all(contents.as_slice())
        .expect("Could not encode the object");
    let result = encoder.finish().expect("Could not encode the object");
    let hash = get_hex(contents);

    let dir = format!("./.git/objects/{}", &hash[0..2]);
    if fs::metadata(&dir).is_err() {
        fs::create_dir(&dir).expect("Could not create a directory");
    }
    let path = format!("{}/{}", dir, &hash[2..]);
    fs::write(path, result).expect("Could not save the object");
    hash
}
