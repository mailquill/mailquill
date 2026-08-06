//! Minimal ManageSieve (RFC 5804) client + Sieve compiler. Uploads the active
//! filter script for an account. Used for IMAP/Sieve accounts only — Gmail and
//! Exchange use their own rule systems and are skipped by the caller.

use base64::Engine;
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Deserialize)]
pub struct Cond {
    pub field: String,
    pub op: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct Act {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub value: Option<String>,
}

pub struct CompiledRule {
    pub name: String,
    pub match_all: bool,
    pub conditions: Vec<Cond>,
    pub actions: Vec<Act>,
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn header_for(field: &str) -> Option<&'static str> {
    match field {
        "from" => Some("from"),
        "to" => Some("to"),
        "subject" => Some("subject"),
        _ => None,
    }
}

fn sieve_test(c: &Cond) -> String {
    if c.field == "body" {
        return if c.op == "notContains" {
            format!("not body :contains {}", quote(&c.value))
        } else {
            format!("body :contains {}", quote(&c.value))
        };
    }
    let header = quote(header_for(&c.field).unwrap_or("subject"));
    match c.op.as_str() {
        "is" => format!("header :is {header} {}", quote(&c.value)),
        "notContains" => format!("not header :contains {header} {}", quote(&c.value)),
        _ => format!("header :contains {header} {}", quote(&c.value)),
    }
}

fn sieve_action(a: &Act) -> Option<String> {
    Some(match a.kind.as_str() {
        "move" => format!(
            "    fileinto {};",
            quote(a.value.as_deref().unwrap_or("INBOX"))
        ),
        "markRead" => "    setflag \"\\\\Seen\";".to_owned(),
        "star" => "    setflag \"\\\\Flagged\";".to_owned(),
        "delete" => "    discard;".to_owned(),
        "forward" => format!("    redirect {};", quote(a.value.as_deref().unwrap_or(""))),
        _ => return None,
    })
}

/// Compile a set of rules into a single Sieve script.
pub fn compile_sieve(rules: &[CompiledRule]) -> String {
    let mut out = String::from("require [\"fileinto\", \"imap4flags\"];\n");
    for rule in rules {
        let tests: Vec<String> = rule.conditions.iter().map(sieve_test).collect();
        let guard = if tests.is_empty() {
            "if true {".to_owned()
        } else {
            let join = if rule.match_all { "allof" } else { "anyof" };
            format!("if {join} ({}) {{", tests.join(", "))
        };
        out.push_str(&format!("\n# {}\n{}\n", rule.name, guard));
        for line in rule.actions.iter().filter_map(sieve_action) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("    stop;\n}\n");
    }
    out
}

/// Read ManageSieve responses until a terminating OK/NO/BYE status line.
async fn read_status<R: AsyncRead + Unpin>(r: &mut R) -> Result<(), String> {
    let mut line: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = r.read(&mut byte).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed".into());
        }
        if byte[0] == b'\n' {
            let text = String::from_utf8_lossy(&line);
            let trimmed = text.trim_end_matches('\r').trim_start();
            let upper = trimmed.to_ascii_uppercase();
            if upper.starts_with("OK") {
                return Ok(());
            }
            if upper.starts_with("NO") || upper.starts_with("BYE") {
                return Err(trimmed.to_owned());
            }
            line.clear();
        } else {
            line.push(byte[0]);
        }
    }
}

/// Upload `script` as the active Sieve filter via ManageSieve (STARTTLS + SASL
/// PLAIN). `host` is the IMAP host; ManageSieve listens on port 4190.
pub async fn upload_script(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    name: &str,
    script: &str,
) -> Result<(), String> {
    let mut tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| e.to_string())?;
    read_status(&mut tcp).await?; // server greeting + capabilities

    tcp.write_all(b"STARTTLS\r\n")
        .await
        .map_err(|e| e.to_string())?;
    read_status(&mut tcp).await?;

    let config = mailquill_core::tls::webpki_client_config();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| e.to_string())?;
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| e.to_string())?;
    read_status(&mut tls).await?; // post-TLS capabilities

    let sasl =
        base64::engine::general_purpose::STANDARD.encode(format!("\0{username}\0{password}"));
    tls.write_all(format!("AUTHENTICATE \"PLAIN\" \"{sasl}\"\r\n").as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    read_status(&mut tls).await?;

    let put = format!("PUTSCRIPT \"{name}\" {{{}+}}\r\n{script}\r\n", script.len());
    tls.write_all(put.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    read_status(&mut tls).await?;

    tls.write_all(format!("SETACTIVE \"{name}\"\r\n").as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    read_status(&mut tls).await?;

    let _ = tls.write_all(b"LOGOUT\r\n").await;
    Ok(())
}
