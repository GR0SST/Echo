//! Local payment methods: per-profile preferences and the card rules the
//! Payments settings page applies (Luhn check, brand from the number's
//! prefix, expiration). Card numbers themselves live only in the browser's
//! encrypted store, never in the session.
use serde::{Deserialize, Serialize};

/// The Payments page's switches, kept in each profile's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentSettings {
    /// "Save and fill payment methods".
    pub save_and_fill: bool,
    /// "Verify it's you to autofill payment methods".
    pub verify_before_fill: bool,
    /// "Save security codes".
    pub save_security_codes: bool,
    /// "Allow sites to check if you have payment methods saved".
    pub sites_can_check: bool,
}

impl Default for PaymentSettings {
    fn default() -> Self {
        Self { save_and_fill: true, verify_before_fill: false, save_security_codes: true, sites_can_check: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardBrand {
    Visa,
    Mastercard,
    Amex,
    Discover,
    DinersClub,
    Jcb,
    UnionPay,
    Mir,
    Elo,
    Troy,
    Unknown,
}

impl CardBrand {
    /// The brand a number's leading digits belong to, as Chromium's
    /// autofill detects it.
    pub fn detect(number: &str) -> Self {
        let digits: String = number.chars().filter(char::is_ascii_digit).collect();
        let prefix = |len: usize| digits.get(..len).and_then(|p| p.parse::<u32>().ok());
        let within = |len: usize, low: u32, high: u32| prefix(len).is_some_and(|p| (low..=high).contains(&p));
        if within(6, 431274, 431274) || within(6, 438935, 438935) || within(6, 451416, 451416) || within(6, 457393, 457393)
            || within(6, 504175, 504175) || within(6, 506699, 506778) || within(6, 509000, 509999) || within(6, 627780, 627780)
            || within(6, 636297, 636297) || within(6, 636368, 636368) || within(6, 650031, 650033)
            || within(6, 650035, 650051) || within(6, 650405, 650439) || within(6, 650485, 650538) || within(6, 650541, 650598)
            || within(6, 650700, 650718) || within(6, 650720, 650727) || within(6, 650901, 650920) || within(6, 651652, 651679)
            || within(6, 655000, 655019) || within(6, 655021, 655058)
        {
            return Self::Elo;
        }
        if within(4, 2200, 2204) {
            return Self::Mir;
        }
        if within(4, 9792, 9792) {
            return Self::Troy;
        }
        if within(1, 4, 4) {
            return Self::Visa;
        }
        if within(2, 51, 55) || within(4, 2221, 2720) {
            return Self::Mastercard;
        }
        if within(2, 34, 34) || within(2, 37, 37) {
            return Self::Amex;
        }
        if within(4, 6011, 6011) || within(3, 644, 649) || within(2, 65, 65) {
            return Self::Discover;
        }
        if within(3, 300, 305) || within(2, 36, 36) || within(2, 38, 39) || within(3, 309, 309) {
            return Self::DinersClub;
        }
        if within(4, 3528, 3589) {
            return Self::Jcb;
        }
        if within(2, 62, 62) {
            return Self::UnionPay;
        }
        Self::Unknown
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Visa => "Visa",
            Self::Mastercard => "Mastercard",
            Self::Amex => "American Express",
            Self::Discover => "Discover",
            Self::DinersClub => "Diners Club",
            Self::Jcb => "JCB",
            Self::UnionPay => "UnionPay",
            Self::Mir => "Mir",
            Self::Elo => "Elo",
            Self::Troy => "Troy",
            Self::Unknown => "Card",
        }
    }
    /// Short text for the list's brand badge.
    pub fn badge(self) -> &'static str {
        match self {
            Self::Visa => "VISA",
            Self::Mastercard => "MC",
            Self::Amex => "AMEX",
            Self::Discover => "DISC",
            Self::DinersClub => "DC",
            Self::Jcb => "JCB",
            Self::UnionPay => "UP",
            Self::Mir => "MIR",
            Self::Elo => "ELO",
            Self::Troy => "TROY",
            Self::Unknown => "CARD",
        }
    }
    fn lengths(self) -> &'static [usize] {
        match self {
            Self::Amex => &[15],
            Self::DinersClub => &[14, 15, 16, 17, 18, 19],
            Self::Visa => &[13, 16, 19],
            Self::Mastercard | Self::Elo => &[16],
            Self::UnionPay | Self::Discover | Self::Jcb | Self::Mir | Self::Troy => &[16, 17, 18, 19],
            Self::Unknown => &[12, 13, 14, 15, 16, 17, 18, 19],
        }
    }
    fn cvc_length(self) -> usize {
        if self == Self::Amex { 4 } else { 3 }
    }
}

/// The digits of a card number with spaces and dashes removed, or `None`
/// when it holds anything else.
pub fn card_digits(number: &str) -> Option<String> {
    let mut digits = String::new();
    for c in number.chars() {
        match c {
            '0'..='9' => digits.push(c),
            ' ' | '-' => (),
            _ => return None,
        }
    }
    Some(digits)
}

