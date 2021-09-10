//! Collection of unicode symbols.

/// Emojis used in responses
pub struct Emoji;

impl Emoji {
	/// <https://emojipedia.org/warning/>
	pub const WARNING: &'static str = "\u{26A0}\u{FE0F}";
}

/// Markdown / prettier unicode symbols
pub struct Markdown;

impl Markdown {
	pub const BULLET_POINT: &'static str = "\u{2022}";
}
