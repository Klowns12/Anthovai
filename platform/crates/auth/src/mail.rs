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

/// Writes the message to the log and reports that it did not deliver.
pub struct LoggingMailer;

#[async_trait::async_trait]
impl Mailer for LoggingMailer {
    async fn send(&self, letter: Letter) -> Result<()> {
        tracing::warn!(
            to = %letter.to,
            subject = %letter.subject,
            body = %letter.text,
            "no mail transport is configured, so this message was not sent — \
             the body is logged here so a link can still be followed by hand"
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
