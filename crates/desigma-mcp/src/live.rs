//! Authenticated, bounded local editor client. Tokens never enter tool results.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    address: String,
    token: String,
    pub document_id: String,
    protocol_version: u32,
}
impl Endpoint {
    pub fn load(path: &Path) -> Result<Self, String> {
        let meta = fs::symlink_metadata(path).map_err(|e| format!("permission_denied: cannot read endpoint: {e}"))?;
        if !meta.is_file() || meta.len() > 16_384 {
            return Err("permission_denied: endpoint must be a regular private file under 16 KiB".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err("permission_denied: endpoint file must be private (mode 0600)".into());
            }
        }
        let e: Self = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| format!("invalid_operation: invalid endpoint: {e}"))?;
        let address: SocketAddr = e.address.parse().map_err(|_| "invalid_operation: invalid endpoint address")?;
        if !address.ip().is_loopback() || e.protocol_version != desigma_agent_core::LIVE_PROTOCOL_VERSION || e.token.len() < 32 || e.document_id.is_empty() {
            return Err("permission_denied: unsupported or unsafe endpoint".into());
        }
        Ok(e)
    }
    pub fn call(&self, tool: &str, args: Value) -> Result<Value, String> {
        let address: SocketAddr = self.address.parse().map_err(|_| "invalid_operation: invalid endpoint address")?;
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| format!("operation_conflict: editor unavailable: {e}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(15))).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
        let mut request = serde_json::to_vec(&json!({"token":self.token,"document_id":self.document_id,"tool":tool,"args":args})).map_err(|e| e.to_string())?;
        if request.len() > 1_048_576 {
            return Err("invalid_operation: request exceeds 1 MiB".into());
        }
        request.push(b'\n');
        stream.write_all(&request).map_err(|e| format!("operation_conflict: editor disconnected: {e}"))?;
        let mut reply = String::new();
        use std::io::Read;
        BufReader::new(stream)
            .take(32 * 1024 * 1024 + 1)
            .read_line(&mut reply)
            .map_err(|e| format!("operation_conflict: editor timeout or disconnect: {e}"))?;
        if reply.len() > 32 * 1024 * 1024 {
            return Err("operation_conflict: editor response exceeds 32 MiB".into());
        }
        let response: Value = serde_json::from_str(&reply).map_err(|_| "operation_conflict: invalid editor response")?;
        if let Some(error) = response.get("error") {
            return Err(format!(
                "{}: {}",
                error["code"].as_str().unwrap_or("operation_conflict"),
                error["message"].as_str().unwrap_or("Editor request failed")
            ));
        }
        response.get("result").cloned().ok_or("operation_conflict: editor response omitted result".into())
    }
}
pub fn discover() -> Vec<Value> {
    let dir = desigma_library::Library::default_root().join("agent-endpoints");
    let mut paths: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let endpoint = Endpoint::load(&path).ok()?;
            Some(json!({"endpoint_path":path,"document_id":endpoint.document_id}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticates_and_bounds_local_endpoint_discovery() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint =
            Endpoint { address: listener.local_addr().unwrap().to_string(), token: "a".repeat(64), document_id: "document-a".into(), protocol_version: 1 };
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(socket.try_clone().unwrap()).read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["token"], "a".repeat(64));
            assert_eq!(request["document_id"], "document-a");
            assert_eq!(request["tool"], "desigma_get_document");
            socket.write_all(b"{\"result\":{\"revision\":7}}\n").unwrap();
        });
        assert_eq!(endpoint.call("desigma_get_document", json!({})).unwrap()["revision"], 7);
        worker.join().unwrap();
        let path = std::env::temp_dir().join(format!("desigma-endpoint-test-{}.json", std::process::id()));
        fs::write(&path, json!({"address":"192.0.2.1:40","token":"x".repeat(64),"document_id":"id","protocol_version":1}).to_string()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(Endpoint::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
