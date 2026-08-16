use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const KIMI_API_URL: &str = "https://api.moonshot.cn/v1/chat/completions";
const KIMI_MODEL: &str = "kimi-k2.6";

const SYSTEM_PROMPT: &str = r#"You are a football analyst for a Monte Carlo simulation of the 2026-27 Trendyol Süper Lig (Turkish top flight).

The simulation models club strength with Elo ratings on the ClubElo scale. Clubs in this league rate roughly 1500-1780. A 50-point swing changes a club's expected points over a season by roughly 4-6.

When the user describes a scenario (injury, suspension, transfer, manager change, European fixture congestion, etc.), assess its impact on the affected club(s) and return a JSON object of new Elo ratings.

Rules:
- A star player injury (a club's leading scorer or first-choice keeper): 25-60 points.
- A key defender or midfielder injury: 15-40 points.
- A major transfer in: +15 to +50. A major transfer out: -15 to -50.
- A manager change: -30 to +30, depending on the direction described.
- A squad-wide issue (illness, financial crisis, points-deduction scandal): 40-100 points.
- Heavy European fixture congestion for a club also playing in Europe: 10-30 points.
- Multiple compounding effects: add them, capped at 120 points of movement for a single club.
- Use the EXACT club names from this list:
  Galatasaray, Fenerbahçe, Beşiktaş, Amedspor, Trabzonspor, Başakşehir, Göztepe, Samsunspor, Erzurumspor, Gençlerbirliği, Rizespor, Alanyaspor, Çorum, Kocaelispor, Eyüpspor, Konyaspor, Kasımpaşa, Gaziantep
- Return absolute new ratings, not deltas. Keep every value between 1200 and 2000.

Return ONLY valid JSON (no markdown fences) in this format:
{
  "analysis": "brief explanation of the impact",
  "adjustments": {
    "ClubName": new_elo_value_as_float,
    ...
  }
}

If a club mentioned is not in the list, omit it. If no clubs are affected, return empty adjustments."#;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    thinking: ThinkingConfig,
}

#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize, Debug)]
pub struct ScenarioImpact {
    pub analysis: String,
    pub adjustments: HashMap<String, f64>,
}

pub async fn analyze_scenario(prompt: &str, api_key: &str) -> Result<ScenarioImpact> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;
    let req = ChatRequest {
        model: KIMI_MODEL.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            },
        ],
        thinking: ThinkingConfig {
            kind: "disabled".to_string(),
        },
    };

    let resp = client
        .post(KIMI_API_URL)
        .bearer_auth(api_key)
        .json(&req)
        .send()
        .await
        .context("Failed to call Kimi API")?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("Failed to read Kimi response body")?;
    if !status.is_success() {
        anyhow::bail!("Kimi API error {status}: {body}");
    }

    let chat: ChatResponse =
        serde_json::from_str(&body).context("Failed to parse Kimi chat response")?;
    let content = chat
        .choices
        .first()
        .context("No choices in Kimi response")?
        .message
        .content
        .clone();

    let cleaned = strip_fences(&content);
    let impact: ScenarioImpact = serde_json::from_str(&cleaned)
        .context(format!("Failed to parse impact JSON: {cleaned}"))?;
    Ok(impact)
}

fn strip_fences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        let inner = trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        inner.to_string()
    } else {
        trimmed.to_string()
    }
}
