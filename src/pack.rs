use bytes::Bytes;

use crate::git_error::{GitError, GitResult};
use crate::object::{Object, Sha};
use crate::{object, zlib};
use std::collections::HashMap;

#[derive(Debug)]
pub enum PackObjType {
    ObjCommit(Bytes),
    ObjTree(Bytes),
    ObjBlob(Bytes),
    ObjTag(Bytes),
    ObjOfsDelta(usize, Bytes),
    ObjRefDelta(Sha, Bytes),
}

enum Instruction {
    Copy(usize, usize),
    Insert(usize),
}

pub fn parse_pack(pack: Bytes) -> GitResult<HashMap<String, Object>> {
    let count = u32::from_be_bytes([pack[8], pack[9], pack[10], pack[11]]) as usize;
    if pack.slice(..8).as_ref() != b"PACK\0\0\0\x02" {
        return Err(GitError(format!(
            "No PACK header in the pack file: {:?}",
            pack.slice(..8)
        )));
    }
    let mut content_by_sha = HashMap::new();
    let mut sha_by_byte_offset = HashMap::new();
    let mut i = 12;

    while i < pack.len() - 20 {
        let (len, obj) = read_pack_object(pack.slice(i..))?;
        match obj {
            PackObjType::ObjCommit(content) => {
                let decoded = Object::decode_commit(content.clone())?;
                let (sha, _) = decoded.encode();
                // objects.push(decoded);
                // let sha = object::get_sha(content.as_ref());
                content_by_sha.insert(sha, (decoded, content));
                sha_by_byte_offset.insert(i, sha);
            }
            PackObjType::ObjTree(content) => {
                let decoded = Object::decode_tree(content.clone())?;
                let (sha, _) = decoded.encode();
                // objects.push(decoded);
                // let sha = object::get_sha(content.as_ref());
                content_by_sha.insert(sha, (decoded, content));
                sha_by_byte_offset.insert(i, sha);
            }
            PackObjType::ObjBlob(content) => {
                let decoded = Object::decode_blob(content.clone())?;
                let (sha, _) = decoded.encode();
                // objects.push(decoded);
                // let sha = object::get_sha(content.as_ref());
                content_by_sha.insert(sha, (decoded, content));
                sha_by_byte_offset.insert(i, sha);
            }
            PackObjType::ObjTag(_) => {}
            PackObjType::ObjOfsDelta(offset, delta) => {
                let base_sha = *sha_by_byte_offset
                    .get(&(i - offset))
                    .ok_or(format!("Could not find object with offset {}", offset))?;
                let (base_object, base) = content_by_sha.get(&base_sha).ok_or(format!(
                    "Could not find object {}",
                    object::to_hex(&base_sha)
                ))?;
                let content = apply_delta(base, &delta)?;
                let unpacked_obj = match base_object {
                    Object::Blob(_) => Object::decode_blob(content.clone())?,
                    Object::Tree(_) => Object::decode_tree(content.clone())?,
                    Object::Commit { .. } => Object::decode_commit(content.clone())?,
                };
                let (sha, _) = unpacked_obj.encode();
                content_by_sha.insert(sha, (unpacked_obj, content));
                sha_by_byte_offset.insert(i, sha);
            }
            PackObjType::ObjRefDelta(base_sha, delta) => {
                let (base_object, base) = content_by_sha.get(&base_sha).ok_or(format!(
                    "Could not find object {}",
                    object::to_hex(&base_sha)
                ))?;
                let content = apply_delta(base, &delta)?;
                let unpacked_obj = match base_object {
                    Object::Blob(_) => Object::decode_blob(content.clone())?,
                    Object::Tree(_) => Object::decode_tree(content.clone())?,
                    Object::Commit { .. } => Object::decode_commit(content.clone())?,
                };
                let (sha, _) = unpacked_obj.encode();
                content_by_sha.insert(sha, (unpacked_obj, content));
                sha_by_byte_offset.insert(i, sha);
            }
        }
        i += len as usize;
    }
    if count != content_by_sha.len() {
        return Err(GitError(format!(
            "Wrong number of objects in a pack: expected {} got {}",
            count,
            content_by_sha.len()
        )));
    }
    Ok(content_by_sha
        .into_iter()
        .map(|(sha, (o, _))| (object::to_hex(&sha), o))
        .collect())
}

fn apply_delta(base: &Bytes, delta: &Bytes) -> GitResult<Bytes> {
    let mut res = Vec::new();
    let mut i = 0;

    let source_len_last_byte = delta
        .iter()
        .position(|&b| b < 128)
        .ok_or("Could not find a byte with a leading 0")?;
    let source_len_bytes = delta.slice(..=source_len_last_byte);
    i += source_len_bytes.len();
    let source_len = read_var_len_integer_le(source_len_bytes);

    if base.len() != source_len {
        return Err(GitError(format!(
            "Wrong source length: expected {} got {}",
            source_len,
            base.len()
        )));
    }

    let target_len_last_byte = delta
        .slice(i..)
        .iter()
        .position(|&b| b < 128)
        .ok_or("Could not find a byte with a leading 0")?;
    let target_len_bytes = delta.slice(i..=target_len_last_byte + i);
    i += target_len_bytes.len();
    let target_len = read_var_len_integer_le(target_len_bytes);

    while i < delta.len() {
        match parse_instruction(delta[i], &delta.slice(i + 1..)) {
            (skip, Instruction::Copy(len, offset)) => {
                i += skip + 1;
                res.extend_from_slice(base.slice(offset..offset + len).as_ref())
            }
            (skip, Instruction::Insert(len)) => {
                i += skip;
                res.extend_from_slice(delta.slice(i..i + len).as_ref());
                i += len;
            }
        }
    }

    if res.len() != target_len {
        return Err(GitError(format!(
            "Wrong length after applying delta: expected {} got {}",
            target_len,
            res.len()
        )));
    }

    Ok(Bytes::from(res))
}

