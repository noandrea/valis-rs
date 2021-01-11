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
}

pub fn confirm(question: &str, default: ConfirmAnswer) -> ConfirmAnswer {
    match Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(question)
        .default(default.to_bool())
        .interact()
        .unwrap()
    {
        true => ConfirmAnswer::Yes,
        false => ConfirmAnswer::No,
    }
}
