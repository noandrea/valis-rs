use dialoguer::{theme::ColorfulTheme, Confirm};

pub enum ConfirmAnswer {
    Yes,
    No,
}

impl ConfirmAnswer {
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Yes => true,
            Self::No => false,
        }
    }

    pub fn from_bool(v: bool) -> ConfirmAnswer {
        match v {
            true => Self::Yes,
            false => Self::No,
        }
    }
}

pub fn confirm(question: &str, default: ConfirmAnswer) -> ConfirmAnswer {
    ConfirmAnswer::from_bool(
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(question)
            .default(default.to_bool())
            .interact()
            .unwrap(),
    )
}
