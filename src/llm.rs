use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const KIMI_API_URL: &str = "https://api.moonshot.cn/v1/chat/completions";
const KIMI_MODEL: &str = "kimi-k2.6";
/// Room for the JSON answer (a short analysis and a few clubs) and no more:
/// a runaway answer costs money and would fail to parse anyway.
const MAX_TOKENS: u32 = 800;
/// The analysis is model output shown to the user; keep it to a paragraph.
pub const MAX_ANALYSIS_CHARS: usize = 600;
/// Upstream detail is logged for diagnosis, but not without bound.
const MAX_LOGGED_CHARS: usize = 2000;

const SYSTEM_PROMPT: &str = r#"You are a football analyst for a Monte Carlo simulation of the 2026-27 Trendyol Süper Lig (Turkish top flight).

The simulation models club strength with Elo-style ratings computed from match results. A 50-point swing changes a club's expected points over a season by roughly 4-6.

When the user describes a scenario (injury, suspension, transfer, manager change, European fixture congestion, etc.), assess its impact on the affected club(s) and return a JSON object of rating changes.

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
- Return rating changes in points (negative weakens a club), not absolute ratings. Keep every change between -150 and +150.

Return ONLY valid JSON (no markdown fences) in this format:
{
  "analysis": "brief explanation of the impact",
  "adjustments": {
    "ClubName": rating_change_as_float,
    ...
  }
}

If a club mentioned is not in the list, omit it. If no clubs are affected, return empty adjustments."#;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    thinking: ThinkingConfig,
    max_tokens: u32,
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

/// Why a scenario could not be analysed. The messages are all the client
/// sees; the detail behind them is logged, never returned, since it can carry
/// provider error bodies or arbitrary model output.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// The API could not be reached or refused the request.
    #[error("The scenario model is unavailable")]
    Unavailable,
    /// The API answered, but not with a usable scenario.
    #[error("The scenario model returned an unusable answer")]
    UnusableAnswer,
}

pub async fn analyze_scenario(prompt: &str, api_key: &str) -> Result<ScenarioImpact, LlmError> {
    let unavailable = |e: anyhow::Error| {
        tracing::warn!("Kimi API unavailable: {e:#}");
        LlmError::Unavailable
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")
        .map_err(unavailable)?;
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
        max_tokens: MAX_TOKENS,
    };

    let resp = client
        .post(KIMI_API_URL)
        .bearer_auth(api_key)
        .json(&req)
        .send()
        .await
        .context("Failed to call Kimi API")
        .map_err(unavailable)?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("Failed to read Kimi response body")
        .map_err(unavailable)?;
    if !status.is_success() {
        tracing::warn!(
            "Kimi API error {status}: {}",
            truncate_chars(&body, MAX_LOGGED_CHARS)
        );
        return Err(LlmError::Unavailable);
    }

    parse_impact(&body).map_err(|e| {
        tracing::warn!(
            "Kimi returned an unusable answer: {}",
            truncate_chars(&format!("{e:#}"), MAX_LOGGED_CHARS)
        );
        LlmError::UnusableAnswer
    })
}

/// The scenario inside a chat-completions response body, with its analysis
/// capped at `MAX_ANALYSIS_CHARS`.
fn parse_impact(body: &str) -> anyhow::Result<ScenarioImpact> {
    let chat: ChatResponse =
        serde_json::from_str(body).context("Failed to parse Kimi chat response")?;
    let content = chat
        .choices
        .first()
        .context("No choices in Kimi response")?
        .message
        .content
        .clone();

    let cleaned = strip_fences(&content);
    let mut impact: ScenarioImpact = serde_json::from_str(&cleaned)
        .with_context(|| format!("Failed to parse impact JSON: {cleaned}"))?;
    impact.analysis = truncate_chars(impact.analysis.trim(), MAX_ANALYSIS_CHARS);
    Ok(impact)
}

/// `s` cut to at most `max` characters (not bytes, so never mid-character),
/// ending in an ellipsis when anything was dropped.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chat_body(content: &str) -> String {
        serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": content } }]
        })
        .to_string()
    }

    #[test]
    fn request_caps_the_answer_length() {
        let req = ChatRequest {
            model: KIMI_MODEL.to_string(),
            messages: Vec::new(),
            thinking: ThinkingConfig {
                kind: "disabled".to_string(),
            },
            max_tokens: MAX_TOKENS,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["max_tokens"], 800);
        assert!(json.get("temperature").is_none());
    }

    #[test]
    fn parses_a_fenced_answer() {
        let body = chat_body(
            "```json\n{\"analysis\": \"Weaker attack.\", \"adjustments\": {\"Galatasaray\": -40}}\n```",
        );
        let impact = parse_impact(&body).unwrap();
        assert_eq!(impact.analysis, "Weaker attack.");
        assert_eq!(impact.adjustments["Galatasaray"], -40.0);
    }

    #[test]
    fn caps_a_long_analysis_on_a_character_boundary() {
        let long = "Beşiktaş Çorum ".repeat(100);
        let content = serde_json::json!({ "analysis": long, "adjustments": {} }).to_string();
        let impact = parse_impact(&chat_body(&content)).unwrap();
        assert_eq!(impact.analysis.chars().count(), MAX_ANALYSIS_CHARS);
        assert!(impact.analysis.ends_with('…'));
        assert!(long.starts_with(impact.analysis.trim_end_matches('…')));
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        assert_eq!(truncate_chars("Çorum", 5), "Çorum");
        assert_eq!(truncate_chars("Çorum FK", 5), "Çoru…");
    }

    #[test]
    fn rejects_an_answer_that_is_not_the_requested_json() {
        assert!(parse_impact(&chat_body("Sorry, I can't help with that.")).is_err());
        assert!(parse_impact("<html>502 Bad Gateway</html>").is_err());
        assert!(parse_impact(r#"{"choices": []}"#).is_err());
    }

    #[test]
    fn client_facing_errors_carry_no_upstream_detail() {
        assert_eq!(
            LlmError::Unavailable.to_string(),
            "The scenario model is unavailable"
        );
        assert_eq!(
            LlmError::UnusableAnswer.to_string(),
            "The scenario model returned an unusable answer"
        );
    }
}
