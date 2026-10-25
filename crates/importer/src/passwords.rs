//! Password exports never enter the browser model, session JSON, logs or Debug.
use crate::Result;
pub struct Credential {
    pub origin: String,
    pub username: String,
    pub password: String,
}
impl Drop for Credential {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
    }
}
pub trait CredentialStore: Send + Sync {
    fn store(&self, profile: &str, credential: &Credential) -> Result<()>;
}
/// The bounded source buffer is zeroed on every return path.
pub fn read_password_file(path: &std::path::Path) -> Result<Vec<Credential>> {
    if path.extension().is_some_and(|v| v.eq_ignore_ascii_case("zip")) {
        let mut credentials = None;
        crate::exports::for_each_entry(path, &[".csv"], 8 * 1024 * 1024, |_, bytes| {
            if credentials.is_some() { return Err("Choose a password CSV when an archive has multiple CSV files.".into()); }
            credentials = Some(parse_password_csv(bytes)?);
            Ok(())
        })?;
        return credentials.ok_or("The archive contains no password CSV.".into());
    }
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|_| "The passwords file could not be opened.".to_string())?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The passwords file could not be read.".to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("The passwords file is too large.".into());
    }
    parse_password_csv(&bytes)
}
pub fn parse_password_csv(input: &[u8]) -> Result<Vec<Credential>> {
    let input = input.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(input);
    let mut reader = csv::ReaderBuilder::new().flexible(false).from_reader(input);
    let headers = reader
        .headers()
        .map_err(|_| "The file you uploaded doesn't look like Passwords.csv.")?
        .clone();
    let column = |names: &[&str]| {
        headers
            .iter()
            .position(|h| names.iter().any(|name| h.eq_ignore_ascii_case(name)))
    };
    let url =
        column(&["url", "website", "login_uri"]).ok_or("The passwords file has no URL column.")?;
    let user = column(&["username", "user name", "login_username"])
        .ok_or("The passwords file has no username column.")?;
    let password = column(&["password", "login_password"])
        .ok_or("The passwords file has no password column.")?;
    let mut out = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|_| "The passwords file contains an invalid CSV row.")?;
        let parsed = url::Url::parse(&record[url])
            .map_err(|_| "The passwords file contains an invalid website address.")?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            continue;
        }
        if record[password].is_empty() {
            continue;
        }
        out.push(Credential {
            origin: parsed.origin().ascii_serialization(),
            username: record[user].into(),
            password: record[password].into(),
        });
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_handles_quoted_secrets_and_safari_and_never_returns_partial_invalid_input() {
        let rows = parse_password_csv(
            b"name,url,username,password\nExample,https://example.com/a,me,\"a,b\"\n",
        )
        .unwrap();
        assert_eq!(rows[0].origin, "https://example.com");
        assert_eq!(rows[0].password, "a,b");
        assert!(
            parse_password_csv(
                b"url,username,password\nhttps://example.com,user,secret\ninvalid,user,secret\n"
            )
            .is_err()
        );
    }
}
