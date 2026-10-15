use super::readers::{Message, Session, Source, timestamp};
use crate::agents::{AgentKind, StartContext, TurnSettings, jsonrpc::Client};
use serde_json::{Value, json};
use std::collections::HashMap;

pub async fn read(
    source: Source,
    binary: String,
    env: HashMap<String, String>,
) -> Result<Session, String> {
    let context = StartContext {
        binary,
        env,
        cwd: std::path::PathBuf::from(source.cwd.as_deref().unwrap_or("/tmp")),
        native_id: None,
        chat_id: "history-reader".into(),
        settings: TurnSettings {
            agent: source.agent,
            ..TurnSettings::default()
        },
        log_dir: crate::paths::data_dir()?.join("logs"),
    };
    let muse = source.agent == AgentKind::Muse;
    let mut client = Client::spawn(
        &context,
        &[if muse { "serve" } else { "app-server" }.into()],
    )?;
    client.router.jsonrpc = muse;
    let result=async {
        client.call("initialize",json!({"clientInfo":{"name":"code_ui","version":env!("CARGO_PKG_VERSION")}})).await?;
        let mut ready=json!({"method":"initialized"});if muse{ready["jsonrpc"]=json!("2.0");}client.write(&ready).await?;
        let mut items=Vec::new();
        let (cwd,title,created,updated,model)=if muse {
            let read=client.call("session/read",json!({"sessionId":source.native_id,"excludeItems":false})).await?;
            let session=&read["session"];let head=read["viewCursor"].clone();
            if let Some(history)=read["history"]["items"].as_array(){items=history.clone();}else {
                let mut cursor:Option<String>=None;let mut ordered=Vec::new();let mut latest=HashMap::new();
                loop {
                    let mut params=json!({"sessionId":source.native_id,"limit":1000,"direction":"forward"});if let Some(cursor)=&cursor{params["cursor"]=json!(cursor);}
                    let page=client.call("view/page",params).await?;let mut reached=false;
                    if let Some(events)=page["events"].as_array(){for event in events {
                        let item=&event["params"]["item"];
                        if let Some(id)=item["itemId"].as_str(){if !latest.contains_key(id){ordered.push(id.to_owned());}latest.insert(id.to_owned(),item.clone());}
                        if event["params"]["viewCursor"]==head{reached=true;break;}
                    }}
                    let next=page["nextCursor"].as_str().map(str::to_owned);if reached || next.is_none() || next==cursor{break;}cursor=next;
                }
                items=ordered.into_iter().filter_map(|id|latest.remove(&id)).collect();
            }
            (session["workspaceRoot"].as_str().unwrap_or_default().to_owned(),session["title"].as_str().map(str::to_owned),timestamp(&session["createdAt"]).unwrap_or(source.updated_at),timestamp(&session["updatedAt"]).unwrap_or(source.updated_at),session["modelId"].as_str().map(str::to_owned))
        }else {
            let read=client.call("thread/read",json!({"threadId":source.native_id,"includeTurns":false})).await?;
            let thread=&read["thread"];let mut cursor=Value::Null;
            loop {
                let page=client.call("thread/turns/list",json!({"threadId":source.native_id,"cursor":cursor,"limit":100,"sortDirection":"asc","itemsView":"full"})).await?;
                if let Some(turns)=page["data"].as_array(){for turn in turns{if let Some(rows)=turn["items"].as_array(){items.extend(rows.iter().cloned());}}}
                let next=page["nextCursor"].clone();if next.is_null() || next==cursor{break;}cursor=next;
            }
            (thread["cwd"].as_str().unwrap_or_default().to_owned(),thread["name"].as_str().map(str::to_owned),timestamp(&thread["createdAt"]).unwrap_or(source.updated_at),timestamp(&thread["updatedAt"]).unwrap_or(source.updated_at),source.model.clone())
        };
        let messages:Vec<_>=items.into_iter().filter_map(|item| {
            let kind=item[if muse{"kind"}else{"type"}].as_str().unwrap_or_default();
            let role=match kind{"userMessage"=>"user","agentMessage"=>"assistant",_=>return None};
            let text=if kind=="userMessage" && !muse {item["content"].as_array().map(|parts|parts.iter().filter_map(|part|part["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default()}else{item["text"].as_str().unwrap_or_default().to_owned()};
            if super::readers::noise(&text){return None;}
            Some(Message{role:role.into(),text,created_at:timestamp(&item["recordedAt"]).unwrap_or(created)})
        }).collect();
        if messages.is_empty(){return Err("The native reader found no conversation text.".into());}
        let title=source.title.clone().filter(|title|!super::readers::noise(title)).or(title.filter(|title|!super::readers::noise(title))).unwrap_or_else(||messages.iter().find(|m|m.role=="user").map(|m|m.text.chars().take(120).collect()).unwrap_or_else(||"Terminal conversation".into()));
        Ok(Session{settings:TurnSettings{agent:source.agent,model,..TurnSettings::default()},source,cwd,title,created_at:created,updated_at:updated,messages})
    }.await;
    client.shutdown().await;
    result
}
