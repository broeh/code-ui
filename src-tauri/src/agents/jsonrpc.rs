use super::{
    StartContext,
    process::{Output, Process},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};
pub enum Message {
    Response {
        method: String,
        result: Result<Value, String>,
    },
    Request(Value),
    Notification(Value),
    Unknown,
}
#[derive(Default)]
pub struct Router {
    pub jsonrpc: bool,
    next: u64,
    pending: HashMap<String, String>,
}
impl Router {
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        self.pending.insert(self.next.to_string(), method.into());
        let mut request = json!({"id":self.next,"method":method,"params":params});
        if self.jsonrpc {
            request["jsonrpc"] = json!("2.0");
        }
        request
    }
    pub fn route(&mut self, msg: Value) -> Message {
        if msg.get("method").is_some() {
            return if msg.get("id").is_some() {
                Message::Request(msg)
            } else {
                Message::Notification(msg)
            };
        }
        let key = msg.get("id").map(|id| {
            id.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| id.to_string())
        });
        if let Some(method) = key.and_then(|key| self.pending.remove(&key)) {
            let result = if let Some(error) = msg.get("error") {
                Err(error["message"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| error.to_string()))
            } else {
                Ok(msg["result"].clone())
            };
            Message::Response { method, result }
        } else {
            Message::Unknown
        }
    }
}
pub struct Client {
    pub process: Process,
    pub router: Router,
    deferred: VecDeque<Value>,
}
impl Client {
    pub fn spawn(context: &StartContext, args: &[String]) -> Result<Self, String> {
        Ok(Self {
            process: Process::spawn(context, args)?,
            router: Router::default(),
            deferred: VecDeque::new(),
        })
    }
    pub async fn send(&mut self, method: &str, params: Value) -> Result<(), String> {
        let msg = self.router.request(method, params);
        self.process.write(&msg).await
    }
    pub async fn write(&mut self, msg: &Value) -> Result<(), String> {
        self.process.write(msg).await
    }
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let request = self.router.request(method, params);
        let id = request["id"].clone();
        self.process.write(&request).await?;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match self.process.output.recv().await {
                    Some(Output::Message(msg))
                        if msg.get("method").is_none() && msg["id"] == id =>
                    {
                        match self.router.route(msg) {
                            Message::Response { result, .. } => return result,
                            _ => return Err("Unexpected RPC response".into()),
                        }
                    }
                    Some(Output::Message(msg)) => self.deferred.push_back(msg),
                    Some(Output::Closed) | None => {
                        return Err(format!("Agent exited: {}", self.process.stderr_tail()));
                    }
                }
            }
        })
        .await
        .map_err(|_| format!("{method} timed out"))?
    }
    pub async fn next(&mut self) -> Option<Value> {
        if let Some(msg) = self.deferred.pop_front() {
            Some(msg)
        } else {
            match self.process.output.recv().await {
                Some(Output::Message(msg)) => Some(msg),
                _ => None,
            }
        }
    }
    pub async fn shutdown(&mut self) {
        self.process.terminate().await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn msp_requests_include_the_jsonrpc_version() {
        let mut router = Router {
            jsonrpc: true,
            ..Router::default()
        };
        let request = router.request("initialize", json!({}));
        assert_eq!(request["jsonrpc"], "2.0");
        assert!(
            Router::default()
                .request("initialize", json!({}))
                .get("jsonrpc")
                .is_none()
        );
    }
    #[test]
    fn responses_and_server_ids_have_separate_routes() {
        let mut r = Router::default();
        let request = r.request("initialize", json!({}));
        assert!(matches!(
            r.route(json!({"id":request["id"],"method":"approval","params":{}})),
            Message::Request(_)
        ));
        assert!(
            matches!(r.route(json!({"id":request["id"],"result":{"ready":true}})),Message::Response{method,result:Ok(_)} if method=="initialize")
        );
        assert!(matches!(
            r.route(json!({"method":"item/started","params":{}})),
            Message::Notification(_)
        ));
    }
}
