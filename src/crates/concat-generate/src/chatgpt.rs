// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The writer's calls answered by ChatGPT: OpenAI's Responses API in place
//! of Anthropic's Messages API, for a person whose key is OpenAI's. The
//! writer asks the same thing either way - its standing brief, the prompt,
//! a tool whose input is the answer - and this turns the ask into
//! OpenAI's shape and the answer back: the tool as a function, web search
//! as OpenAI's own, pictures and a PDF as input items. An answer missing
//! what the tool requires is sent back, as Claude's is, twice at most.

use serde_json::{Value, json};

use crate::writer::{decode_json_text, given, message_of, missing};

/// The Responses API.
const API: &str = "https://api.openai.com/v1/responses";
/// The model that writes when ChatGPT is picked and no model is named.
pub const DEFAULT_MODEL: &str = "gpt-5.4";

/// What marks a model as OpenAI's whatever its name: the sheet's ChatGPT
/// choice puts it before the model typed, and it is taken off before the
/// model is sent. OpenAI names some models in no pattern (`chat-latest`).
pub const MARK: &str = "openai:";

/// Whether `model` is one of OpenAI's: what sends a call here rather than
/// to Anthropic. Anything marked, GPT models, ChatGPT's, and the o-series
/// reasoning ones.
pub fn is_openai(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with(MARK)
        || model.starts_with("gpt")
        || model.starts_with("chatgpt")
        || (model.starts_with('o') && model[1..].starts_with(|c: char| c.is_ascii_digit()))
}

/// `model` as OpenAI knows it: without the mark.
fn sent_as(model: &str) -> &str {
    let model = model.trim();
    match model.get(..MARK.len()) {
        Some(head) if head.eq_ignore_ascii_case(MARK) => model[MARK.len()..].trim(),
        _ => model,
    }
}

/// The words of the ask, after whatever goes before them - Anthropic's
/// content blocks, as the writer builds them - as Responses input items:
/// a picture as an image, a PDF as a file, a text document as text.
fn content(before: &[Value], prompt: &str, name: &str) -> Vec<Value> {
    let mut items: Vec<Value> = before
        .iter()
        .filter_map(|block| {
            let source = &block["source"];
            let media = source["media_type"]
                .as_str()
                .unwrap_or("application/octet-stream");
            let data = source["data"].as_str().unwrap_or_default();
            match (block["type"].as_str(), source["type"].as_str()) {
                (Some("image"), Some("base64")) => Some(json!({
                    "type": "input_image",
                    "image_url": format!("data:{media};base64,{data}"),
                })),
                (Some("document"), Some("base64")) => Some(json!({
                    "type": "input_file",
                    "filename": "source.pdf",
                    "file_data": format!("data:{media};base64,{data}"),
                })),
                (Some("document"), Some("text")) => {
                    Some(json!({"type": "input_text", "text": data}))
                }
                _ => None,
            }
        })
        .collect();
    items.push(json!({
        "type": "input_text",
        "text": format!("{prompt}\n\nAnswer by calling the {name} tool, once."),
    }));
    items
}

/// The tools offered: OpenAI's web search where the writer offered
/// Anthropic's (`search` names which; OpenAI has no web fetch, and its
/// search opens the pages it finds), and the answer's tool as a function.
fn tools(server_tools: &[Value], tool: &Value, search: &str) -> Vec<Value> {
    let mut tools = Vec::new();
    if server_tools
        .iter()
        .any(|offered| offered["name"] == "web_search")
    {
        tools.push(json!({"type": search}));
    }
    tools.push(json!({
        "type": "function",
        "name": tool["name"],
        "description": tool["description"],
        "parameters": tool["input_schema"],
    }));
    tools
}

