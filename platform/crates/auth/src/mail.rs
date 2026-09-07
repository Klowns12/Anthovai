//! Sending the one email this platform sends.
//!
//! Deliberately small. A confirmation link is the only message the product has
//! any reason to send today, and a general-purpose mail layer built for one
//! message is a general-purpose mail layer built on one example.
//!
//! Transport is chosen from configuration at startup. When none is configured
//! the link is written to the log instead, which is what makes local
//! development and a first deploy possible without a mail provider — and which
//! is announced loudly, because a platform silently not sending its only email
//! looks exactly like one that is.

use std::sync::Arc;

use anthovai_core::{DomainError, Result};
use lettre::message::{header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[derive(Clone, Debug)]
pub struct MailSettings {
    /// `smtp.example.com:587`. Absent means nothing is sent.
    pub smtp_url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    /// The From address. Must be one the provider will accept.
    pub from: String,
}

/// What a message needs to reach somebody.
pub struct Letter {
    pub to: String,
    pub subject: String,
    pub text: String,
}

#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, letter: Letter) -> Result<()>;
    /// Whether anything actually leaves the process. The dashboard tells a
    /// customer to check their inbox, and it should not say that when nothing
    /// was sent.
    fn delivers(&self) -> bool;
}

/// Writes enough to the log to follow the link by hand, and no more.
///
/// The first version logged the recipient, the subject and the whole body. Two
/// things wrong with that: an address is personal data, and the body carries a
/// single-use credential — so an ordinary log became a place where both sat in
/// plain text, retained for as long as logs are retained and readable by
/// anybody who can read them.
///
/// What survives is the link, because without it this fallback is useless, and
/// the address with its local part masked, because the operator still has to
/// know whose link it is. That is the least that does the job.
pub struct LoggingMailer;

/// `somchai@example.com` becomes `s***@example.com`.
///
/// Enough to recognise an address you were expecting; not enough to harvest one
/// you were not.
fn mask(address: &str) -> String {
    match address.split_once('@') {
        Some((local, domain)) => {
            let first = local.chars().next().unwrap_or('*');
            format!("{first}***@{domain}")
        }
        // Not an address shape. Say nothing about it rather than guess.
        None => "***".to_owned(),
    }
}

/// The URL out of the message, so the body does not have to be logged whole.
fn first_link(text: &str) -> Option<&str> {
    let start = text.find("http")?;
    let rest = &text[start..];
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some(&rest[..end])
}

#[async_trait::async_trait]
impl Mailer for LoggingMailer {
    async fn send(&self, letter: Letter) -> Result<()> {
        tracing::warn!(
            to = %mask(&letter.to),
            link = first_link(&letter.text).unwrap_or("(none in this message)"),
            "no mail transport is configured, so this was not sent. The link is \
             a single-use credential and it is in this log — configure \
             ANTHOVAI__MAIL__SMTP_URL before this runs anywhere real."
        );
        Ok(())
    }

    fn delivers(&self) -> bool {
        false
    }
}

pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

#[async_trait::async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, letter: Letter) -> Result<()> {
        let to: Mailbox = letter
            .to
            .parse()
            .map_err(|e| DomainError::rejected("email_invalid", format!("{e}")))?;

        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(letter.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(letter.text)
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("could not build message: {e}")))?;

        self.transport
            .send(message)
            .await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("could not send message: {e}")))?;

        Ok(())
    }

    fn delivers(&self) -> bool {
        true
    }
}

/// Build the mailer a deployment asked for.
///
/// A configuration that names a server but no credentials is accepted: an
/// internal relay that authenticates by network position is a normal thing to
/// have, and refusing it would push people towards putting a password where one
/// is not needed.
pub fn from_settings(settings: &MailSettings) -> Result<Arc<dyn Mailer>> {
    let Some(url) = settings.smtp_url.as_deref().filter(|u| !u.is_empty()) else {
        tracing::warn!(
            "no SMTP server configured: confirmation emails will be logged, not sent. \
             Set ANTHOVAI__MAIL__SMTP_URL to change that."
        );
        return Ok(Arc::new(LoggingMailer));
    };

    let from: Mailbox = settings.from.parse().map_err(|e| {
        DomainError::Internal(anyhow::anyhow!(
            "`{}` is not a usable From address: {e}",
            settings.from
        ))
    })?;

    // STARTTLS rather than implicit TLS: it is what port 587 speaks, which is
    // what nearly every provider documents.
    let mut builder =
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(url.split(':').next().unwrap_or(url))
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("SMTP relay `{url}`: {e}")))?;

    if let Some(port) = url.split(':').nth(1).and_then(|p| p.parse::<u16>().ok()) {
        builder = builder.port(port);
    }

    if let (Some(user), Some(password)) = (&settings.username, &settings.password) {
        builder = builder.credentials(Credentials::new(user.clone(), password.clone()));
    }

    tracing::info!(server = %url, from = %settings.from, "SMTP ready");
    Ok(Arc::new(SmtpMailer {
        transport: builder.build(),
        from,
    }))
}

/// The confirmation message.
///
/// Plain text on purpose: it renders everywhere, cannot carry a tracking pixel,
/// and a link a person can read is a link they can judge before clicking.
pub fn confirmation_letter(to: &str, link: &str) -> Letter {
    Letter {
        to: to.to_owned(),
        subject: "Confirm your Anthovai address".to_owned(),
        text: format!(
            "Confirm this address to finish setting up your Anthovai account:\n\
             \n\
             {link}\n\
             \n\
             The link works once and expires in 24 hours.\n\
             \n\
             If you did not create an account, nothing has been set up and you\n\
             can ignore this message.\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_keeps_the_domain_and_one_letter() {
        assert_eq!(mask("somchai@example.com"), "s***@example.com");
        assert_eq!(mask("a@b.co"), "a***@b.co");
    }

    #[test]
    fn something_that_is_not_an_address_reveals_nothing() {
        // Better to say nothing than to log a string of unknown provenance in
        // the belief that it was an address.
        assert_eq!(mask("not-an-address"), "***");
        assert_eq!(mask(""), "***");
    }

    #[test]
    fn the_link_comes_out_without_the_rest_of_the_message() {
        let letter = confirmation_letter("somchai@example.com", "https://x.test/verify?token=abc");
        let link = first_link(&letter.text).expect("the letter contains a link");
        assert_eq!(link, "https://x.test/verify?token=abc");
        assert!(!link.contains(char::is_whitespace));
    }

    #[test]
    fn a_message_with_no_link_is_not_a_panic() {
        assert_eq!(first_link("nothing to follow here"), None);
    }

    #[test]
    fn the_confirmation_letter_says_what_the_link_is_for() {
        let letter = confirmation_letter("a@b.test", "https://x.test/verify?token=t");
        assert!(letter.text.contains("https://x.test/verify?token=t"));
        // A recipient who did not ask for an account should be told that
        // ignoring this leaves nothing behind.
        assert!(letter.text.contains("ignore"));
        assert_eq!(letter.to, "a@b.test");
    }
}
