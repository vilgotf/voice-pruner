use twilight_model::application::callback::{CallbackData, InteractionResponse};

/// Emojis used in responses
pub struct Emoji;

impl Emoji {
	pub const WARNING: &'static str = "\u{26A0}\u{FE0F}";
}

/// Markdown / prettier unicode symbols
pub struct Markdown;

impl Markdown {
	pub const BULLET_POINT: &'static str = "\u{2022}";
}

/// Different types of [`InteractionResponse`]s
pub struct Response;

impl Response {
	const BASE: CallbackData = CallbackData {
		allowed_mentions: None,
		content: None,
		embeds: vec![],
		flags: None,
		tts: None,
	};

	pub const fn ack() -> InteractionResponse {
		InteractionResponse::DeferredChannelMessageWithSource(Self::BASE)
	}

	pub fn message(msg: impl Into<String>) -> InteractionResponse {
		let msg = msg.into();
		Self::_message(msg)
	}

	fn _message(msg: String) -> InteractionResponse {
		if msg.is_empty() {
			panic!("empty message is not allowed")
		}

		let mut data = Self::BASE;
		data.content = Some(msg);
		InteractionResponse::ChannelMessageWithSource(data)
	}
}
