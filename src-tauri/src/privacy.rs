use serde_json::Value;
use std::{
    fs, io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub fn private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

pub fn private_file(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

pub fn append_file(path: &Path) -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    private_file(path)?;
    Ok(file)
}

pub fn redact_answer(payload: &Value, answer: &Value) -> Value {
    let mut answer = answer.clone();
    let secret_ids: Vec<_> = payload["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|question| question["secret"] == true)
        .filter_map(|question| question["id"].as_str())
        .collect();
    if secret_ids.is_empty() {
        return answer;
    }
    if let Some(answers) = answer["answers"].as_array_mut() {
        for item in answers {
            if item["questionId"]
                .as_str()
                .is_some_and(|id| secret_ids.contains(&id))
            {
                item["labels"] = serde_json::json!(["[redacted]"]);
                item["other"] = Value::Null;
            }
        }
    }
    if answer["note"].is_string() {
        answer["note"] = Value::String("[redacted]".into());
    }
    answer
}

pub fn safe_answer(payload: &Value, raw: Option<String>) -> Option<String> {
    let answer: Value = serde_json::from_str(&raw?).ok()?;
    Some(redact_answer(payload, &answer).to_string())
}

pub fn has_secret_input(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.get("isSecret") == Some(&Value::Bool(true))
                || object.get("secret") == Some(&Value::Bool(true))
                || object.values().any(has_secret_input)
        }
        Value::Array(items) => items.iter().any(has_secret_input),
        _ => false,
    }
}

pub fn redact_trace(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let value = if matches!(
                        key.as_str(),
                        "answers"
                            | "freeText"
                            | "selectedLabel"
                            | "selectedLabels"
                            | "note"
                            | "feedback"
                    ) {
                        Value::String("[redacted]".into())
                    } else {
                        redact_trace(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_trace).collect()),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn data_directories_and_diagnostics_are_owner_only() {
        let dir = std::env::temp_dir().join(format!("code-ui-private-{}", uuid::Uuid::new_v4()));
        private_dir(&dir).unwrap();
        let path = dir.join("trace.jsonl");
        append_file(&path).unwrap();
        assert_eq!(dir.metadata().unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn secret_answers_and_notes_are_redacted_without_changing_normal_answers() {
        let payload =
            json!({"questions":[{"id":"token","secret":true},{"id":"color","secret":false}]});
        let answer = json!({"type":"answers","answers":[{"questionId":"token","labels":[],"other":"private-token"},{"questionId":"color","labels":["Blue"],"other":null}],"note":"private-note"});
        let redacted = redact_answer(&payload, &answer);
        assert!(!redacted.to_string().contains("private-"));
        assert_eq!(redacted["answers"][1]["labels"], json!(["Blue"]));
        assert_eq!(answer["answers"][0]["other"], "private-token");
        assert_eq!(redact_answer(&json!({"questions":[]}), &answer), answer);
    }

    #[test]
    fn diagnostics_recognize_nested_secret_prompts_and_redact_all_answer_protocols() {
        assert!(has_secret_input(
            &json!({"params":{"questions":[{"isSecret":true}]}})
        ));
        for message in [
            json!({"result":{"answers":{"q":{"answers":["private-token"]}}}}),
            json!({"response":{"response":{"updatedInput":{"answers":{"q":"private-token"}}}}}),
            json!({"params":{"answers":[{"freeText":"private-token"}]}}),
        ] {
            assert!(!redact_trace(&message).to_string().contains("private-token"));
        }
    }
}
