//! Independent Git envelope schema and external hash verification.
use super::{Value, array, bytes, map, uint};
use sha1::Digest;
pub(super) fn validate(value: &Value) -> Result<(), String> {
    let m = map(value, &[0, 1, 2], &[])?;
    let kind = match uint(&m[&0])? {
        1 => "blob",
        2 => "tree",
        3 => "commit",
        4 => "tag",
        _ => return Err("invalid Git kind".into()),
    };
    let id = array(&m[&1])?;
    if id.len() != 3 || uint(&id[0])? != 0 {
        return Err("invalid external Git reference".into());
    }
    let body = bytes(&m[&2])?;
    // Build the Git preimage independently of the production helper.
    let mut header = kind.as_bytes().to_vec();
    header.push(b' ');
    header.extend(body.len().to_string().bytes());
    header.push(0);
    let expected = bytes(&id[2])?;
    let actual = match uint(&id[1])? {
        1 => {
            let mut hash = sha1::Sha1::default();
            hash.update(&header);
            hash.update(body);
            hash.finalize().to_vec()
        }
        2 => {
            let mut hash = sha2::Sha256::default();
            hash.update(&header);
            hash.update(body);
            hash.finalize().to_vec()
        }
        _ => return Err("unsupported Git algorithm".into()),
    };
    if expected != actual {
        return Err("Git object ID mismatch".into());
    }
    Ok(())
}
