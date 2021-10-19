use reqwest::blocking::{get, Client};

use crate::git_error::{GitError, GitResult};
use crate::object::Object;
use crate::pack;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Ref {
    pub sha: String,
    pub name: String,
}

pub fn get_refs(url: &str) -> GitResult<Vec<Ref>> {
    let body = get(format!("{}/info/refs?service=git-upload-pack", url).as_str())?.text()?;
    let mut refs = <Vec<Ref>>::new();

    for line in body.lines().skip(1) {
        if line == "0000" {
            break;
        }
        let ref_data: Vec<&str> = line
            .split('\0')
            .next()
            .ok_or("Empty line in refs")?
            .split(' ')
            .collect();
        refs.push(Ref {
            sha: ref_data
                .get(0)
                .ok_or("ref id not found")?
                .trim_start_matches("0000")[4..]
                .to_string(),
            name: ref_data.get(1).ok_or("ref name not found")?.to_string(),
        })
    }
    Ok(refs)
}

pub fn fetch_ref(url: &str, ref_id: &str) -> GitResult<HashMap<String, Object>> {
    let mut response = Client::builder()
        .build()?
        .post(format!("{}/git-upload-pack", url).as_str())
        .body(pkt_message(vec![format!("want {}", ref_id)]))
        .header("Content-Type", "application/x-git-upload-pack-request")
        .send()?
        .bytes()?;
    let nak = response.split_to(8);
    if nak.as_ref() != b"0008NAK\n" {
        return Err(GitError(format!("No NAK header in response: {:?}", nak)));
    }
    Ok(pack::parse_pack(response)?)
}

fn pkt_message(lines: Vec<String>) -> String {
    lines
        .into_iter()
        .map(encode_pkt)
        .collect::<Vec<String>>()
        .join("")
        + "00000009done\n"
}

fn encode_pkt(msg: String) -> String {
    format!("{:04x}{}\n", msg.len() + 5, msg)
}
