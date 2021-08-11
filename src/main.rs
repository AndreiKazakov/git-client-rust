use flate2::read::ZlibDecoder;
use std::env;
use std::fs;
use std::io::Read;

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
            println!("{}", get_object_contents(string))
        }
        _ => println!("unknown command: {}", args[1]),
    }
}

fn get_object_contents(obj: String) -> String {
    obj[obj.find('\u{0}').expect("Invalid object") + 1..obj.len() - 2].to_owned()
}
