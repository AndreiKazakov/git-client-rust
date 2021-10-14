use crate::git_error::{GitError, GitResult};
use crate::parser::{parse_string_until, take_until};
use bytes::Bytes;

pub type Sha = [u8; 20];

#[derive(Debug)]
pub enum Object {
    Blob(Vec<u8>),
    Tree(Vec<ObjectReference>),
    Commit {
        tree: String,
        parents: Vec<String>,
        author: Contributer,
        committer: Contributer,
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
pub struct Contributer {
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
                        crate::to_hex(&r.hash)?,
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

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Blob(bytes) => {
                let mut res = Vec::new();
                res.extend_from_slice(b"blob ");
                res.extend_from_slice(bytes.len().to_string().as_bytes());
                res.push(b'\0');
                res.extend(bytes);
                res
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
                res
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
                res
            }
        }
    }

    pub fn decode(bytes: Bytes) -> GitResult<Self> {
        if &bytes[0..4] == b"blob" {
            Ok(Object::Blob(
                bytes
                    .into_iter()
                    .skip_while(|c| *c != b'\0')
                    .skip(1)
                    .collect(),
            ))
        } else if &bytes[0..4] == b"tree" {
            let mut refs = Vec::new();
            let mut i = bytes
                .iter()
                .position(|&b| b == b'\0')
                .ok_or("No null character found in tree object")?
                + 1;

            while i < bytes.len() {
                let mode_bytes = take_until(&bytes[i..], b' ');
                let mode: usize = std::str::from_utf8(&mode_bytes)?.parse()?;
                i += mode_bytes.len() + 1;
                let name = parse_string_until(&bytes[i..], b'\0')?;
                i += name.len() + 1;
                let mut hash = [0u8; 20];
                hash.copy_from_slice(&bytes[i..i + 20]);
                i += 20;
                refs.push(ObjectReference { mode, name, hash })
            }
            Ok(Self::Tree(refs))
        } else if &bytes[0..6] == b"commit" {
            let mut i = bytes
                .iter()
                .position(|&b| b == b'\0')
                .ok_or("No null character found in commit object")?
                + 1;

            i += 5; // "tree "
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
        } else {
            Err(GitError(format!(
                "Unsupported object type: {}",
                parse_string_until(&bytes, b' ')?
            )))
        }
    }
}
