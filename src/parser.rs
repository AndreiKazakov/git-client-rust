use crate::git_error::GitResult;
use crate::object::Contributor;

pub fn parse_contributor(bytes: &[u8]) -> GitResult<(usize, Contributor)> {
    let mut i = 0;
    let name = parse_string_until(&bytes[i..], b'<')?.trim_end().to_owned();
    i += name.len() + 2;

    let email = parse_string_until(&bytes[i..], b'>')?;
    i += email.len() + 2;

    let timestamp_bytes = parse_string_until(&bytes[i..], b' ')?;
    let timestamp = timestamp_bytes.parse::<u64>()?;
    i += timestamp_bytes.len() + 1;

    let timezone = parse_string_until(&bytes[i..], b'\n')?;
    i += timezone.len() + 1;

    Ok((
        i,
        Contributor {
            name,
            email,
            timestamp,
            timezone,
        },
    ))
}

pub fn take_until(bytes: &[u8], delimiter: u8) -> Vec<u8> {
    bytes
        .iter()
        .take_while(|&&b| b != delimiter)
        .copied()
        .collect::<Vec<u8>>()
}

pub fn parse_string_until(bytes: &[u8], delimiter: u8) -> GitResult<String> {
    let byte_vec = take_until(bytes, delimiter);
    Ok(std::str::from_utf8(&byte_vec)?.to_owned())
}
