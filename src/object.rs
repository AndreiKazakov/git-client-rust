use bytes::Bytes;
use sha1::{Digest, Sha1};

use crate::git_error::{GitError, GitResult};
use crate::parser::{parse_string_until, take_until};

pub type Sha = [u8; 20];

#[derive(Debug)]
pub enum Object {
    Blob(Bytes),
    Tree(Vec<ObjectReference>),
    Commit {
        tree: String,
        parents: Vec<String>,
        author: Contributor,
        committer: Contributor,
        message: String,
    },
}

#[derive(Debug)]
pub struct ObjectReference {
    pub mode: usize,
    pub name: String,
    pub hash: Sha,
}

#[derive(Debug, Clone)]
pub struct Contributor {
    pub name: String,
    pub email: String,
    pub timestamp: u64,
    pub timezone: String,
}

impl Object {
    pub fn content(&self) -> GitResult<String> {
        match self {
            Self::Blob(bytes) => Ok(std::str::from_utf8(bytes)?.to_owned()),
            Self::Tree(refs) => {
                let mut res = String::new();
                for r in refs {
                    res.push_str(&format!(
                        "{:0>6} {} {}    {}",
                        r.mode,
                        if r.mode.to_string().starts_with('1') {
                            "blob"
                        } else {
                            "tree"
                        },
                        to_hex(&r.hash),
                        r.name
                    ));
                    res.push('\n')
                }
                Ok(res)
            }
            Self::Commit {
                tree,
                parents,
                author,
                committer,
                message,
            } => {
                let mut content = String::new();

                content.push_str(&format!("tree {}\n", tree));

                for parent in parents {
                    content.push_str(&format!("parent {}\n", parent));
                }

                content.push_str(&format!(
                    "author {} <{}> {} {}\n",
                    author.name, author.email, author.timestamp, author.timezone
                ));
                content.push_str(&format!(
                    "committer {} <{}> {} {}\n",
                    committer.name, committer.email, committer.timestamp, committer.timezone
                ));

                content.push('\n');
                content.push_str(&message);
                Ok(content)
            }
        }
    }

    pub fn encode(&self) -> (Sha, Bytes) {
        match self {
            Self::Blob(bytes) => {
                let mut res = Vec::new();
                res.extend_from_slice(b"blob ");
                res.extend_from_slice(bytes.len().to_string().as_bytes());
                res.push(b'\0');
                res.extend(bytes);
                (get_sha(&res), Bytes::from(res))
            }
            Self::Tree(refs) => {
                let mut res = Vec::new();
                res.extend_from_slice(b"tree ");
                let mut content = Vec::new();
                for r in refs {
                    content.extend_from_slice(r.mode.to_string().as_bytes());
                    content.push(b' ');
                    content.extend_from_slice(r.name.as_bytes());
                    content.push(b'\0');
                    content.extend(&r.hash);
                }
                res.extend_from_slice(content.len().to_string().as_bytes());
                res.push(b'\0');
                res.extend(content);
                (get_sha(&res), Bytes::from(res))
            }
            Self::Commit {
                tree,
                parents,
                author,
                committer,
                message,
            } => {
                let mut res = Vec::new();
                res.extend_from_slice(b"commit ");
                let mut content = Vec::new();

                content.extend_from_slice(b"tree ");
                content.extend_from_slice(tree.as_bytes());
                content.push(b'\n');

                for parent in parents {
                    content.extend_from_slice(b"parent ");
                    content.extend_from_slice(parent.as_bytes());
                    content.push(b'\n');
                }

                content.extend_from_slice(
                    format!(
                        "author {} <{}> {} {}\n",
                        author.name, author.email, author.timestamp, author.timezone
                    )
                    .as_bytes(),
                );
                content.extend_from_slice(
                    format!(
                        "committer {} <{}> {} {}\n",
                        committer.name, committer.email, committer.timestamp, committer.timezone
                    )
                    .as_bytes(),
                );

                content.push(b'\n');
                content.extend_from_slice(message.as_bytes());

                res.extend_from_slice(content.len().to_string().as_bytes());
                res.push(b'\0');
                res.extend(content);
                (get_sha(&res), Bytes::from(res))
            }
        }
    }

    pub fn decode(bytes: Bytes) -> GitResult<Self> {
        let i = bytes
            .iter()
            .position(|&b| b == b'\0')
            .ok_or("No null character found in object")?
            + 1;

        if &bytes[0..4] == b"blob" {
            Object::decode_blob(bytes.slice(i..))
        } else if &bytes[0..4] == b"tree" {
            Object::decode_tree(bytes.slice(i..))
        } else if &bytes[0..6] == b"commit" {
            Object::decode_commit(bytes.slice(i..))
        } else {
            Err(GitError(format!(
                "Unsupported object type: {}",
                parse_string_until(&bytes, b' ')?
            )))
        }
    }

    pub fn decode_blob(bytes: Bytes) -> GitResult<Self> {
        Ok(Object::Blob(bytes))
    }

    pub fn decode_tree(bytes: Bytes) -> GitResult<Self> {
        let mut i: usize = 0;

        let mut refs = Vec::new();
        while i < bytes.len() {
            let mode_bytes = take_until(&bytes[i..], b' ');
            let mode: usize = std::str::from_utf8(&mode_bytes)?.parse()?;
            i += mode_bytes.len() + 1;
            let name = parse_string_until(&bytes[i..], b'\0')?;
            i += name.len() + 1;
            let mut hash = [0u8; 20];
            hash.copy_from_slice(&bytes[i..i + 20]);
            i += 20;
            refs.push(ObjectReference { mode, name, hash });
        }
        Ok(Self::Tree(refs))
    }

    pub fn decode_commit(bytes: Bytes) -> GitResult<Self> {
        let mut i = 5; // "tree "
        let tree = parse_string_until(&bytes[i..], b'\n')?;
        i += tree.len() + 1;

        let mut parents = Vec::new();
        while let b"parent" = &bytes[i..i + 6] {
            i += 7; // "parent "
            let parent = parse_string_until(&bytes[i..], b'\n')?;
            i += parent.len() + 1;
            parents.push(parent);
        }

        i += 7; // "author "
        let author_result = crate::parser::parse_contributor(&bytes[i..])?;
        i += author_result.0;
        let author = author_result.1;

        i += 10; // "committer "
        let committer_result = crate::parser::parse_contributor(&bytes[i..])?;
        i += committer_result.0;
        let committer = committer_result.1;

        i += 1; // double newline before the commit message

        let message =
            std::str::from_utf8(&bytes[i..].iter().copied().collect::<Vec<u8>>())?.to_owned();

        let commit = Self::Commit {
            tree,
            parents,
            author,
            committer,
            message,
        };
        Ok(commit)
    }
}

pub fn get_sha(string: &[u8]) -> Sha {
    let mut sha_one = Sha1::new();
    sha_one.update(string);
    let bytes = sha_one.finalize();
    let mut sha = [0u8; 20];
    sha[..20].copy_from_slice(&bytes);
    sha
}

pub fn to_hex(bytes: &Sha) -> String {
    let mut hash = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter() {
        hash.push_str(format!("{:02x}", byte).as_str());
    }
    hash
}
