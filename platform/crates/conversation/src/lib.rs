//! Conversations and messages.
//!
//! Conversation memory is not knowledge. Nothing said in a chat is written back
//! into a knowledge base automatically.

pub mod repo;
pub mod service;

pub use repo::{ConversationFilter, Exchange, MessageDetail};
pub use service::ConversationService;

use anthovai_core::{AgentId, ConversationId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug)]
pub struct Conversation {
    pub id: ConversationId,
    pub agent_id: AgentId,
    pub external_user_id: Option<String>,
    pub message_count: i32,
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// The last `turns` exchanges, oldest first. One turn is a user message plus
/// the assistant reply that followed it.
pub fn history_window(messages: &[Message], turns: usize) -> Vec<&Message> {
    if turns == 0 {
        return Vec::new();
    }
    let start = messages.len().saturating_sub(turns * 2);
    messages[start..].iter().collect()
}

impl std::str::FromStr for MessageRole {
    type Err = anthovai_core::DomainError;

    fn from_str(s: &str) -> anthovai_core::Result<Self> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            other => Err(anthovai_core::DomainError::validation(format!(
                "unknown message role `{other}`"
            ))),
        }
    }
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: MessageRole, content: &str) -> Message {
        Message {
            id: MessageId::new(),
            conversation_id: ConversationId::new(),
            role,
            content: content.into(),
            created_at: Utc::now(),
        }
    }

    fn history(pairs: usize) -> Vec<Message> {
        (0..pairs)
            .flat_map(|i| {
                [
                    message(MessageRole::User, &format!("q{i}")),
                    message(MessageRole::Assistant, &format!("a{i}")),
                ]
            })
            .collect()
    }

    #[test]
    fn keeps_the_most_recent_turns() {
        let all = history(10);
        let window = history_window(&all, 2);
        assert_eq!(window.len(), 4);
        assert_eq!(window[0].content, "q8");
        assert_eq!(window[3].content, "a9");
    }

    #[test]
    fn a_short_history_is_returned_whole() {
        let all = history(1);
        assert_eq!(history_window(&all, 6).len(), 2);
    }

    #[test]
    fn zero_turns_means_no_history() {
        let all = history(3);
        assert!(history_window(&all, 0).is_empty());
    }

    #[test]
    fn an_empty_history_is_handled() {
        assert!(history_window(&[], 6).is_empty());
    }
}