fn parse_instruction(instruction: u8, bs: &Bytes) -> (usize, Instruction) {
    if instruction > 128 {
        let mut i: usize = 0;
        let mut len: usize = 0;
        let mut offset: usize = 0;
        for o in 0..4 {
            if (1 << o) & instruction > 0 {
                offset += (bs[i] as usize) << (8 * o);
                i += 1;
            }
        }
        for l in 4..6 {
            if (1 << l) & instruction > 0 {
                len += (bs[i] as usize) << (8 * (l - 4));
                i += 1;
            }
        }
        (i, Instruction::Copy(len, offset))
    } else {
        (1, Instruction::Insert((instruction & 0b0111_1111) as usize))
    }
}

fn read_pack_object(bytes: Bytes) -> GitResult<(usize, PackObjType)> {
    let metadata_last_byte = bytes
        .iter()
        .position(|&b| b < 128)
        .ok_or("Could not find a byte with a leading 0")?;
    let metadata = bytes.slice(..=metadata_last_byte);
    let (obj_type_code, len) = read_pack_metadata(&metadata)?;
    let object_byte_length: usize;
    let real_content_length: usize;

    let obj_type = match obj_type_code {
        1 => {
            let obj_bytes = bytes.slice(metadata.len()..);
            let (compressed_length, content) = zlib::read(obj_bytes)?;
            real_content_length = content.len();
            object_byte_length = compressed_length + metadata.len();
            PackObjType::ObjCommit(content)
        }
        2 => {
            let obj_bytes = bytes.slice(metadata.len()..);
            let (compressed_length, content) = zlib::read(obj_bytes)?;
            real_content_length = content.len();
            object_byte_length = compressed_length + metadata.len();
            PackObjType::ObjTree(content)
        }
        3 => {
            let obj_bytes = bytes.slice(metadata.len()..);
            let (compressed_length, content) = zlib::read(obj_bytes)?;
            real_content_length = content.len();
            object_byte_length = compressed_length + metadata.len();
            PackObjType::ObjBlob(content)
        }
        4 => {
            let obj_bytes = bytes.slice(metadata.len()..);
            let (compressed_length, content) = zlib::read(obj_bytes)?;
            real_content_length = content.len();
            object_byte_length = compressed_length + metadata.len();
            PackObjType::ObjTag(content)
        }
        6 => {
            let offset_last_byte = bytes
                .iter()
                .skip(metadata.len())
                .position(|&b| b < 128)
                .ok_or("Could not find a byte with a leading 0")?;
            let offset_bytes = bytes.slice(metadata.len()..=metadata.len() + offset_last_byte);
            let obj_bytes = bytes.slice(metadata.len() + offset_last_byte + 1..);
            let (compressed_length, content) = zlib::read(obj_bytes)?;
            object_byte_length = compressed_length + offset_last_byte + 1 + metadata.len();
            let offset = read_var_len_integer_be_with_increment(offset_bytes);
            real_content_length = content.len();
            PackObjType::ObjOfsDelta(offset, content)
        }
        7 => {
            let obj_bytes = bytes.slice(metadata.len() + 20..);
            let (compressed_length, content) = zlib::read(obj_bytes)?;
            real_content_length = content.len();
            let mut sha = [0u8; 20];
            sha[..20].copy_from_slice(&bytes.slice(metadata.len()..metadata.len() + 20));
            object_byte_length = compressed_length + 20 + metadata.len();
            PackObjType::ObjRefDelta(sha, content)
        }
        _ => {
            return Err(GitError(format!(
                "Unrecognized object type: {}",
                obj_type_code
            )))
        }
    };

    if real_content_length != len {
        return Err(GitError(format!(
            "Wrong object length: expected {} got {}, obj_type {}",
            len, real_content_length, obj_type_code
        )));
    }
    Ok((object_byte_length, obj_type))
}

fn read_pack_metadata(bytes: &Bytes) -> GitResult<(u8, usize)> {
    let obj_type_code = (bytes[0] & 0b01110000) >> 4;
    let little_end = (bytes[0] & 0b00001111) as usize;
    let mut res = read_var_len_integer_le(bytes.slice(1..));
    res <<= 4;
    res += little_end;

    Ok((obj_type_code, res))
}

fn read_var_len_integer_le(bytes: Bytes) -> usize {
    let mut res = 0;
    let mut shift = 0;
    for byte in bytes {
        res += ((byte & 0b01111111) as usize) << shift;
        shift += 7
    }
    res
}

fn read_var_len_integer_be_with_increment(bytes: Bytes) -> usize {
    let mut res = 0;
    let mut shift = 0;
    for (i, byte) in bytes.iter().enumerate() {
        res += ((byte & 0b01111111) as usize) << shift;
        if i != bytes.len() - 1 {
            res += 1;
        }
        shift += 7
    }
    res
}