/// One call answered through `tool`, as `writer`'s Messages API call is:
/// the tool's input, held to what it requires.
pub(crate) fn call(
    key: &str,
    model: &str,
    system: &str,
    server_tools: &[Value],
    tool: &Value,
    prompt: &str,
    before: Vec<Value>,
) -> Result<Value, String> {
    let name = tool["name"].as_str().unwrap_or_default();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(600))
        .build();
    let mut input = json!([{"role": "user", "content": content(&before, prompt, name)}]);
    let mut previous: Option<String> = None;
    // The search tool's name: a model that refuses it is asked again with
    // the older one, once.
    let mut search = "web_search";
    let mut last = format!("ChatGPT did not answer with the {name}");
    let mut tries = 0;
    while tries < 3 {
        tries += 1;
        let mut body = json!({
            "model": sent_as(model),
            "instructions": system,
            "input": input,
            "tools": tools(server_tools, tool, search),
            "tool_choice": "auto",
            "max_output_tokens": 32000,
        });
        if let Some(id) = &previous {
            body["previous_response_id"] = json!(id);
        }
        let reply: Value = match agent
            .post(API)
            .set("Authorization", &format!("Bearer {}", key.trim()))
            .send_json(body)
        {
            Ok(response) => response.into_json().map_err(|error| error.to_string())?,
            Err(ureq::Error::Status(code, response)) => {
                let detail = message_of(&response.into_string().unwrap_or_default());
                if search == "web_search" && detail.contains("web_search") {
                    search = "web_search_preview";
                    tries -= 1;
                    continue;
                }
                return Err(format!("ChatGPT answered {code}: {detail}"));
            }
            Err(error) => return Err(format!("ChatGPT did not answer: {error}")),
        };
        if reply["status"] == "incomplete"
            && reply["incomplete_details"]["reason"] == "max_output_tokens"
        {
            return Err(format!(
                "ChatGPT ran out of room writing the {name}: try a shorter video"
            ));
        }
        let output = reply["output"].as_array().cloned().unwrap_or_default();
        let Some(call) = output.iter().find(|item| item["type"] == "function_call") else {
            let refused = output.iter().any(|item| {
                item["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|part| part["type"] == "refusal"))
            });
            if refused {
                return Err(format!("ChatGPT declined to write the {name}"));
            }
            previous = reply["id"].as_str().map(str::to_owned);
            input = json!([{"role": "user", "content": format!("Call the {name} tool now with your answer.")}]);
            continue;
        };
        let mut answer: Value =
            serde_json::from_str(call["arguments"].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
        decode_json_text(&mut answer);
        let missing = missing(tool, &answer);
        if missing.is_empty() {
            return Ok(answer);
        }
        last = format!(
            "the writer returned the {name} without {} (it gave: {})",
            missing.join(", "),
            given(&answer)
        );
        previous = reply["id"].as_str().map(str::to_owned);
        input = json!([{
            "type": "function_call_output",
            "call_id": call["call_id"],
            "output": format!(
                "{} missing or empty. Call the {name} tool again with every required field \
                 filled, lists as JSON arrays.",
                missing.join(", ")
            ),
        }]);
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OpenAI's models are told from Anthropic's by their names.
    #[test]
    fn openai_models_are_told_apart() {
        for model in [
            "gpt-5.4",
            "GPT-6",
            "chatgpt-4o-latest",
            "o3",
            "o4-mini",
            DEFAULT_MODEL,
        ] {
            assert!(is_openai(model), "{model}");
        }
        for model in ["claude-sonnet-5-5", "opus", "", " "] {
            assert!(!is_openai(model), "{model}");
        }
        // A model OpenAI names in no pattern is sent here when marked, and
        // sent to OpenAI without the mark.
        assert!(!is_openai("chat-latest"));
        assert!(is_openai("openai:chat-latest"));
        assert_eq!(sent_as(" openai:chat-latest "), "chat-latest");
        assert_eq!(sent_as("OpenAI:gpt-6"), "gpt-6");
        assert_eq!(sent_as("gpt-6"), "gpt-6");
    }

    /// The ask in OpenAI's shape: a picture and a PDF go before the words
    /// as input items, the answer's tool is a function with its schema,
    /// and web search is offered only where the writer offered it.
    #[test]
    fn the_ask_is_put_in_openai_shape() {
        let before = vec![
            json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}),
            json!({"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "BBBB"}}),
        ];
        let items = content(&before, "Write it.", "outline");
        assert_eq!(items[0]["type"], "input_image");
        assert_eq!(items[0]["image_url"], "data:image/png;base64,AAAA");
        assert_eq!(items[1]["type"], "input_file");
        assert_eq!(items[1]["file_data"], "data:application/pdf;base64,BBBB");
        assert!(
            items[2]["text"]
                .as_str()
                .unwrap()
                .contains("calling the outline tool")
        );

        let tool = json!({"name": "outline", "description": "The outline.", "input_schema": {"type": "object"}});
        let plain = tools(&[], &tool, "web_search");
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0]["type"], "function");
        assert_eq!(plain[0]["parameters"], json!({"type": "object"}));
        let searching = tools(
            &[
                json!({"type": "web_search_20260209", "name": "web_search"}),
                json!({"name": "web_fetch"}),
            ],
            &tool,
            "web_search",
        );
        assert_eq!(searching.len(), 2);
        assert_eq!(searching[0], json!({"type": "web_search"}));
    }
}
