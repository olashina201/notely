//! Post-meeting notes through the selected assistant provider.
use crate::{
    providers::{self, Prompt, Provider},
    state::TranscriptLine,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingNotes {
    summary: String,
    decisions: Vec<String>,
    action_items: Vec<ActionItem>,
    follow_up_email: String,
}
#[derive(Debug, Deserialize, Serialize)]
struct ActionItem {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
}

pub async fn generate_notes(
    provider: Provider,
    key: &str,
    model: &str,
    transcript: &[TranscriptLine],
    profile: &str,
) -> Result<serde_json::Value, String> {
    let context = transcript
        .iter()
        .map(|l| format!("[{}] {}", l.speaker, l.text))
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!("Generate structured meeting notes. Profile context: {profile}. Use only evidence in the transcript; do not invent decisions, owners, or commitments. Return ONLY a JSON object matching this structure: {{\"summary\": \"2-4 sentences\", \"decisions\": [\"decision\"], \"actionItems\": [{{\"text\": \"action\", \"owner\": \"only if known\"}}], \"followUpEmail\": \"ready-to-send email body\"}}. Empty lists are allowed. Omit owner when unknown.");
    let text = providers::complete(
        provider,
        key,
        &Prompt {
            model,
            system: &system,
            context: &context,
            question: "Generate the meeting notes JSON now.",
            images: &[],
            smart: true,
            thinking: "off",
            grounding: false,
            json: true,
        },
    )
    .await?;
    parse_notes(&text)
}

fn parse_notes(text: &str) -> Result<serde_json::Value, String> {
    let trimmed = text.trim();
    let json = if let Some(fenced) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        fenced.trim().strip_suffix("```").unwrap_or(fenced).trim()
    } else {
        trimmed
    };
    let notes: MeetingNotes = serde_json::from_str(json).map_err(|_| {
        "The model returned invalid meeting notes. Please retry or choose another model."
    })?;
    serde_json::to_value(notes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_notes_and_handles_claude_fences() {
        assert!(parse_notes(r#"{"summary":"ok"}"#).is_err());
        assert!(parse_notes("not JSON").is_err());
        let v = parse_notes("```json\n{\"summary\":\"ok\",\"decisions\":[],\"actionItems\":[{\"text\":\"follow up\"}],\"followUpEmail\":\"hi\"}\n```").unwrap();
        assert_eq!(v["actionItems"][0]["text"], "follow up");
        assert!(v["actionItems"][0].get("owner").is_none());
    }
}