pub fn luhn(digits: &str) -> bool {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = digits.bytes().rev().enumerate().map(|(i, b)| {
        let d = u32::from(b - b'0');
        if i % 2 == 1 { if d * 2 > 9 { d * 2 - 9 } else { d * 2 } } else { d }
    }).sum();
    sum % 10 == 0
}

/// A valid card number: digits only (after removing separators), a length
/// the brand issues, and a passing Luhn checksum.
pub fn valid_card_number(number: &str) -> bool {
    card_digits(number).is_some_and(|d| CardBrand::detect(&d).lengths().contains(&d.len()) && luhn(&d))
}

/// The card fields as typed in the Add/Edit form.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CardInput {
    pub number: String,
    pub month: u8,
    pub year: u16,
    pub name: String,
    pub nickname: String,
    pub cvc: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardError {
    Number,
    Expiration,
    Cvc,
    Nickname,
}

pub const MAX_NAME: usize = 256;
/// Chromium's limit for card nicknames.
pub const MAX_NICKNAME: usize = 25;

impl CardInput {
    /// Validates against the current `(year, month)`; returns the brand.
    pub fn validate(&self, today: (u16, u8)) -> Result<CardBrand, CardError> {
        if !valid_card_number(&self.number) {
            return Err(CardError::Number);
        }
        let brand = CardBrand::detect(&self.number);
        if !(1..=12).contains(&self.month) || (self.year, self.month) < today || self.year > today.0 + 50 {
            return Err(CardError::Expiration);
        }
        if !self.cvc.is_empty() && (self.cvc.len() != brand.cvc_length() || !self.cvc.bytes().all(|b| b.is_ascii_digit())) {
            return Err(CardError::Cvc);
        }
        if self.nickname.chars().count() > MAX_NICKNAME || self.nickname.chars().any(|c| c.is_ascii_digit())
            || self.name.chars().count() > MAX_NAME
        {
            return Err(CardError::Nickname);
        }
        Ok(brand)
    }
}

/// The list label of a saved card: "Mastercard •• 4248".
pub fn card_label(number: &str, nickname: &str) -> String {
    let digits = card_digits(number).unwrap_or_default();
    let last4 = &digits[digits.len().saturating_sub(4)..];
    let title = if nickname.is_empty() { CardBrand::detect(&digits).name() } else { nickname };
    format!("{title} •• {last4}")
}

/// "MM/YY", as the list shows an expiration date.
pub fn expiration_label(month: u8, year: u16) -> String {
    format!("{month:02}/{:02}", year % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_and_brand_detection_follow_the_issuer_prefixes() {
        for (number, brand) in [
            ("4111 1111 1111 1111", CardBrand::Visa),
            ("5555555555554444", CardBrand::Mastercard),
            ("2223003122003222", CardBrand::Mastercard),
            ("378282246310005", CardBrand::Amex),
            ("6011111111111117", CardBrand::Discover),
            ("3056930009020004", CardBrand::DinersClub),
            ("3566002020360505", CardBrand::Jcb),
            ("6200000000000005", CardBrand::UnionPay),
            ("2200000000000004", CardBrand::Mir),
        ] {
            assert_eq!(CardBrand::detect(number), brand, "{number}");
            assert!(valid_card_number(number), "{number}");
        }
        assert!(!valid_card_number("4111 1111 1111 1112"));
        assert!(!valid_card_number("4111-1111-1111-111a"));
        assert!(!valid_card_number("37828224631000"));
        assert!(!luhn(""));
    }

    #[test]
    fn card_input_validates_expiration_cvc_and_nickname() {
        let mut card = CardInput { number: "5555555555554444".into(), month: 2, year: 2028, ..Default::default() };
        assert_eq!(card.validate((2026, 11)), Ok(CardBrand::Mastercard));
        card.month = 10;
        card.year = 2026;
        assert_eq!(card.validate((2026, 11)), Err(CardError::Expiration));
        card.year = 2027;
        card.cvc = "1234".into();
        assert_eq!(card.validate((2026, 11)), Err(CardError::Cvc));
        card.cvc = "123".into();
        card.nickname = "Card 2".into();
        assert_eq!(card.validate((2026, 11)), Err(CardError::Nickname));
        card.nickname = "Work".into();
        assert!(card.validate((2026, 11)).is_ok());
    }

    #[test]
    fn labels_show_only_the_last_four_digits() {
        assert_eq!(card_label("5555 5555 5555 4248", ""), "Mastercard •• 4248");
        assert_eq!(card_label("4111111111111111", "Work"), "Work •• 1111");
        assert_eq!(expiration_label(2, 2028), "02/28");
    }

    #[test]
    fn older_settings_load_with_payment_defaults() {
        let settings: crate::settings::ProfileSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.payments, PaymentSettings::default());
        assert!(settings.payments.save_and_fill && settings.payments.sites_can_check && !settings.payments.verify_before_fill);
    }
}
